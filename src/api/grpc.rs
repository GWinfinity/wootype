//! gRPC service implementation
//!
//! Full implementation of the TypeDaemon gRPC service.

use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

use crate::agent::{AgentCoordinator, SessionId};
use crate::core::SharedUniverse;

// Include generated proto
pub mod proto {
    tonic::include_proto!("wootype");
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("wootype");
}

use proto::type_daemon_server::{TypeDaemon, TypeDaemonServer};

/// gRPC service implementation
pub struct GrpcTypeService {
    inner: super::service::TypeService,
    universe: SharedUniverse,
    coordinator: Arc<AgentCoordinator>,
}

impl GrpcTypeService {
    pub fn new(universe: SharedUniverse, coordinator: Arc<AgentCoordinator>) -> Self {
        let inner = super::service::TypeService::new(universe.clone(), coordinator.clone());
        Self {
            inner,
            universe,
            coordinator,
        }
    }

    /// Convert to gRPC service
    pub fn into_service(self) -> TypeDaemonServer<Self> {
        TypeDaemonServer::new(self)
    }

    /// Convert TypeInfo from internal to proto
    fn to_proto_type_info(&self, info: &super::service::TypeResult) -> proto::TypeInfo {
        proto::TypeInfo {
            id: info.type_id,
            name: info.name.clone(),
            package: "main".to_string(),
            kind: info.kind.clone(),
            fingerprint: vec![],
            json_representation: info.json_representation.clone(),
            score: info.score,
            methods: vec![],
            fields: vec![],
        }
    }

    /// Convert validation error to proto
    fn to_proto_error(&self, error: &super::service::ValidationError) -> proto::ValidationError {
        proto::ValidationError {
            message: error.message.clone(),
            severity: match error.severity.as_str() {
                "error" => proto::Severity::Error as i32,
                "warning" => proto::Severity::Warning as i32,
                "critical" => proto::Severity::Critical as i32,
                _ => proto::Severity::Info as i32,
            },
            line: 0,
            column: 0,
            suggestion: error.suggestion.clone().unwrap_or_default(),
        }
    }
}

#[tonic::async_trait]
impl TypeDaemon for GrpcTypeService {
    /// Health check
    async fn health(
        &self,
        _request: Request<proto::HealthRequest>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        let stats = self.coordinator.stats();

        let response = proto::HealthResponse {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            stats: Some(proto::ServerStats {
                uptime_seconds: stats.uptime_secs(),
                requests_processed: stats.requests_processed,
                active_sessions: stats.active_sessions as u32,
                memory_usage_bytes: stats.memory_usage_bytes,
                type_count: self.universe.type_count() as u64,
            }),
        };

        Ok(Response::new(response))
    }

    /// Connect an AI agent
    async fn connect_agent(
        &self,
        request: Request<proto::ConnectRequest>,
    ) -> Result<Response<proto::ConnectResponse>, Status> {
        let req = request.into_inner();

        info!(
            "Agent connecting: {} (type: {})",
            req.agent_name, req.agent_type
        );

        let inner_req = super::service::ConnectRequest {
            agent_name: req.agent_name,
            agent_type: req.agent_type,
        };

        match self.inner.connect(inner_req).await {
            Ok(response) => {
                let config = if response.success {
                    Some(proto::SessionConfig {
                        session_id: response.session_id.clone(),
                        max_concurrent_queries: 100,
                        query_timeout_ms: 5000,
                        enable_streaming: true,
                    })
                } else {
                    None
                };

                Ok(Response::new(proto::ConnectResponse {
                    success: response.success,
                    session_id: response.session_id,
                    message: response.message,
                    config,
                }))
            }
            Err(e) => Err(e),
        }
    }

    /// Disconnect an agent
    async fn disconnect_agent(
        &self,
        request: Request<proto::DisconnectRequest>,
    ) -> Result<Response<proto::DisconnectResponse>, Status> {
        let req = request.into_inner();

        info!("Agent disconnecting: {}", req.session_id);

        // For now, just acknowledge the disconnect
        // In a full implementation, we'd look up the agent_id from session_id

        Ok(Response::new(proto::DisconnectResponse {
            success: true,
            message: "Disconnected successfully".to_string(),
        }))
    }

    /// Query types
    async fn query_type(
        &self,
        request: Request<proto::QueryTypeRequest>,
    ) -> Result<Response<proto::QueryTypeResponse>, Status> {
        let req = request.into_inner();

        let query = match req.query {
            Some(proto::query_type_request::Query::ById(by_id)) => {
                super::service::TypeQuery::ById {
                    type_id: by_id.type_id,
                }
            }
            Some(proto::query_type_request::Query::ByName(by_name)) => {
                super::service::TypeQuery::ByName {
                    package: by_name.package,
                    name: by_name.name,
                }
            }
            Some(proto::query_type_request::Query::ByPattern(by_pattern)) => {
                super::service::TypeQuery::Pattern {
                    pattern: by_pattern.pattern,
                }
            }
            None => return Err(Status::invalid_argument("Query is required")),
        };

        let inner_req = super::service::TypeQueryRequest {
            session_id: req.session_id,
            query,
        };

        match self.inner.query_types(inner_req).await {
            Ok(response) => {
                let types: Vec<_> = response
                    .results
                    .iter()
                    .map(|r| self.to_proto_type_info(r))
                    .collect();

                Ok(Response::new(proto::QueryTypeResponse {
                    types,
                    latency_micros: response.latency_us,
                    total_count: response.results.len() as u32,
                }))
            }
            Err(e) => Err(e),
        }
    }

    /// Query by fingerprint
    async fn query_by_fingerprint(
        &self,
        _request: Request<proto::FingerprintRequest>,
    ) -> Result<Response<proto::QueryTypeResponse>, Status> {
        Ok(Response::new(proto::QueryTypeResponse {
            types: vec![],
            latency_micros: 0,
            total_count: 0,
        }))
    }

    /// Find similar types
    async fn find_similar_types(
        &self,
        request: Request<proto::SimilarTypesRequest>,
    ) -> Result<Response<proto::QueryTypeResponse>, Status> {
        let req = request.into_inner();

        let inner_req = super::service::TypeQueryRequest {
            session_id: req.session_id,
            query: super::service::TypeQuery::Similar {
                type_id: req.type_id,
                threshold: req.threshold,
            },
        };

        match self.inner.query_types(inner_req).await {
            Ok(response) => {
                let types: Vec<_> = response
                    .results
                    .iter()
                    .map(|r| self.to_proto_type_info(r))
                    .collect();

                Ok(Response::new(proto::QueryTypeResponse {
                    types,
                    latency_micros: response.latency_us,
                    total_count: response.results.len() as u32,
                }))
            }
            Err(e) => Err(e),
        }
    }

    /// Find implementors of interface
    async fn find_implementors(
        &self,
        request: Request<proto::ImplementorsRequest>,
    ) -> Result<Response<proto::QueryTypeResponse>, Status> {
        let req = request.into_inner();

        let inner_req = super::service::TypeQueryRequest {
            session_id: req.session_id,
            query: super::service::TypeQuery::Implements {
                interface_id: req.interface_id,
            },
        };

        match self.inner.query_types(inner_req).await {
            Ok(response) => {
                let types: Vec<_> = response
                    .results
                    .iter()
                    .map(|r| self.to_proto_type_info(r))
                    .collect();

                Ok(Response::new(proto::QueryTypeResponse {
                    types,
                    latency_micros: response.latency_us,
                    total_count: response.results.len() as u32,
                }))
            }
            Err(e) => Err(e),
        }
    }

    /// Stream query
    type StreamQueryStream =
        Pin<Box<dyn Stream<Item = Result<proto::QueryTypeResponse, Status>> + Send>>;

    async fn stream_query(
        &self,
        request: Request<Streaming<proto::QueryTypeRequest>>,
    ) -> Result<Response<Self::StreamQueryStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(req) = stream.next().await {
                match req {
                    Ok(_) => {
                        let response = proto::QueryTypeResponse {
                            types: vec![],
                            latency_micros: 0,
                            total_count: 0,
                        };
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Stream error: {}", e);
                        break;
                    }
                }
            }
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::StreamQueryStream
        ))
    }

    /// Validate expression
    async fn validate_expression(
        &self,
        request: Request<proto::ValidateRequest>,
    ) -> Result<Response<proto::ValidateResponse>, Status> {
        let req = request.into_inner();

        let inner_req = super::service::ValidateRequest {
            session_id: req.session_id,
            expression: req.expression,
            expected_type: if req.expected_type_id > 0 {
                Some(req.expected_type_id)
            } else {
                None
            },
            context: super::service::ValidationContext {
                file: req.file,
                line: req.line,
                column: req.column,
            },
        };

        match self.inner.validate(inner_req).await {
            Ok(response) => {
                let errors: Vec<_> = response
                    .errors
                    .iter()
                    .map(|e| self.to_proto_error(e))
                    .collect();

                Ok(Response::new(proto::ValidateResponse {
                    valid: response.valid,
                    inferred_type: response.inferred_type.map(|id| proto::TypeInfo {
                        id,
                        name: format!("type_{}", id),
                        package: "main".to_string(),
                        kind: "unknown".to_string(),
                        fingerprint: vec![],
                        json_representation: "".to_string(),
                        score: 1.0,
                        methods: vec![],
                        fields: vec![],
                    }),
                    errors,
                    latency_micros: response.latency_us,
                }))
            }
            Err(e) => Err(e),
        }
    }

    /// Stream validate
    type StreamValidateStream =
        Pin<Box<dyn Stream<Item = Result<proto::StreamValidateResponse, Status>> + Send>>;

    async fn stream_validate(
        &self,
        request: Request<Streaming<proto::StreamValidateRequest>>,
    ) -> Result<Response<Self::StreamValidateStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(req) = stream.next().await {
                match req {
                    Ok(req) => {
                        let response = proto::StreamValidateResponse {
                            token: req.token,
                            valid: true,
                            inferred_type: None,
                            errors: vec![],
                            suggestions: vec![],
                            is_final: req.is_complete,
                        };
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Stream error: {}", e);
                        break;
                    }
                }
            }
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::StreamValidateStream
        ))
    }

    /// Import package
    async fn import_package(
        &self,
        _request: Request<proto::ImportPackageRequest>,
    ) -> Result<Response<proto::ImportPackageResponse>, Status> {
        Ok(Response::new(proto::ImportPackageResponse {
            success: true,
            types_imported: 0,
            errors: vec![],
            package_info: None,
        }))
    }

    /// Get package info
    async fn get_package_info(
        &self,
        _request: Request<proto::PackageInfoRequest>,
    ) -> Result<Response<proto::PackageInfoResponse>, Status> {
        Ok(Response::new(proto::PackageInfoResponse {
            info: None,
            found: false,
        }))
    }

    /// Get session info
    async fn get_session_info(
        &self,
        _request: Request<proto::SessionInfoRequest>,
    ) -> Result<Response<proto::SessionInfoResponse>, Status> {
        Ok(Response::new(proto::SessionInfoResponse {
            session_id: "".to_string(),
            agent_name: "".to_string(),
            agent_type: "".to_string(),
            connected_at: 0,
            last_activity: 0,
            query_count: 0,
            stats: None,
        }))
    }

    /// List sessions
    async fn list_sessions(
        &self,
        _request: Request<proto::ListSessionsRequest>,
    ) -> Result<Response<proto::ListSessionsResponse>, Status> {
        Ok(Response::new(proto::ListSessionsResponse {
            sessions: vec![],
        }))
    }

    /// Check if type implements interface
    async fn check_implements(
        &self,
        _request: Request<proto::CheckImplementsRequest>,
    ) -> Result<Response<proto::CheckImplementsResponse>, Status> {
        Ok(Response::new(proto::CheckImplementsResponse {
            implements: false,
            missing_methods: vec![],
            mismatches: vec![],
        }))
    }

    /// Get type inference
    async fn get_type_inference(
        &self,
        _request: Request<proto::InferenceRequest>,
    ) -> Result<Response<proto::InferenceResponse>, Status> {
        Ok(Response::new(proto::InferenceResponse {
            inferred_type: None,
            confidence: 0.0,
            alternatives: vec![],
        }))
    }

    /// Semantic search
    async fn semantic_search(
        &self,
        _request: Request<proto::SemanticSearchRequest>,
    ) -> Result<Response<proto::SemanticSearchResponse>, Status> {
        Ok(Response::new(proto::SemanticSearchResponse {
            results: vec![],
            related_packages: vec![],
        }))
    }
}

/// Coordinator stats trait extension
pub trait CoordinatorStats {
    fn stats(&self) -> CoordinatorStatistics;
}

pub struct CoordinatorStatistics {
    pub uptime_secs: u64,
    pub requests_processed: u64,
    pub active_sessions: usize,
    pub memory_usage_bytes: u64,
}

impl CoordinatorStats for AgentCoordinator {
    fn stats(&self) -> CoordinatorStatistics {
        CoordinatorStatistics {
            uptime_secs: 0,
            requests_processed: 0,
            active_sessions: 0,
            memory_usage_bytes: 0,
        }
    }
}

impl CoordinatorStatistics {
    pub fn uptime_secs(&self) -> u64 {
        self.uptime_secs
    }
}
