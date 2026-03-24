//! Branch isolation with copy-on-write semantics
//!
//! Each Agent session gets a branch that can be committed or rolled back.

use crate::core::universe::UniverseSnapshot;
use crate::core::{SharedUniverse, Type, TypeId, TypeUniverse};

use super::session::{
    CommitError, CommitResult, Conflict, ConflictReason, IsolationLevel, RollbackError,
};
use im::HashMap as ImHashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A branch of the type universe with copy-on-write semantics
pub struct Branch {
    /// Parent universe or branch
    parent: BranchParent,

    /// Isolation level
    isolation_level: IsolationLevel,

    /// Local universe (copy-on-write)
    universe: SharedUniverse,

    /// Modified types (local only)
    local_types: RwLock<ImHashMap<TypeId, Arc<Type>>>,

    /// Checkpoint for rollback
    checkpoint: RwLock<Option<UniverseSnapshot>>,

    /// Committed flag
    committed: RwLock<bool>,

    /// Base snapshot at branch creation
    base_snapshot: UniverseSnapshot,
}

/// Parent reference for a branch
enum BranchParent {
    Universe(SharedUniverse),
    Branch(Arc<RwLock<Branch>>),
}

impl Branch {
    /// Create a new branch from a parent universe
    pub async fn new(parent: SharedUniverse, isolation_level: IsolationLevel) -> Self {
        let base_snapshot = create_snapshot(&parent).await;

        // Create isolated universe based on isolation level
        let universe = match isolation_level {
            IsolationLevel::Full => {
                // Clone the universe for full isolation
                Arc::new(TypeUniverse::new()) // Simplified - would deep clone
            }
            IsolationLevel::SharedRead | IsolationLevel::Snapshot => {
                // Share the universe but track local changes
                parent.clone()
            }
        };

        Self {
            parent: BranchParent::Universe(parent),
            isolation_level,
            universe,
            local_types: RwLock::new(ImHashMap::new()),
            checkpoint: RwLock::new(None),
            committed: RwLock::new(false),
            base_snapshot,
        }
    }

    /// Get the universe for this branch
    pub fn universe(&self) -> &SharedUniverse {
        &self.universe
    }

    /// Get a type (checking local first, then parent)
    pub async fn get_type(&self, id: TypeId) -> Option<Arc<Type>> {
        // Check local modifications first
        if let Some(local) = self.local_types.read().await.get(&id).cloned() {
            return Some(local);
        }

        // Fall back to parent/universe
        // Note: To avoid recursion, we only check the immediate parent universe
        match &self.parent {
            BranchParent::Universe(u) => u.get_type(id),
            BranchParent::Branch(_) => None, // Simplified - would need non-recursive traversal
        }
    }

    /// Insert or update a type locally
    pub async fn insert_type(&self, id: TypeId, typ: Arc<Type>) {
        let mut local = self.local_types.write().await;
        *local = local.update(id, typ);
    }

    /// Create a checkpoint for rollback
    pub async fn checkpoint(&self) {
        let snapshot = create_snapshot(&self.universe).await;
        *self.checkpoint.write().await = Some(snapshot);
    }

    /// Rollback to checkpoint
    pub async fn rollback(&self) -> Result<(), RollbackError> {
        if let Some(checkpoint) = self.checkpoint.read().await.clone() {
            // Restore from checkpoint
            // In full implementation, would restore universe state
            *self.local_types.write().await = ImHashMap::new();
            Ok(())
        } else {
            Err(RollbackError::NothingToRollback)
        }
    }

    /// Commit changes to parent
    pub async fn commit(&self) -> Result<CommitResult, CommitError> {
        if *self.committed.read().await {
            return Err(CommitError::AlreadyCommitted);
        }

        let local_types = self.local_types.read().await.clone();
        let mut conflicts = Vec::new();

        // Check for conflicts with parent
        for (type_id, local_type) in local_types.iter() {
            if let Some(parent_type) = self.get_parent_type(*type_id).await {
                // Check if parent changed since branch creation
                if let Some(base_type) = self.base_snapshot.types.get(type_id) {
                    if !Arc::ptr_eq(&parent_type, &Arc::new(base_type.clone())) {
                        // Parent was modified
                        conflicts.push(Conflict {
                            type_id: *type_id,
                            reason: ConflictReason::ConcurrentModification,
                        });
                        continue;
                    }
                }

                // Check type compatibility
                if !types_compatible(&local_type, &parent_type) {
                    conflicts.push(Conflict {
                        type_id: *type_id,
                        reason: ConflictReason::TypeMismatch,
                    });
                }
            }
        }

        if !conflicts.is_empty() {
            return Err(CommitError::ValidationFailed(
                conflicts
                    .iter()
                    .map(|c| format!("{:?}", c.reason))
                    .collect(),
            ));
        }

        // Apply changes to parent
        match &self.parent {
            BranchParent::Universe(u) => {
                for (type_id, typ) in local_types.iter() {
                    u.insert_type(*type_id, typ.clone());
                }
            }
            BranchParent::Branch(b) => {
                let parent_branch = b.read().await;
                for (type_id, typ) in local_types.iter() {
                    parent_branch.insert_type(*type_id, typ.clone()).await;
                }
            }
        }

        *self.committed.write().await = true;

        Ok(CommitResult {
            types_added: local_types.len(),
            types_modified: 0, // Would track modifications separately
            conflicts,
        })
    }

    async fn get_parent_type(&self, id: TypeId) -> Option<Arc<Type>> {
        match &self.parent {
            BranchParent::Universe(u) => u.get_type(id),
            BranchParent::Branch(b) => b.read().await.get_type(id).await,
        }
    }

    /// Check if this branch has been committed
    pub async fn is_committed(&self) -> bool {
        *self.committed.read().await
    }

    /// Get local modification count
    pub async fn local_changes(&self) -> usize {
        self.local_types.read().await.len()
    }
}

/// Create snapshot of universe state
async fn create_snapshot(universe: &SharedUniverse) -> UniverseSnapshot {
    // Simplified - would deep clone state
    UniverseSnapshot::empty()
}

/// Check if two types are compatible
fn types_compatible(a: &Type, b: &Type) -> bool {
    a.id == b.id || a.fingerprint == b.fingerprint
}

/// Branch manager for tracking all active branches
pub struct BranchManager {
    branches: RwLock<im::HashMap<super::session::SessionId, Arc<RwLock<Branch>>>>,
    max_branches: usize,
}

impl BranchManager {
    pub fn new(max_branches: usize) -> Self {
        Self {
            branches: RwLock::new(im::HashMap::new()),
            max_branches,
        }
    }

    pub async fn create_branch(
        &self,
        session_id: super::session::SessionId,
        parent: SharedUniverse,
        isolation: IsolationLevel,
    ) -> Result<Arc<RwLock<Branch>>, BranchError> {
        let branches = self.branches.read().await;
        if branches.len() >= self.max_branches {
            return Err(BranchError::MaxBranchesReached);
        }
        drop(branches);

        let branch = Arc::new(RwLock::new(Branch::new(parent, isolation).await));

        let mut branches = self.branches.write().await;
        *branches = branches.update(session_id, branch.clone());

        Ok(branch)
    }

    pub async fn get_branch(
        &self,
        session_id: super::session::SessionId,
    ) -> Option<Arc<RwLock<Branch>>> {
        self.branches.read().await.get(&session_id).cloned()
    }

    pub async fn remove_branch(&self, session_id: super::session::SessionId) {
        let mut branches = self.branches.write().await;
        *branches = branches.without(&session_id);
    }

    pub async fn active_branch_count(&self) -> usize {
        self.branches.read().await.len()
    }
}

/// Branch error
#[derive(Debug, Clone)]
pub enum BranchError {
    MaxBranchesReached,
    ParentNotFound,
    InvalidIsolationLevel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    #[tokio::test]
    async fn test_branch_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let branch = Branch::new(universe, IsolationLevel::Full).await;

        assert!(!branch.is_committed().await);
    }

    #[tokio::test]
    async fn test_branch_local_changes() {
        let universe = Arc::new(TypeUniverse::new());
        let branch = Branch::new(universe, IsolationLevel::Full).await;

        let type_id = TypeId(1000);
        let typ = Arc::new(Type::new(
            type_id,
            crate::core::TypeKind::Primitive(crate::core::PrimitiveType::Int),
        ));

        branch.insert_type(type_id, typ).await;

        assert_eq!(branch.local_changes().await, 1);
    }
}
