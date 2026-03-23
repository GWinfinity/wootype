//! Type query engine - Zero-latency type queries
//! 
//! Provides:
//! - Sub-millisecond type resolution
//! - Interface satisfaction queries
//! - Type similarity search
//! - Cross-reference analysis

pub mod engine;
pub mod cache;
pub mod pattern;

pub use engine::QueryEngine;
pub use cache::QueryCache;
pub use pattern::{TypePattern, QueryFilter};
