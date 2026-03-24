//! gRPC API for AI Agent access
//!
//! Provides high-performance API for Cursor, Claude Code, etc.

pub mod grpc;
pub mod server;
pub mod service;
pub mod websocket;

pub use grpc::GrpcTypeService;
pub use server::{ApiConfig, ApiServer};
pub use service::TypeService;
pub use websocket::WebSocketServer;
