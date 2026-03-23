//! Protocol for Go compiler IPC communication
//! 
//! Defines message types for hybrid mode interaction.

use crate::core::{TypeId, Type};
use serde::{Serialize, Deserialize};

/// Protocol message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
    Heartbeat,
}

/// Request from Go compiler to wootype
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    /// Query type by ID
    GetType { type_id: TypeId },
    
    /// Query type by name
    GetTypeByName { package: String, name: String },
    
    /// Check if type implements interface
    CheckImplementation { 
        concrete_type: TypeId, 
        interface_type: TypeId 
    },
    
    /// Get assignability
    CheckAssignable { from: TypeId, to: TypeId },
    
    /// Type-check expression
    TypeCheckExpression {
        expr: String,
        context: TypeCheckContext,
    },
    
    /// Get completions
    GetCompletions {
        prefix: String,
        position: SourcePosition,
        file: String,
    },
    
    /// Import package types
    ImportPackage { path: String },
    
    /// Export type to Go
    ExportType { typ: Type },
    
    /// Sync request
    Sync { checkpoint: u64 },
}

/// Type check context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeCheckContext {
    pub package: String,
    pub file: String,
    pub expected_type: Option<TypeId>,
    pub scope_bindings: Vec<(String, TypeId)>,
}

/// Source position
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourcePosition {
    pub line: u32,
    pub column: u32,
    pub offset: u32,
}

/// Response from wootype to Go compiler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Type(Option<Type>),
    ImplementationCheck { implements: bool },
    Assignable { assignable: bool },
    TypeCheckResult(TypeCheckResult),
    Completions(Vec<Completion>),
    ImportResult(ImportResult),
    ExportResult(ExportResult),
    SyncAck { checkpoint: u64 },
    Error(String),
}

/// Type check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeCheckResult {
    pub valid: bool,
    pub inferred_type: Option<TypeId>,
    pub errors: Vec<TypeError>,
}

/// Type error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeError {
    pub message: String,
    pub position: SourcePosition,
    pub code: String,
}

/// Completion item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
    pub documentation: String,
}

/// Completion kind
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompletionKind {
    Type,
    Function,
    Method,
    Field,
    Variable,
    Constant,
    Package,
}

/// Import result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub success: bool,
    pub types_imported: usize,
    pub errors: Vec<String>,
}

/// Export result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub success: bool,
    pub type_id: TypeId,
    pub error: Option<String>,
}

/// Notification (async update)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Notification {
    /// Types updated
    TypesChanged { type_ids: Vec<TypeId> },
    
    /// New package imported
    PackageImported { path: String },
    
    /// Compilation started
    CompilationStarted,
    
    /// Compilation finished
    CompilationFinished { success: bool },
    
    /// Error notification
    Error { message: String },
}

/// Protocol version
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Message header for framing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    pub version: String,
    pub message_type: MessageType,
    pub payload_len: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MessageType {
    Request,
    Response,
    Notification,
    Heartbeat,
}

impl MessageHeader {
    pub fn new(message_type: MessageType, payload_len: u32) -> Self {
        Self {
            version: PROTOCOL_VERSION.to_string(),
            message_type,
            payload_len,
        }
    }
    
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }
    
    pub fn decode(data: &[u8]) -> Option<Self> {
        bincode::deserialize(data).ok()
    }
}

/// Serialize message to bytes
pub fn serialize_message(msg: &Message) -> Vec<u8> {
    bincode::serialize(msg).unwrap_or_default()
}

/// Deserialize message from bytes
pub fn deserialize_message(data: &[u8]) -> Option<Message> {
    bincode::deserialize(data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = Message::Request(Request::GetType { type_id: TypeId(42) });
        let bytes = serialize_message(&msg);
        let decoded = deserialize_message(&bytes);
        
        assert!(decoded.is_some());
    }

    #[test]
    fn test_header_encode_decode() {
        let header = MessageHeader::new(MessageType::Request, 100);
        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes);
        
        assert!(decoded.is_some());
        assert_eq!(decoded.unwrap().payload_len, 100);
    }
}
