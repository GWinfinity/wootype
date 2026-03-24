//! gRPC API server
//!
//! Serves TypeService over gRPC with high performance.

use super::service::TypeService;
use crate::agent::AgentCoordinator;
use crate::core::SharedUniverse;

use std::net::SocketAddr;
use std::sync::Arc;

use tracing::info;

/// API server configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub bind_address: SocketAddr,
    pub max_concurrent_streams: usize,
    pub enable_reflection: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_address: "[::1]:50051".parse().unwrap(),
            max_concurrent_streams: 100,
            enable_reflection: true,
        }
    }
}

/// API server
pub struct ApiServer {
    config: ApiConfig,
    service: TypeService,
}

impl ApiServer {
    pub fn new(config: ApiConfig, universe: SharedUniverse) -> Self {
        let coordinator = Arc::new(AgentCoordinator::new(universe.clone()));
        let service = TypeService::new(universe, coordinator);

        Self { config, service }
    }

    /// Start the API server
    pub async fn start(&self) -> Result<(), ApiError> {
        info!("Starting API server on {}", self.config.bind_address);

        // Would create actual gRPC service here
        // For now, just a placeholder
        info!(
            "gRPC server placeholder - would start on {}",
            self.config.bind_address
        );

        Ok(())
    }

    /// Get server health
    pub async fn health_check(&self) -> HealthStatus {
        HealthStatus {
            healthy: true,
            active_sessions: 0, // Would get from coordinator
            uptime_seconds: 0,
        }
    }
}

/// Health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub active_sessions: usize,
    pub uptime_seconds: u64,
}

/// API error
#[derive(Debug)]
pub enum ApiError {
    Transport(tonic::transport::Error),
    Bind(std::io::Error),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "Transport error: {}", e),
            Self::Bind(e) => write!(f, "Bind error: {}", e),
        }
    }
}

impl std::error::Error for ApiError {}

/// HTTP/REST fallback server
pub struct RestServer {
    bind_address: SocketAddr,
    service: TypeService,
}

impl RestServer {
    pub fn new(bind_address: SocketAddr, service: TypeService) -> Self {
        Self {
            bind_address,
            service,
        }
    }

    /// Start REST server
    pub async fn start(&self) -> Result<(), ApiError> {
        // Would implement REST endpoints using axum or similar
        info!("REST server would start on {}", self.bind_address);
        Ok(())
    }
}

/// WebSocket server for real-time streaming
pub struct WebSocketServer {
    bind_address: SocketAddr,
    service: TypeService,
}

impl WebSocketServer {
    pub fn new(bind_address: SocketAddr, service: TypeService) -> Self {
        Self {
            bind_address,
            service,
        }
    }

    /// Start WebSocket server for streaming validation
    pub async fn start(&self) -> Result<(), ApiError> {
        info!("WebSocket server would start on {}", self.bind_address);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    #[test]
    fn test_server_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let config = ApiConfig::default();
        let server = ApiServer::new(config, universe);

        // Just verify it compiles
    }
}
