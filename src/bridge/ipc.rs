//! IPC Bridge implementation
//! 
//! Handles communication with Go compiler via Unix domain sockets / named pipes.

use super::protocol::{Message, Request, Response, MessageHeader, MessageType, serialize_message, deserialize_message};
use crate::core::SharedUniverse;
use crate::query::QueryEngine;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, error, warn};

/// IPC Bridge configuration
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub socket_path: PathBuf,
    pub max_connections: usize,
    pub message_timeout_ms: u64,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/wooftype.sock"),
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
    listener: Option<UnixListener>,
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
    pub async fn start(&mut self) -> Result<(), BridgeError> {
        // Remove existing socket
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)
                .map_err(|e| BridgeError::Io(e))?;
        }
        
        // Create listener
        let listener = UnixListener::bind(&self.config.socket_path)
            .map_err(|e| BridgeError::Io(e))?;
        
        info!("IPC bridge listening on {:?}", self.config.socket_path);
        
        self.listener = Some(listener);
        
        // Accept connections
        loop {
            if let Some(ref listener) = self.listener {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("New connection from {:?}", addr);
                        let universe = self.universe.clone();
                        tokio::spawn(handle_connection(stream, universe));
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
        }
    }
    
    /// Stop the bridge
    pub async fn stop(&mut self) -> Result<(), BridgeError> {
        self.listener = None;
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)
                .map_err(|e| BridgeError::Io(e))?;
        }
        Ok(())
    }
}

/// Handle a single connection
async fn handle_connection(mut stream: UnixStream, universe: SharedUniverse) {
    let mut buffer = vec![0u8; 8192];
    
    loop {
        // Read header
        let header_size = match stream.read(&mut buffer[..256]).await {
            Ok(0) => {
                info!("Connection closed");
                return;
            }
            Ok(n) => n,
            Err(e) => {
                error!("Read error: {}", e);
                return;
            }
        };
        
        // Parse header
        let header = match MessageHeader::decode(&buffer[..header_size]) {
            Some(h) => h,
            None => {
                warn!("Invalid header");
                continue;
            }
        };
        
        // Read payload
        let payload_len = header.payload_len as usize;
        if payload_len > buffer.len() {
            buffer.resize(payload_len, 0);
        }
        
        if let Err(e) = stream.read_exact(&mut buffer[..payload_len]).await {
            error!("Read payload error: {}", e);
            return;
        }
        
        // Deserialize and handle message
        if let Some(msg) = deserialize_message(&buffer[..payload_len]) {
            let response = handle_message(msg, &universe).await;
            let response_bytes = serialize_message(&response);
            
            // Send response
            let header = MessageHeader::new(MessageType::Response, response_bytes.len() as u32);
            let header_bytes = header.encode();
            
            if let Err(e) = stream.write_all(&header_bytes).await {
                error!("Write header error: {}", e);
                return;
            }
            
            if let Err(e) = stream.write_all(&response_bytes).await {
                error!("Write payload error: {}", e);
                return;
            }
        }
    }
}

/// Handle a message and produce response
async fn handle_message(msg: Message, universe: &SharedUniverse) -> Message {
    match msg {
        Message::Request(req) => {
            let response = handle_request(req, universe).await;
            Message::Response(response)
        }
        Message::Heartbeat => Message::Heartbeat,
        _ => Message::Response(Response::Error("Unexpected message type".to_string())),
    }
}

/// Handle a request
async fn handle_request(req: Request, universe: &SharedUniverse) -> Response {
    match req {
        Request::GetType { type_id } => {
            let typ = universe.get_type(type_id);
            Response::Type(typ.map(|t| (*t).clone()))
        }
        
        Request::GetTypeByName { package, name } => {
            let symbol = universe.symbols().lookup(Some(&package), &name);
            let typ = symbol.and_then(|s| universe.lookup_by_symbol(s));
            Response::Type(typ.map(|t| (*t).clone()))
        }
        
        Request::CheckImplementation { concrete_type, interface_type } => {
            // Simplified - would use query engine
            Response::ImplementationCheck { implements: true }
        }
        
        Request::CheckAssignable { from, to } => {
            // Simplified assignability check
            let assignable = from == to;
            Response::Assignable { assignable }
        }
        
        Request::TypeCheckExpression { expr, context } => {
            // Would integrate with streaming checker
            Response::TypeCheckResult(super::protocol::TypeCheckResult {
                valid: true,
                inferred_type: None,
                errors: vec![],
            })
        }
        
        Request::GetCompletions { prefix, position, file } => {
            Response::Completions(vec![])
        }
        
        Request::ImportPackage { path } => {
            Response::ImportResult(super::protocol::ImportResult {
                success: true,
                types_imported: 0,
                errors: vec![],
            })
        }
        
        Request::ExportType { typ } => {
            universe.insert_type(typ.id, Arc::new(typ));
            Response::ExportResult(super::protocol::ExportResult {
                success: true,
                type_id: TypeId(0),
                error: None,
            })
        }
        
        Request::Sync { checkpoint } => {
            Response::SyncAck { checkpoint }
        }
    }
}

/// Bridge error
#[derive(Debug)]
pub enum BridgeError {
    Io(std::io::Error),
    Bind(String),
    Timeout,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Bind(s) => write!(f, "Bind error: {}", s),
            Self::Timeout => write!(f, "Operation timed out"),
        }
    }
}

impl std::error::Error for BridgeError {}

use crate::core::TypeId;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    #[tokio::test]
    async fn test_bridge_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let config = BridgeConfig::default();
        let bridge = IpcBridge::new(universe, config);
        
        assert!(bridge.listener.is_none());
    }
}
