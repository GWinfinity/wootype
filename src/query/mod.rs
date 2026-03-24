//! Type query engine - Zero-latency type queries
//!
//! Provides:
//! - Sub-millisecond type resolution
//! - Interface satisfaction queries
//! - Type similarity search
//! - Cross-reference analysis

pub mod cache;
pub mod engine;
pub mod pattern;

pub use cache::QueryCache;
pub use engine::QueryEngine;
pub use pattern::{QueryFilter, TypePattern};
