//! Multi-Agent concurrency with branch isolation
//!
//! Supports multiple AI Agents (Cursor, Claude Code, etc.)
//! with copy-on-write semantics for isolated type environments.

pub mod branch;
pub mod coordinator;
pub mod rag;
pub mod session;

pub use branch::{Branch, BranchManager};
pub use coordinator::{AgentCoordinator, AgentId, ConnectionRequest, ConnectionResult};
pub use rag::{SemanticSearch, TypeEmbeddings};
pub use session::{AgentSession, AgentType, IsolationLevel, SessionConfig, SessionId};
