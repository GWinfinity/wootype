//! IPC Bridge implementation
//!
//! Handles communication with Go compiler via Unix domain sockets (Unix)
//! or TCP sockets (Windows).

use super::protocol::{deserialize_message, serialize_message, Message, Request, Response};
use crate::core::SharedUniverse;
use crate::query::QueryEngine;

use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info, warn};

#[cfg(windows)]
use tokio::net::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

/// IPC Bridge configuration
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    #[cfg(unix)]
    pub socket_path: PathBuf,
    #[cfg(windows)]
    pub tcp_port: u16,
    pub max_connections: usize,
    pub message_timeout_ms: u64,
}

impl Default for BridgeConfig {
    #[cfg(unix)]
    fn default() -> Self {
        use dirs::runtime_dir;
        let socket_path = runtime_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("wootype.sock");

        Self {
            socket_path,
            max_connections: 10,
            message_timeout_ms: 5000,
        }
    }

    #[cfg(windows)]
    fn default() -> Self {
        Self {
            tcp_port: 0, // Random available port
            max_connections: 10,
            message_timeout_ms: 5000,
        }
    }
}

/// IPC Bridge for Go compiler communication
pub struct IpcBridge {
    config: BridgeConfig,
    universe: SharedUniverse,
    query_engine: QueryEngine,
    #[cfg(unix)]
    listener: Option<UnixListener>,
    #[cfg(windows)]
    listener: Option<TcpListener>,
}

impl IpcBridge {
    pub fn new(universe: SharedUniverse, config: BridgeConfig) -> Self {
        let query_engine = QueryEngine::new(universe.clone());

        Self {
            config,
            universe,
            query_engine,
            listener: None,
        }
    }

    /// Start the IPC bridge
    #[cfg(unix)]
    pub async fn start(&mut self) -> anyhow::Result<()> {
        // Remove old socket file if exists
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)?;
        }

        let listener = UnixListener::bind(&self.config.socket_path)?;
        info!("IPC bridge listening on {:?}", self.config.socket_path);

        self.listener = Some(listener);
        self.run_unix().await
    }

    /// Start the IPC bridge (Windows)
    #[cfg(windows)]
    pub async fn start(&mut self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.config.tcp_port)).await?;
        let local_addr = listener.local_addr()?;
        info!("IPC bridge listening on TCP {}", local_addr);

        self.listener = Some(listener);
        self.run_windows().await
    }

    /// Run the Unix socket server
    #[cfg(unix)]
    async fn run_unix(&mut self) -> anyhow::Result<()> {
        let listener = self.listener.as_ref().unwrap();

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let universe = self.universe.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_unix_connection(stream, universe).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Run the TCP server (Windows)
    #[cfg(windows)]
    async fn run_windows(&mut self) -> anyhow::Result<()> {
        let listener = self.listener.as_ref().unwrap();

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New connection from {}", addr);
                    let universe = self.universe.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_tcp_connection(stream, universe).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Stop the IPC bridge
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        self.listener = None;

        #[cfg(unix)]
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)?;
        }

        Ok(())
    }
}

/// Handle a single Unix socket connection
#[cfg(unix)]
async fn handle_unix_connection(
    mut stream: UnixStream,
    _universe: SharedUniverse,
) -> anyhow::Result<()> {
    let mut buffer = vec![0u8; 4096];

    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            break;
        }

        if let Some(message) = deserialize_message(&buffer[..n]) {
            let response = process_message(message).await;
            let response_bytes = serialize_message(&response);
            stream.write_all(&response_bytes).await?;
        } else {
            let error_response =
                Message::Response(Response::Error("Failed to deserialize message".to_string()));
            let response_bytes = serialize_message(&error_response);
            stream.write_all(&response_bytes).await?;
        }
    }

    Ok(())
}

/// Handle a single TCP connection (Windows)
#[cfg(windows)]
async fn handle_tcp_connection(
    mut stream: TcpStream,
    _universe: SharedUniverse,
) -> anyhow::Result<()> {
    let mut buffer = vec![0u8; 4096];

    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            break;
        }

        if let Some(message) = deserialize_message(&buffer[..n]) {
            let response = process_message(message).await;
            let response_bytes = serialize_message(&response);
            stream.write_all(&response_bytes).await?;
        } else {
            let error_response =
                Message::Response(Response::Error("Failed to deserialize message".to_string()));
            let response_bytes = serialize_message(&error_response);
            stream.write_all(&response_bytes).await?;
        }
    }

    Ok(())
}

/// Process a message and generate a response
async fn process_message(message: Message) -> Message {
    match message {
        Message::Request(request) => handle_request(request).await,
        Message::Response(_) => {
            warn!("Received unexpected response message");
            Message::Response(Response::Error("Unexpected message type".to_string()))
        }
        Message::Notification(_) => {
            warn!("Received notification message");
            Message::Response(Response::Error("Notifications not handled".to_string()))
        }
        Message::Heartbeat => Message::Heartbeat,
    }
}

/// Handle a request and generate response
async fn handle_request(request: Request) -> Message {
    match request {
        Request::Sync { checkpoint } => Message::Response(Response::SyncAck { checkpoint }),
        _ => Message::Response(Response::Error(format!(
            "Unimplemented request: {:?}",
            request
        ))),
    }
}
