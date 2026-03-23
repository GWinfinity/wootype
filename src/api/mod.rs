//! gRPC API for AI Agent access
//! 
//! Provides high-performance API for Cursor, Claude Code, etc.

pub mod server;
pub mod service;

pub use server::ApiServer;
pub use service::TypeService;
