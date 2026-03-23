//! Agent session management
//! 
//! Each AI Agent gets an isolated session with its own branch.

use crate::core::{SharedUniverse, TypeUniverse};
use crate::query::QueryEngine;
use crate::validate::StreamingChecker;
use super::branch::Branch;
use super::rag::TypeEmbeddings;

use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Unique agent session ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub name: String,
    pub agent_type: AgentType,
    pub enable_rag: bool,
    pub max_branches: usize,
    pub isolation_level: IsolationLevel,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            agent_type: AgentType::Generic,
            enable_rag: true,
            max_branches: 10,
            isolation_level: IsolationLevel::Full,
        }
    }
}

/// Type of AI Agent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    Cursor,
    ClaudeCode,
    GeminiCLI,
    GitHubCopilot,
    Generic,
}

impl AgentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::ClaudeCode => "claude_code",
            Self::GeminiCLI => "gemini_cli",
            Self::GitHubCopilot => "github_copilot",
            Self::Generic => "generic",
        }
    }
}

/// Isolation level for the session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// Full copy-on-write isolation
    Full,
    /// Shared read, isolated write
    SharedRead,
    /// Snapshot isolation (point-in-time read)
    Snapshot,
}

/// An agent session with isolated type environment
pub struct AgentSession {
    pub id: SessionId,
    pub config: SessionConfig,
    
    // Isolated universe branch
    universe: Arc<RwLock<Branch>>,
    
    // Query engine
    query_engine: QueryEngine,
    
    // Streaming checker
    checker: StreamingChecker,
    
    // RAG embeddings (optional)
    embeddings: Option<Arc<TypeEmbeddings>>,
    
    // Session metrics
    metrics: RwLock<SessionMetrics>,
}

/// Session metrics
#[derive(Debug, Clone, Default)]
pub struct SessionMetrics {
    pub queries_processed: u64,
    pub validations_performed: u64,
    pub branches_created: u64,
    pub errors_encountered: u64,
    pub latency_us_total: u64,
}

impl AgentSession {
    pub async fn new(
        base_universe: SharedUniverse,
        config: SessionConfig,
    ) -> Self {
        // Create isolated branch
        let branch = Branch::new(base_universe, config.isolation_level).await;
        let branch_arc = Arc::new(RwLock::new(branch));
        
        // Create query engine and checker for the branch
        let universe: crate::core::SharedUniverse = branch_arc.read().await.universe().clone();
        let query_engine = QueryEngine::new(universe.clone());
        let checker = StreamingChecker::new(universe);
        
        // Initialize RAG if enabled
        let embeddings = if config.enable_rag {
            Some(Arc::new(TypeEmbeddings::new()))
        } else {
            None
        };
        
        Self {
            id: SessionId::new(),
            config,
            universe: branch_arc,
            query_engine,
            checker,
            embeddings,
            metrics: RwLock::new(SessionMetrics::default()),
        }
    }
    
    /// Get the query engine for this session
    pub fn query_engine(&self) -> &QueryEngine {
        &self.query_engine
    }
    
    /// Get the streaming checker for this session
    pub fn checker(&self) -> &StreamingChecker {
        &self.checker
    }
    
    /// Get the branch (for advanced operations)
    pub fn branch(&self) -> &Arc<RwLock<Branch>> {
        &self.universe
    }
    
    /// Perform semantic search if RAG is enabled
    pub async fn semantic_search(&self, query: &str, limit: usize) -> Vec<super::rag::SearchResult> {
        if let Some(embeddings) = &self.embeddings {
            embeddings.search(query, limit).await
        } else {
            Vec::new()
        }
    }
    
    /// Commit changes to parent universe
    pub async fn commit(&self) -> Result<CommitResult, CommitError> {
        let branch: tokio::sync::RwLockReadGuard<'_, super::branch::Branch> = self.universe.read().await;
        branch.commit().await
    }
    
    /// Rollback to last checkpoint
    pub async fn rollback(&self) -> Result<(), RollbackError> {
        let mut branch = self.universe.write().await;
        branch.rollback().await
    }
    
    /// Create a sub-branch (for speculative exploration)
    pub async fn fork(&self, name: impl Into<String>) -> Result<SessionId, ForkError> {
        // Would create a nested branch
        Err(ForkError::NotImplemented)
    }
    
    /// Get session metrics
    pub async fn metrics(&self) -> SessionMetrics {
        self.metrics.read().await.clone()
    }
    
    /// Update metrics
    pub async fn record_query(&self, latency_us: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.queries_processed += 1;
        metrics.latency_us_total += latency_us;
    }
    
    /// Close the session
    pub async fn close(self) -> SessionSummary {
        SessionSummary {
            id: self.id,
            config: self.config,
            metrics: self.metrics.read().await.clone(),
        }
    }
}

/// Search result from semantic search
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub type_id: crate::core::TypeId,
    pub similarity: f32,
    pub description: String,
}

/// Commit result
#[derive(Debug, Clone)]
pub struct CommitResult {
    pub types_added: usize,
    pub types_modified: usize,
    pub conflicts: Vec<Conflict>,
}

/// Merge conflict
#[derive(Debug, Clone)]
pub struct Conflict {
    pub type_id: crate::core::TypeId,
    pub reason: ConflictReason,
}

#[derive(Debug, Clone)]
pub enum ConflictReason {
    ConcurrentModification,
    TypeMismatch,
    SymbolCollision,
}

/// Commit error
#[derive(Debug, Clone)]
pub enum CommitError {
    AlreadyCommitted,
    ParentChanged,
    ValidationFailed(Vec<String>),
}

/// Rollback error
#[derive(Debug, Clone)]
pub enum RollbackError {
    NothingToRollback,
    CheckpointCorrupted,
}

/// Fork error
#[derive(Debug, Clone)]
pub enum ForkError {
    MaxBranchesReached,
    NotImplemented,
}

/// Session summary after close
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: SessionId,
    pub config: SessionConfig,
    pub metrics: SessionMetrics,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    #[tokio::test]
    async fn test_session_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let config = SessionConfig::default();
        
        let session = AgentSession::new(universe, config).await;
        
        assert_eq!(session.config.name, "unnamed");
    }

    #[test]
    fn test_session_id() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        
        assert_ne!(id1, id2);
    }
}
