//! Wooftype - Type System as a Service for Go
//! 
//! A Rust-powered type checker providing zero-latency type queries
//! for AI coding assistants.
//!
//! # Architecture
//!
//! - **Core**: ECS-based type storage with lock-free indexing
//! - **Query**: Sub-millisecond type resolution with SIMD acceleration
//! - **Validate**: Streaming validation for AI token generation
//! - **Agent**: Multi-agent concurrency with branch isolation
//! - **Bridge**: IPC integration with Go compiler
//! - **API**: gRPC/WebSocket services for AI agents
//!
//! # Example Usage
//!
//! ```rust,no_run
//! use wooftype::core::TypeUniverse;
//! use wooftype::query::QueryEngine;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let universe = Arc::new(TypeUniverse::new());
//!     let engine = QueryEngine::new(universe);
//!     
//!     // Query types
//!     let results = engine.query_by_fingerprint(...);
//! }
//! ```

#![warn(missing_docs)]
#![allow(dead_code)] // Phase 1 implementation

pub mod core;
pub mod query;
pub mod validate;
pub mod agent;
pub mod bridge;
pub mod api;
pub mod parser;

// Re-export agent types for convenience
pub use agent::{
    AgentCoordinator,
    AgentSession,
    SessionConfig,
    SessionId,
    AgentType,
    IsolationLevel,
    AgentId,
};

/// Version of the library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-export commonly used types
pub mod prelude {
    pub use crate::core::{
        TypeUniverse, SharedUniverse,
        Type, TypeId, TypeKind, PrimitiveType,
        Entity, EntityId,
    };
    pub use crate::query::QueryEngine;
    pub use crate::agent::{AgentCoordinator, AgentSession};
}

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize logging/tracing
pub fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Build information
pub mod build {
    /// Build timestamp
    pub const TIMESTAMP: &str = env!("VERGEN_BUILD_TIMESTAMP", "unknown");
    
    /// Git commit
    pub const GIT_COMMIT: &str = env!("VERGEN_GIT_SHA", "unknown");
    
    /// Target triple
    pub const TARGET: &str = env!("VERGEN_CARGO_TARGET_TRIPLE", "unknown");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_init_logging() {
        // Should not panic
        init_logging();
    }
}
