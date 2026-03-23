//! gRPC API for AI Agent access
//! 
//! Provides high-performance API for Cursor, Claude Code, etc.

pub mod server;
pub mod service;
pub mod grpc;
pub mod websocket;

pub use server::{ApiServer, ApiConfig};
pub use service::TypeService;
pub use grpc::GrpcTypeService;
pub use websocket::WebSocketServer;
