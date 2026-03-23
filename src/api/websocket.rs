//! WebSocket server for real-time streaming
//!
//! Provides WebSocket endpoint for streaming validation and queries.

use axum::{
    extract::{ws::Message, ws::WebSocket, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug, error, warn};

use crate::core::SharedUniverse;
use crate::agent::AgentCoordinator;
use super::service::TypeService;

/// WebSocket server state
#[derive(Clone)]
pub struct WebSocketState {
    pub universe: SharedUniverse,
    pub coordinator: Arc<AgentCoordinator>,
    pub service: Arc<RwLock<TypeService>>,
}

/// WebSocket server
pub struct WebSocketServer {
    bind_address: SocketAddr,
    state: WebSocketState,
}

impl WebSocketServer {
    pub fn new(
        bind_address: SocketAddr,
        universe: SharedUniverse,
        coordinator: Arc<AgentCoordinator>,
    ) -> Self {
        let service = Arc::new(RwLock::new(TypeService::new(
            universe.clone(),
            coordinator.clone(),
        )));
        
        let state = WebSocketState {
            universe,
            coordinator,
            service,
        };
        
        Self {
            bind_address,
            state,
        }
    }

    /// Start the WebSocket server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/health", get(health_handler))
            .with_state(self.state.clone());

        let listener = tokio::net::TcpListener::bind(&self.bind_address).await?;
        
        info!("🌐 WebSocket server listening on ws://{}", self.bind_address);

        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// WebSocket request
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
#[serde(rename_all = "camelCase")]
pub enum WsRequest {
    #[serde(rename = "health")]
    Health,
    #[serde(rename = "connect")]
    Connect(WsConnectRequest),
    #[serde(rename = "query")]
    Query(WsQueryRequest),
    #[serde(rename = "validate")]
    Validate(WsValidateRequest),
    #[serde(rename = "streamValidate")]
    StreamValidate(WsStreamValidateRequest),
    #[serde(rename = "import")]
    Import(WsImportRequest),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WsConnectRequest {
    pub agent_name: String,
    pub agent_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WsQueryRequest {
    pub session_id: String,
    #[serde(flatten)]
    pub query: WsQuery,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsQuery {
    #[serde(rename = "byId")]
    ById { type_id: u64 },
    #[serde(rename = "byName")]
    ByName { package: String, name: String },
    #[serde(rename = "similar")]
    Similar { type_id: u64, threshold: f32 },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WsValidateRequest {
    pub session_id: String,
    pub expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_type_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WsStreamValidateRequest {
    pub session_id: String,
    pub token: String,
    pub is_complete: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WsImportRequest {
    pub session_id: String,
    pub package_path: String,
}

/// WebSocket response
#[derive(Debug, Serialize, Deserialize)]
pub struct WsResponse {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WsError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WsError {
    pub code: i32,
    pub message: String,
}

/// Health response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub version: String,
}

async fn health_handler() -> impl IntoResponse {
    axum::Json(HealthResponse {
        healthy: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WebSocketState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: WebSocketState) {
    let (mut sender, mut receiver) = socket.split();

    info!("WebSocket connection established");

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                debug!("Received: {}", text);

                match serde_json::from_str::<WsRequest>(&text) {
                    Ok(request) => {
                        let response = handle_request(request, &state).await;
                        let response_text = serde_json::to_string(&response).unwrap_or_default();

                        if sender.send(Message::Text(response_text)).await.is_err() {
                            warn!("Failed to send response");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Invalid request: {}", e);
                        let error_response = WsResponse {
                            method: "error".to_string(),
                            result: None,
                            error: Some(WsError {
                                code: 400,
                                message: format!("Invalid request: {}", e),
                            }),
                        };
                        let response_text = serde_json::to_string(&error_response).unwrap_or_default();
                        let _ = sender.send(Message::Text(response_text)).await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket connection closed");
                break;
            }
            Ok(Message::Ping(data)) => {
                if sender.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}

async fn handle_request(request: WsRequest, state: &WebSocketState) -> WsResponse {
    match request {
        WsRequest::Health => {
            WsResponse {
                method: "health".to_string(),
                result: serde_json::to_value(HealthResponse {
                    healthy: true,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                }).ok(),
                error: None,
            }
        }

        WsRequest::Connect(req) => {
            info!("Agent connecting via WebSocket: {}", req.agent_name);
            
            let inner_req = super::service::ConnectRequest {
                agent_name: req.agent_name,
                agent_type: req.agent_type,
            };

            let service = state.service.read().await;
            match service.connect(inner_req).await {
                Ok(response) => {
                    WsResponse {
                        method: "connect".to_string(),
                        result: serde_json::to_value(serde_json::json!({
                            "success": response.success,
                            "sessionId": response.session_id,
                            "message": response.message,
                        })).ok(),
                        error: None,
                    }
                }
                Err(e) => {
                    WsResponse {
                        method: "connect".to_string(),
                        result: None,
                        error: Some(WsError {
                            code: 500,
                            message: e.to_string(),
                        }),
                    }
                }
            }
        }

        WsRequest::Query(req) => {
            let query = match req.query {
                WsQuery::ById { type_id } => {
                    super::service::TypeQuery::ById { type_id }
                }
                WsQuery::ByName { package, name } => {
                    super::service::TypeQuery::ByName { package, name }
                }
                WsQuery::Similar { type_id, threshold } => {
                    super::service::TypeQuery::Similar { type_id, threshold }
                }
            };

            let inner_req = super::service::TypeQueryRequest {
                session_id: req.session_id,
                query,
            };

            let service = state.service.read().await;
            match service.query_types(inner_req).await {
                Ok(response) => {
                    WsResponse {
                        method: "query".to_string(),
                        result: serde_json::to_value(serde_json::json!({
                            "types": response.results,
                            "latencyMicros": response.latency_us,
                            "totalCount": response.results.len(),
                        })).ok(),
                        error: None,
                    }
                }
                Err(e) => {
                    WsResponse {
                        method: "query".to_string(),
                        result: None,
                        error: Some(WsError {
                            code: 500,
                            message: e.to_string(),
                        }),
                    }
                }
            }
        }

        WsRequest::Validate(req) => {
            let inner_req = super::service::ValidateRequest {
                session_id: req.session_id,
                expression: req.expression,
                expected_type: req.expected_type_id,
                context: super::service::ValidationContext::default(),
            };

            let service = state.service.read().await;
            match service.validate(inner_req).await {
                Ok(response) => {
                    WsResponse {
                        method: "validate".to_string(),
                        result: serde_json::to_value(serde_json::json!({
                            "valid": response.valid,
                            "inferredType": response.inferred_type,
                            "errors": response.errors,
                            "latencyMicros": response.latency_us,
                        })).ok(),
                        error: None,
                    }
                }
                Err(e) => {
                    WsResponse {
                        method: "validate".to_string(),
                        result: None,
                        error: Some(WsError {
                            code: 500,
                            message: e.to_string(),
                        }),
                    }
                }
            }
        }

        WsRequest::StreamValidate(req) => {
            // For WebSocket, we respond synchronously
            WsResponse {
                method: "streamValidate".to_string(),
                result: serde_json::to_value(serde_json::json!({
                    "token": req.token,
                    "valid": true,
                    "isFinal": req.is_complete,
                })).ok(),
                error: None,
            }
        }

        WsRequest::Import(req) => {
            info!("Importing package via WebSocket: {}", req.package_path);
            
            WsResponse {
                method: "import".to_string(),
                result: serde_json::to_value(serde_json::json!({
                    "success": true,
                    "packagePath": req.package_path,
                    "typesImported": 0,
                })).ok(),
                error: None,
            }
        }
    }
}
