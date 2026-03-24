//! Core type system components
//!
//! This module implements the ECS-based type storage architecture,
//! providing zero-latency type queries for AI Agents.

pub mod entity;
pub mod gomod;
pub mod method;
pub mod serde_impl;
pub mod storage;
pub mod symbol;
pub mod types;
pub mod universe;

pub use entity::{Entity, EntityId, Generation};
pub use storage::ArchetypeStorage;
pub use symbol::{Scope, SymbolId, SymbolTable};
pub use types::{PrimitiveType, Type, TypeFingerprint, TypeFlags, TypeId, TypeKind};
pub use universe::{PackageInfo, SharedUniverse, TypeUniverse};
