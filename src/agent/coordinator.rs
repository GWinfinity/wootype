//! Agent coordinator for managing multiple AI Agents
//!
//! Central coordination for Cursor, Claude Code, Gemini CLI, etc.

use super::branch::BranchManager;
use super::session::{AgentSession, SessionConfig, SessionId, SessionSummary};
use crate::core::SharedUniverse;

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unique agent ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentId(pub u64);

impl AgentId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Agent metadata
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: AgentId,
    pub name: String,
    pub agent_type: super::session::AgentType,
    pub session_id: SessionId,
    pub connected_at: std::time::Instant,
}

/// Central coordinator for all AI Agents
pub struct AgentCoordinator {
    /// Base universe (shared across all agents)
    base_universe: SharedUniverse,

    /// Active sessions
    sessions: DashMap<SessionId, Arc<RwLock<AgentSession>>>,

    /// Agent info registry
    agents: DashMap<AgentId, AgentInfo>,

    /// Branch manager
    branch_manager: Arc<BranchManager>,

    /// Configuration
    config: CoordinatorConfig,

    /// Metrics
    metrics: RwLock<CoordinatorMetrics>,
}

/// Coordinator configuration
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub max_agents: usize,
    pub max_branches_per_agent: usize,
    pub default_isolation: super::session::IsolationLevel,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_agents: 100,
            max_branches_per_agent: 10,
            default_isolation: super::session::IsolationLevel::Full,
        }
    }
}

/// Coordinator metrics
#[derive(Debug, Clone, Default)]
pub struct CoordinatorMetrics {
    pub total_sessions_created: u64,
    pub total_sessions_closed: u64,
    pub total_commits: u64,
    pub total_rollbacks: u64,
    pub active_agents: u64,
    pub conflicts_resolved: u64,
}

/// Connection request from an AI Agent
#[derive(Debug, Clone)]
pub struct ConnectionRequest {
    pub agent_id: AgentId,
    pub name: String,
    pub agent_type: super::session::AgentType,
    pub preferred_isolation: Option<super::session::IsolationLevel>,
}

/// Connection result
#[derive(Debug, Clone)]
pub enum ConnectionResult {
    Connected { session_id: SessionId },
    Rejected { reason: RejectionReason },
}

/// Rejection reason
#[derive(Debug, Clone)]
pub enum RejectionReason {
    MaxAgentsReached,
    InvalidAgentType,
    AlreadyConnected,
}

impl AgentCoordinator {
    pub fn new(base_universe: SharedUniverse) -> Self {
        let config = CoordinatorConfig::default();
        let branch_manager = Arc::new(BranchManager::new(
            config.max_agents * config.max_branches_per_agent,
        ));

        Self {
            base_universe,
            sessions: DashMap::new(),
            agents: DashMap::new(),
            branch_manager,
            config,
            metrics: RwLock::new(CoordinatorMetrics::default()),
        }
    }

    /// Connect an AI Agent
    pub async fn connect(&self, request: ConnectionRequest) -> ConnectionResult {
        // Check max agents
        if self.sessions.len() >= self.config.max_agents {
            return ConnectionResult::Rejected {
                reason: RejectionReason::MaxAgentsReached,
            };
        }

        // Check if already connected
        if self.agents.contains_key(&request.agent_id) {
            return ConnectionResult::Rejected {
                reason: RejectionReason::AlreadyConnected,
            };
        }

        // Create session config
        let session_config = SessionConfig {
            name: request.name.clone(),
            agent_type: request.agent_type,
            enable_rag: true,
            max_branches: self.config.max_branches_per_agent,
            isolation_level: request
                .preferred_isolation
                .unwrap_or(self.config.default_isolation),
        };

        // Create session
        let session = AgentSession::new(self.base_universe.clone(), session_config.clone()).await;

        let session_id = session.id;
        let session_arc = Arc::new(RwLock::new(session));

        // Register agent
        let agent_info = AgentInfo {
            id: request.agent_id,
            name: request.name,
            agent_type: request.agent_type,
            session_id,
            connected_at: std::time::Instant::now(),
        };

        self.agents.insert(request.agent_id, agent_info);
        self.sessions.insert(session_id, session_arc);

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_sessions_created += 1;
            metrics.active_agents = self.agents.len() as u64;
        }

        ConnectionResult::Connected { session_id }
    }

    /// Disconnect an AI Agent
    pub async fn disconnect(&self, agent_id: AgentId) -> Option<SessionSummary> {
        let agent_info = self.agents.remove(&agent_id).map(|(_, info)| info)?;

        // Remove session
        if let Some((_, session)) = self.sessions.remove(&agent_info.session_id) {
            let session = Arc::try_unwrap(session).ok().map(|s| s.into_inner());

            if let Some(s) = session {
                let summary = s.close().await;

                // Update metrics
                let mut metrics = self.metrics.write().await;
                metrics.total_sessions_closed += 1;
                metrics.active_agents = self.agents.len() as u64;

                return Some(summary);
            }
        }

        None
    }

    /// Get session by ID
    pub fn get_session(&self, session_id: SessionId) -> Option<Arc<RwLock<AgentSession>>> {
        self.sessions.get(&session_id).map(|s| s.clone())
    }

    /// Get session for agent
    pub fn get_agent_session(&self, agent_id: AgentId) -> Option<Arc<RwLock<AgentSession>>> {
        self.agents
            .get(&agent_id)
            .and_then(|info| self.sessions.get(&info.session_id))
            .map(|s| s.clone())
    }

    /// List all connected agents
    pub fn list_agents(&self) -> Vec<AgentInfo> {
        self.agents.iter().map(|e| e.clone()).collect()
    }

    /// Get active session count
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get coordinator metrics
    pub async fn metrics(&self) -> CoordinatorMetrics {
        self.metrics.read().await.clone()
    }

    /// Broadcast a message to all agents
    pub async fn broadcast(&self, message: CoordinatorMessage) {
        for session_ref in self.sessions.iter() {
            let session = session_ref.value().read().await;
            // Would send message to session
            let _ = message.clone();
        }
    }

    /// Commit a session's changes to the base universe
    pub async fn commit_session(&self, session_id: SessionId) -> Result<(), CommitError> {
        if let Some(session) = self.sessions.get(&session_id) {
            let session = session.read().await;
            let result = session.commit().await;

            if result.is_ok() {
                let mut metrics = self.metrics.write().await;
                metrics.total_commits += 1;
            }

            result
                .map(|_| ())
                .map_err(|e| CommitError::SessionError(format!("{:?}", e)))
        } else {
            Err(CommitError::SessionNotFound)
        }
    }
}

/// Commit error
#[derive(Debug, Clone)]
pub enum CommitError {
    SessionNotFound,
    SessionError(String),
    MergeConflict,
}

/// Coordinator message
#[derive(Debug, Clone)]
pub enum CoordinatorMessage {
    TypeChanged { type_id: crate::core::TypeId },
    SessionCommitted { session_id: SessionId },
    GlobalRefresh,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let coordinator = AgentCoordinator::new(universe);

        assert_eq!(coordinator.session_count(), 0);
    }

    #[tokio::test]
    async fn test_agent_connection() {
        let universe = Arc::new(TypeUniverse::new());
        let coordinator = AgentCoordinator::new(universe);

        let request = ConnectionRequest {
            agent_id: AgentId::new(1),
            name: "Test Agent".to_string(),
            agent_type: super::super::session::AgentType::Generic,
            preferred_isolation: None,
        };

        let result = coordinator.connect(request).await;

        match result {
            ConnectionResult::Connected { .. } => {}
            _ => panic!("Expected successful connection"),
        }

        assert_eq!(coordinator.session_count(), 1);
    }

    #[tokio::test]
    async fn test_agent_disconnection() {
        let universe = Arc::new(TypeUniverse::new());
        let coordinator = AgentCoordinator::new(universe);

        let request = ConnectionRequest {
            agent_id: AgentId::new(1),
            name: "Test Agent".to_string(),
            agent_type: super::super::session::AgentType::Generic,
            preferred_isolation: None,
        };

        coordinator.connect(request).await;
        let summary = coordinator.disconnect(AgentId::new(1)).await;

        assert!(summary.is_some());
        assert_eq!(coordinator.session_count(), 0);
    }
}
