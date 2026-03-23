//! Core type system components
//! 
//! This module implements the ECS-based type storage architecture,
//! providing zero-latency type queries for AI Agents.

pub mod entity;
pub mod storage;
pub mod types;
pub mod symbol;
pub mod universe;

pub use entity::{Entity, EntityId, Generation};
pub use storage::ArchetypeStorage;
pub use types::{Type, TypeId, TypeKind, PrimitiveType, TypeFlags, TypeFingerprint};
pub use symbol::{SymbolId, SymbolTable, Scope};
pub use universe::{TypeUniverse, SharedUniverse, PackageInfo};
