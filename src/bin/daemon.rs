//! Wootype Daemon - Type System as a Service
//!
//! Standalone daemon binary for running wootype as a service.

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{error, info};

use wootype::daemon::{DaemonConfig, TypeDaemon};

/// Wootype Daemon CLI
#[derive(Parser)]
#[command(name = "wootype-daemon")]
#[command(about = "Type System as a Service for Go")]
#[command(version)]
struct Cli {
    /// gRPC bind address
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    grpc_addr: String,

    /// WebSocket bind address
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    ws_addr: String,

    /// IPC socket path
    #[arg(short, long, default_value = "/tmp/wootype.sock")]
    socket: PathBuf,

    /// Preload stdlib packages
    #[arg(long)]
    preload_stdlib: bool,

    /// Disable gRPC
    #[arg(long)]
    no_grpc: bool,

    /// Disable WebSocket
    #[arg(long)]
    no_ws: bool,

    /// Disable IPC
    #[arg(long)]
    no_ipc: bool,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(&cli.log_level)
        .init();

    info!("🚀 Wootype Daemon v{}", env!("CARGO_PKG_VERSION"));

    // Parse addresses
    let grpc_addr: SocketAddr = cli.grpc_addr.parse()?;
    let ws_addr: SocketAddr = cli.ws_addr.parse()?;

    info!("Configuration:");
    info!("  gRPC: {} (enabled: {})", grpc_addr, !cli.no_grpc);
    info!("  WebSocket: {} (enabled: {})", ws_addr, !cli.no_ws);
    info!("  IPC: {:?} (enabled: {})", cli.socket, !cli.no_ipc);
    info!("  Preload stdlib: {}", cli.preload_stdlib);

    // Create config
    let config = DaemonConfig {
        grpc_addr,
        ws_addr,
        ipc_socket: cli.socket,
        preload_stdlib: cli.preload_stdlib,
        enable_grpc: !cli.no_grpc,
        enable_ws: !cli.no_ws,
        enable_ipc: !cli.no_ipc,
    };

    // Create and run daemon
    let daemon = TypeDaemon::new(config);

    if let Err(e) = daemon.run().await {
        error!("Daemon error: {}", e);
        return Err(e.into());
    }

    Ok(())
}
