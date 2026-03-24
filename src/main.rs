//! Wootype Daemon - Type System as a Service
//!
//! CLI entry point for the wootype type checking service.
//!
//! # Usage
//!
//! ```bash
//! # Start the daemon
//! wootype daemon
//!
//! # Import a package
//! wootype import github.com/example/mypackage
//!
//! # Query types
//! wootype query --session <id> --type int
//!
//! # Validate expression
//! wootype validate --session <id> --expr "x + 1"
//! ```

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

use wootype::agent::{AgentCoordinator, AgentSession, AgentType, SessionConfig};
use wootype::api::{ApiConfig, ApiServer};
use wootype::bridge::{BridgeConfig, IpcBridge};
use wootype::core::{TypeId, TypeUniverse};

/// Wootype CLI
#[derive(Parser)]
#[command(name = "wootype")]
#[command(about = "Type System as a Service for Go")]
#[command(version)]
struct Cli {
    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Subcommand
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the type daemon
    Daemon {
        /// IPC socket path
        #[arg(short, long, default_value = "/tmp/wootype.sock")]
        socket: PathBuf,

        /// gRPC bind address
        #[arg(short, long, default_value = "127.0.0.1:50051")]
        grpc_addr: String,

        /// Preload stdlib packages
        #[arg(long)]
        preload_stdlib: bool,
    },

    /// Import a Go package
    Import {
        /// Package path
        package: String,

        /// Recursive import
        #[arg(short, long)]
        recursive: bool,
    },

    /// Query types
    Query {
        /// Session ID
        #[arg(short, long)]
        session: Option<String>,

        /// Type name
        #[arg(short, long)]
        type_name: Option<String>,

        /// Package path
        #[arg(short, long)]
        package: Option<String>,

        /// Pattern search
        #[arg(short, long)]
        pattern: Option<String>,
    },

    /// Validate expression
    Validate {
        /// Session ID
        #[arg(short, long)]
        session: String,

        /// Expression to validate
        #[arg(short, long)]
        expr: String,

        /// Expected type
        #[arg(short, long)]
        expected_type: Option<String>,
    },

    /// Connect as an AI Agent
    Connect {
        /// Agent name
        #[arg(short, long)]
        name: String,

        /// Agent type
        #[arg(short, long, default_value = "generic")]
        agent_type: String,

        /// Isolation level
        #[arg(short, long, default_value = "full")]
        isolation: String,
    },

    /// Show status
    Status,

    /// Run benchmarks
    Bench {
        /// Benchmark type
        #[arg(short, long, default_value = "all")]
        kind: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(&cli.log_level)
        .init();

    info!("Wootype v{}", env!("CARGO_PKG_VERSION"));

    match cli.command {
        Commands::Daemon {
            socket,
            grpc_addr,
            preload_stdlib,
        } => {
            run_daemon(socket, grpc_addr, preload_stdlib).await?;
        }
        Commands::Import { package, recursive } => {
            run_import(package, recursive).await?;
        }
        Commands::Query {
            session,
            type_name,
            package,
            pattern,
        } => {
            run_query(session, type_name, package, pattern).await?;
        }
        Commands::Validate {
            session,
            expr,
            expected_type,
        } => {
            run_validate(session, expr, expected_type).await?;
        }
        Commands::Connect {
            name,
            agent_type,
            isolation,
        } => {
            run_connect(name, agent_type, isolation).await?;
        }
        Commands::Status => {
            run_status().await?;
        }
        Commands::Bench { kind } => {
            run_bench(kind).await?;
        }
    }

    Ok(())
}

/// Run the daemon
async fn run_daemon(
    socket: PathBuf,
    grpc_addr: String,
    preload_stdlib: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting wootype daemon");
    info!("IPC socket: {:?}", socket);
    info!("gRPC address: {}", grpc_addr);

    // Create universe
    let universe = Arc::new(TypeUniverse::new());
    info!(
        "Type universe initialized with {} types",
        universe.type_count()
    );

    // Preload stdlib if requested
    if preload_stdlib {
        info!("Preloading stdlib packages...");
        let importer = wootype::parser::PackageImporter::new(universe.clone());
        let results = importer.preload_stdlib().await;
        let total_imported: usize = results.iter().map(|r| r.types_imported).sum();
        info!("Preloaded {} types from stdlib", total_imported);
    }

    // Create coordinator
    let coordinator = Arc::new(AgentCoordinator::new(universe.clone()));
    info!("Agent coordinator initialized");

    // Start IPC bridge
    let bridge_config = BridgeConfig {
        socket_path: socket,
        max_connections: 100,
        message_timeout_ms: 5000,
    };
    let mut bridge = IpcBridge::new(universe.clone(), bridge_config);

    // Start gRPC server
    let api_config = ApiConfig {
        bind_address: grpc_addr.parse()?,
        max_concurrent_streams: 100,
        enable_reflection: true,
    };
    let api_server = ApiServer::new(api_config, universe.clone());

    // Run both services concurrently
    tokio::select! {
        result = bridge.start() => {
            if let Err(e) = result {
                error!("IPC bridge error: {}", e);
            }
        }
        result = api_server.start() => {
            if let Err(e) = result {
                error!("API server error: {}", e);
            }
        }
    }

    Ok(())
}

/// Import a package
async fn run_import(package: String, _recursive: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!("Importing package: {}", package);

    let universe = Arc::new(TypeUniverse::new());
    let importer = wootype::parser::PackageImporter::new(universe);

    match importer.import(&package).await {
        Ok(result) => {
            info!("Imported {} types from {}", result.types_imported, package);
            for error in &result.errors {
                error!("Import error: {:?}", error);
            }
        }
        Err(e) => {
            error!("Failed to import {}: {:?}", package, e);
        }
    }

    Ok(())
}

/// Query types
async fn run_query(
    _session: Option<String>,
    type_name: Option<String>,
    package: Option<String>,
    pattern: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let universe = Arc::new(TypeUniverse::new());
    let engine = wootype::query::QueryEngine::new(universe);

    info!("Querying types...");

    if let Some(name) = type_name {
        info!("Looking up type: {}", name);
        // Would query by name
    }

    if let Some(pkg) = package {
        info!("In package: {}", pkg);
    }

    if let Some(pat) = pattern {
        info!("Pattern: {}", pat);
        // Would do pattern search
    }

    Ok(())
}

/// Validate expression
async fn run_validate(
    _session: String,
    expr: String,
    expected_type: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Validating expression: {}", expr);

    if let Some(expected) = expected_type {
        info!("Expected type: {}", expected);
    }

    // Would validate expression

    Ok(())
}

/// Connect as AI Agent
async fn run_connect(
    name: String,
    agent_type: String,
    isolation: String,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Connecting agent: {} (type: {})", name, agent_type);

    let universe = Arc::new(TypeUniverse::new());
    let coordinator = Arc::new(AgentCoordinator::new(universe));

    let agent_type = match agent_type.as_str() {
        "cursor" => AgentType::Cursor,
        "claude_code" => AgentType::ClaudeCode,
        "gemini_cli" => AgentType::GeminiCLI,
        "github_copilot" => AgentType::GitHubCopilot,
        _ => AgentType::Generic,
    };

    let isolation_level = match isolation.as_str() {
        "full" => wootype::agent::IsolationLevel::Full,
        "shared" => wootype::agent::IsolationLevel::SharedRead,
        "snapshot" => wootype::agent::IsolationLevel::Snapshot,
        _ => wootype::agent::IsolationLevel::Full,
    };

    let conn_request = wootype::agent::coordinator::ConnectionRequest {
        agent_id: wootype::agent::AgentId::new(1),
        name,
        agent_type,
        preferred_isolation: Some(isolation_level),
    };

    match coordinator.connect(conn_request).await {
        wootype::agent::coordinator::ConnectionResult::Connected { session_id } => {
            info!("Connected! Session ID: {}", session_id.0);
        }
        wootype::agent::coordinator::ConnectionResult::Rejected { reason } => {
            error!("Connection rejected: {:?}", reason);
        }
    }

    Ok(())
}

/// Show status
async fn run_status() -> Result<(), Box<dyn std::error::Error>> {
    info!("Wootype Status");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    // Would show actual status

    Ok(())
}

/// Run benchmarks
async fn run_bench(kind: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("Running benchmarks: {}", kind);

    let universe = Arc::new(TypeUniverse::new());
    let engine = wootype::query::QueryEngine::new(universe);

    // Type query benchmark
    let start = std::time::Instant::now();
    for _ in 0..10000 {
        let _ = engine.get_type(TypeId(1));
    }
    let elapsed = start.elapsed();

    info!("Query benchmark: {:?} for 10000 queries", elapsed);
    info!("Average: {:?} per query", elapsed / 10000);

    Ok(())
}
