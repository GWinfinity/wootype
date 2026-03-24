//! Type Daemon - Type System as a Service
//!
//! Main daemon implementation for running wootype as a service.

use crate::agent::AgentCoordinator;
use crate::api::{ApiConfig, ApiServer, GrpcTypeService, WebSocketServer};
use crate::core::SharedUniverse;

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Daemon configuration
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// gRPC bind address
    pub grpc_addr: SocketAddr,
    /// WebSocket bind address
    pub ws_addr: SocketAddr,
    /// IPC socket path
    pub ipc_socket: std::path::PathBuf,
    /// Preload stdlib
    pub preload_stdlib: bool,
    /// Enable gRPC
    pub enable_grpc: bool,
    /// Enable WebSocket
    pub enable_ws: bool,
    /// Enable IPC
    pub enable_ipc: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "[::1]:50051".parse().unwrap(),
            ws_addr: "[::1]:8080".parse().unwrap(),
            ipc_socket: std::path::PathBuf::from("/tmp/wootype.sock"),
            preload_stdlib: false,
            enable_grpc: true,
            enable_ws: true,
            enable_ipc: true,
        }
    }
}

/// Type Daemon
pub struct TypeDaemon {
    config: DaemonConfig,
    universe: SharedUniverse,
    coordinator: Arc<AgentCoordinator>,
}

impl TypeDaemon {
    /// Create a new daemon
    pub fn new(config: DaemonConfig) -> Self {
        let universe = Arc::new(crate::core::TypeUniverse::new());
        let coordinator = Arc::new(AgentCoordinator::new(universe.clone()));

        info!("🚀 Type Daemon initialized");
        info!("   Universe size: {} types", universe.type_count());

        Self {
            config,
            universe,
            coordinator,
        }
    }

    /// Run the daemon
    pub async fn run(&self) -> Result<(), DaemonError> {
        info!("Starting Type Daemon...");

        // Preload stdlib if requested
        if self.config.preload_stdlib {
            self.preload_stdlib().await?;
        }

        // Start services
        let mut handles = vec![];

        if self.config.enable_grpc {
            let grpc_handle = self.start_grpc().await?;
            handles.push(grpc_handle);
        }

        if self.config.enable_ws {
            let ws_handle = self.start_websocket().await?;
            handles.push(ws_handle);
        }

        // Wait for all services
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Service error: {}", e);
            }
        }

        Ok(())
    }

    /// Preload stdlib packages
    async fn preload_stdlib(&self) -> Result<(), DaemonError> {
        info!("Preloading stdlib packages...");

        let importer = crate::parser::PackageImporter::new(self.universe.clone());
        let results = importer.preload_stdlib().await;
        let total_imported: usize = results.iter().map(|r| r.types_imported).sum();

        info!("Preloaded {} types from stdlib", total_imported);
        Ok(())
    }

    /// Start gRPC service
    async fn start_grpc(&self) -> Result<tokio::task::JoinHandle<()>, DaemonError> {
        info!("Starting gRPC server on {}", self.config.grpc_addr);

        let service = GrpcTypeService::new(self.universe.clone(), self.coordinator.clone());

        let addr = self.config.grpc_addr;
        let reflection_service = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(crate::api::grpc::proto::FILE_DESCRIPTOR_SET)
            .build_v1()
            .map_err(|e| DaemonError::Internal(e.to_string()))?;

        let handle = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(service.into_service())
                .add_service(reflection_service)
                .serve(addr)
                .await
                .expect("gRPC server failed");
        });

        Ok(handle)
    }

    /// Start WebSocket service
    async fn start_websocket(&self) -> Result<tokio::task::JoinHandle<()>, DaemonError> {
        info!("Starting WebSocket server on {}", self.config.ws_addr);

        let server = WebSocketServer::new(
            self.config.ws_addr,
            self.universe.clone(),
            self.coordinator.clone(),
        );

        let handle = tokio::spawn(async move {
            if let Err(e) = server.start().await {
                error!("WebSocket server error: {}", e);
            }
        });

        Ok(handle)
    }

    /// Get daemon stats
    pub fn stats(&self) -> DaemonStats {
        DaemonStats {
            universe_size: self.universe.type_count(),
            active_sessions: 0, // Would get from coordinator
            uptime_secs: 0,
        }
    }
}

/// Daemon statistics
#[derive(Debug)]
pub struct DaemonStats {
    pub universe_size: usize,
    pub active_sessions: usize,
    pub uptime_secs: u64,
}

/// Daemon error
#[derive(Debug)]
pub enum DaemonError {
    Io(std::io::Error),
    Transport(tonic::transport::Error),
    Internal(String),
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Transport(e) => write!(f, "Transport error: {}", e),
            Self::Internal(s) => write!(f, "Internal error: {}", s),
        }
    }
}

impl std::error::Error for DaemonError {}

impl From<std::io::Error> for DaemonError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<tonic::transport::Error> for DaemonError {
    fn from(e: tonic::transport::Error) -> Self {
        Self::Transport(e)
    }
}
