//! Type service implementation for gRPC
//! 
//! Implements the type system service API.

use crate::core::{SharedUniverse, TypeId};
use crate::query::QueryEngine;
use crate::query::engine::{QueryResult, TypeConstraint};
use crate::agent::{AgentCoordinator, AgentId, SessionId, AgentType, ConnectionRequest};
use crate::validate::{StreamingChecker, ValidationStream};

use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

/// Type service handler
pub struct TypeService {
    universe: SharedUniverse,
    coordinator: Arc<AgentCoordinator>,
}

impl TypeService {
    pub fn new(universe: SharedUniverse, coordinator: Arc<AgentCoordinator>) -> Self {
        Self {
            universe,
            coordinator,
        }
    }
    
    /// Get query engine for a session
    fn get_query_engine(&self, session_id: SessionId) -> Option<QueryEngine> {
        self.coordinator.get_session(session_id)
            .map(|session| {
                // Would get query engine from session
                QueryEngine::new(self.universe.clone())
            })
    }
}

/// Connect request
#[derive(Debug, Clone)]
pub struct ConnectRequest {
    pub agent_name: String,
    pub agent_type: String,
}

/// Connect response
#[derive(Debug, Clone)]
pub struct ConnectResponse {
    pub session_id: String,
    pub success: bool,
    pub message: String,
}

/// Type query request
#[derive(Debug, Clone)]
pub struct TypeQueryRequest {
    pub session_id: String,
    pub query: TypeQuery,
}

#[derive(Debug, Clone)]
pub enum TypeQuery {
    ById { type_id: u64 },
    ByName { package: String, name: String },
    Similar { type_id: u64, threshold: f32 },
    Implements { interface_id: u64 },
    Pattern { pattern: String },
}

/// Type query response
#[derive(Debug, Clone)]
pub struct TypeQueryResponse {
    pub results: Vec<TypeResult>,
    pub latency_us: u64,
}

/// Single type result
#[derive(Debug, Clone)]
pub struct TypeResult {
    pub type_id: u64,
    pub name: String,
    pub kind: String,
    pub score: f32,
    pub json_representation: String,
}

/// Validate request
#[derive(Debug, Clone)]
pub struct ValidateRequest {
    pub session_id: String,
    pub expression: String,
    pub expected_type: Option<u64>,
    pub context: ValidationContext,
}

#[derive(Debug, Clone, Default)]
pub struct ValidationContext {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Validate response
#[derive(Debug, Clone)]
pub struct ValidateResponse {
    pub valid: bool,
    pub inferred_type: Option<u64>,
    pub errors: Vec<ValidationError>,
    pub latency_us: u64,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub severity: String,
    pub suggestion: Option<String>,
}

/// Stream validate request (for AI token streaming)
#[derive(Debug, Clone)]
pub struct StreamValidateRequest {
    pub session_id: String,
    pub tokens: Vec<String>,
    pub context: ValidationContext,
}

impl TypeService {
    /// Handle connect request
    pub async fn connect(&self, request: ConnectRequest) -> Result<ConnectResponse, Status> {
        let agent_type = match request.agent_type.as_str() {
            "cursor" => AgentType::Cursor,
            "claude_code" => AgentType::ClaudeCode,
            "gemini_cli" => AgentType::GeminiCLI,
            "github_copilot" => AgentType::GitHubCopilot,
            _ => AgentType::Generic,
        };
        
        let conn_request = ConnectionRequest {
            agent_id: AgentId::new(rand::random()),
            name: request.agent_name,
            agent_type,
            preferred_isolation: None,
        };
        
        match self.coordinator.connect(conn_request).await {
            super::super::agent::coordinator::ConnectionResult::Connected { session_id } => {
                Ok(ConnectResponse {
                    session_id: format!("{}", session_id.0),
                    success: true,
                    message: "Connected successfully".to_string(),
                })
            }
            super::super::agent::coordinator::ConnectionResult::Rejected { reason } => {
                Ok(ConnectResponse {
                    session_id: String::new(),
                    success: false,
                    message: format!("Connection rejected: {:?}", reason),
                })
            }
        }
    }
    
    /// Handle type query
    pub async fn query_types(&self, request: TypeQueryRequest) -> Result<TypeQueryResponse, Status> {
        let start = std::time::Instant::now();
        
        let session_id = SessionId(
            uuid::Uuid::parse_str(&request.session_id)
                .map_err(|_| Status::invalid_argument("Invalid session ID"))?
        );
        
        let query_engine = self.get_query_engine(session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;
        
        let results = match request.query {
            TypeQuery::ById { type_id } => {
                query_engine.get_type(TypeId(type_id))
                    .into_iter()
                    .map(|t| type_to_result(&t, 1.0))
                    .collect()
            }
            TypeQuery::ByName { package, name } => {
                // Would look up by symbol
                vec![]
            }
            TypeQuery::Similar { type_id, threshold } => {
                query_engine.find_similar(TypeId(type_id), threshold)
                    .into_iter()
                    .map(|r| QueryResult {
                        item: r.item,
                        score: r.score,
                        match_details: r.match_details,
                    })
                    .map(|r| type_to_result(
                        &query_engine.get_type(r.item).unwrap(),
                        r.score
                    ))
                    .collect()
            }
            TypeQuery::Implements { interface_id } => {
                query_engine.find_implementors(TypeId(interface_id))
                    .into_iter()
                    .map(|r| type_to_result(
                        &query_engine.get_type(r.item).unwrap(),
                        r.score
                    ))
                    .collect()
            }
            TypeQuery::Pattern { pattern: _ } => {
                // Pattern-based search
                vec![]
            }
        };
        
        let latency = start.elapsed().as_micros() as u64;
        
        Ok(TypeQueryResponse {
            results,
            latency_us: latency,
        })
    }
    
    /// Handle validate request
    pub async fn validate(&self, request: ValidateRequest) -> Result<ValidateResponse, Status> {
        let start = std::time::Instant::now();
        
        let session_id = SessionId(
            uuid::Uuid::parse_str(&request.session_id)
                .map_err(|_| Status::invalid_argument("Invalid session ID"))?
        );
        
        // Get session's checker
        let session = self.coordinator.get_session(session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;
        
        // Parse expression and validate
        // Would use streaming checker
        
        let latency = start.elapsed().as_micros() as u64;
        
        Ok(ValidateResponse {
            valid: true,
            inferred_type: None,
            errors: vec![],
            latency_us: latency,
        })
    }
    
    /// Handle streaming validation
    pub async fn stream_validate(
        &self,
        request: StreamValidateRequest,
        tx: mpsc::Sender<StreamValidateResponse>,
    ) -> Result<(), Status> {
        // Process tokens incrementally
        for token in request.tokens {
            // Validate token
            let response = StreamValidateResponse {
                token: token.clone(),
                valid: true,
                inferred_type: None,
                suggestions: vec![],
            };
            
            if tx.send(response).await.is_err() {
                break;
            }
        }
        
        Ok(())
    }
}

/// Streaming validation response
#[derive(Debug, Clone)]
pub struct StreamValidateResponse {
    pub token: String,
    pub valid: bool,
    pub inferred_type: Option<u64>,
    pub suggestions: Vec<String>,
}

/// Convert type to result
fn type_to_result(typ: &crate::core::Type, score: f32) -> TypeResult {
    TypeResult {
        type_id: typ.id.0,
        name: format!("{:?}", typ.kind), // Would extract proper name
        kind: format!("{:?}", typ.flags),
        score,
        json_representation: serde_json::to_string(typ).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    fn setup_service() -> TypeService {
        let universe = Arc::new(TypeUniverse::new());
        let coordinator = Arc::new(AgentCoordinator::new(universe.clone()));
        TypeService::new(universe, coordinator)
    }

    #[tokio::test]
    async fn test_connect() {
        let service = setup_service();
        
        let request = ConnectRequest {
            agent_name: "Test".to_string(),
            agent_type: "cursor".to_string(),
        };
        
        let response = service.connect(request).await;
        assert!(response.is_ok());
    }
}
