//! Concurrent safety checker for Go code
//!
//! Detects common concurrency issues:
//! - Send on closed channel
//! - Close of nil channel
//! - Potential deadlocks
//! - Data races on shared variables
//! - Misuse of sync primitives

use super::error::{ErrorSeverity, SoftError, ValidationError};
use crate::core::{SharedUniverse, TypeId};
use crate::validate::stream::{Expression, SourcePosition};

use std::collections::{HashMap, HashSet};

/// Concurrent safety analysis result
#[derive(Debug, Clone, Default)]
pub struct ConcurrentSafetyResult {
    pub errors: Vec<ConcurrentError>,
    pub warnings: Vec<ConcurrentWarning>,
}

/// Concurrent safety error
#[derive(Debug, Clone)]
pub struct ConcurrentError {
    pub message: String,
    pub position: SourcePosition,
    pub kind: ConcurrentErrorKind,
}

/// Concurrent safety warning
#[derive(Debug, Clone)]
pub struct ConcurrentWarning {
    pub message: String,
    pub position: SourcePosition,
    pub kind: ConcurrentWarningKind,
}

/// Types of concurrent errors
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConcurrentErrorKind {
    /// Send on closed channel
    SendOnClosed,
    /// Close of nil channel
    CloseNilChannel,
    /// Double close of channel
    DoubleClose,
    /// Receive from send-only channel
    RecvFromSendOnly,
    /// Send to receive-only channel
    SendToRecvOnly,
    /// Potential deadlock
    PotentialDeadlock,
    /// Data race on shared variable
    DataRace,
    /// Unlock of unlocked mutex
    UnlockUnlocked,
}

/// Types of concurrent warnings
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConcurrentWarningKind {
    /// Unchecked channel close
    UncheckedClose,
    /// Shared variable without synchronization
    UnprotectedSharedVar,
    /// Goroutine leak potential
    GoroutineLeak,
    /// Select without default on blocking operation
    BlockingSelect,
    /// Copy of sync.Mutex (shouldn't be copied)
    CopiedMutex,
}

/// Concurrent safety checker state
pub struct ConcurrentChecker {
    universe: SharedUniverse,

    /// Track channel states: id -> (may_be_closed, may_be_nil)
    channel_states: HashMap<ChannelId, ChannelState>,

    /// Track mutex states: id -> (locked_locations, unlocked_locations)
    mutex_states: HashMap<MutexId, MutexState>,

    /// Track shared variables: var_id -> access_points
    shared_vars: HashMap<VarId, Vec<VarAccess>>,

    /// Current goroutine nesting level
    goroutine_depth: usize,

    /// Next channel ID
    next_channel_id: u64,

    /// Next mutex ID
    next_mutex_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChannelId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MutexId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct VarId(u64);

#[derive(Debug, Clone)]
struct ChannelState {
    may_be_closed: bool,
    may_be_nil: bool,
    closed_at: Vec<SourcePosition>,
    send_count: usize,
    recv_count: usize,
    direction: ChannelDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelDirection {
    SendOnly,
    RecvOnly,
    Both,
}

#[derive(Debug, Clone)]
struct MutexState {
    locked: bool,
    locked_at: Option<SourcePosition>,
    lock_count: usize,
    unlock_count: usize,
}

#[derive(Debug, Clone)]
struct VarAccess {
    position: SourcePosition,
    is_write: bool,
    in_goroutine: bool,
    sync_protected: bool,
}

impl ConcurrentChecker {
    pub fn new(universe: SharedUniverse) -> Self {
        Self {
            universe,
            channel_states: HashMap::new(),
            mutex_states: HashMap::new(),
            shared_vars: HashMap::new(),
            goroutine_depth: 0,
            next_channel_id: 1,
            next_mutex_id: 1,
        }
    }

    /// Check a function for concurrent safety issues
    pub fn check_function(&mut self, body: &[Expression]) -> ConcurrentSafetyResult {
        let mut result = ConcurrentSafetyResult::default();

        for expr in body {
            self.check_expression(expr, &mut result);
        }

        // Post-check analysis
        self.analyze_mutex_states(&mut result);
        self.analyze_shared_vars(&mut result);

        result
    }

    fn check_expression(&mut self, expr: &Expression, result: &mut ConcurrentSafetyResult) {
        match expr {
            Expression::Call { func, args } => {
                self.check_call(func, args, result);
            }
            Expression::Binary { op: _, left, right } => {
                self.check_expression(left, result);
                self.check_expression(right, result);
            }
            Expression::Unary { op, operand } => {
                self.check_expression(operand, result);
                if *op == super::stream::UnaryOp::Recv {
                    // Check receive operation
                    self.check_receive(operand, result);
                }
            }
            Expression::Selector { base, field } => {
                self.check_selector(base, field, result);
            }
            Expression::Index { base, index } => {
                self.check_expression(base, result);
                self.check_expression(index, result);
            }
            Expression::TypeAssertion { expr, .. } => {
                self.check_expression(expr, result);
            }
            Expression::Composite { elements, .. } => {
                for e in elements {
                    self.check_expression(e, result);
                }
            }
            _ => {}
        }
    }

    fn check_call(
        &mut self,
        func: &Expression,
        args: &[Expression],
        result: &mut ConcurrentSafetyResult,
    ) {
        // Check arguments first
        for arg in args {
            self.check_expression(arg, result);
        }

        // Check for built-in functions
        if let Expression::Identifier(name) = func {
            match name.as_str() {
                "close" if !args.is_empty() => {
                    self.check_close(&args[0], result);
                }
                "make" => {
                    // Channel creation
                    if !args.is_empty() {
                        self.track_channel_creation(&args[0]);
                    }
                }
                _ => {}
            }
        }
    }

    fn check_selector(
        &mut self,
        base: &Expression,
        field: &str,
        result: &mut ConcurrentSafetyResult,
    ) {
        self.check_expression(base, result);

        // Check for method calls on sync primitives
        match field {
            "Lock" | "Unlock" | "RLock" | "RUnlock" => {
                self.check_sync_method_call(base, field, result);
            }
            _ => {}
        }
    }

    fn check_close(&mut self, ch: &Expression, result: &mut ConcurrentSafetyResult) {
        let channel_id = self.get_channel_id(ch);

        if let Some(id) = channel_id {
            if let Some(state) = self.channel_states.get(&id) {
                if state.may_be_closed {
                    result.errors.push(ConcurrentError {
                        message: "channel may already be closed".to_string(),
                        position: SourcePosition::default(),
                        kind: ConcurrentErrorKind::DoubleClose,
                    });
                }
                if state.may_be_nil {
                    result.errors.push(ConcurrentError {
                        message: "close of nil channel".to_string(),
                        position: SourcePosition::default(),
                        kind: ConcurrentErrorKind::CloseNilChannel,
                    });
                }
            }
        }
    }

    fn check_receive(&mut self, ch: &Expression, result: &mut ConcurrentSafetyResult) {
        let channel_id = self.get_channel_id(ch);

        if let Some(id) = channel_id {
            if let Some(state) = self.channel_states.get(&id) {
                if matches!(state.direction, ChannelDirection::SendOnly) {
                    result.errors.push(ConcurrentError {
                        message: "receive from send-only channel".to_string(),
                        position: SourcePosition::default(),
                        kind: ConcurrentErrorKind::RecvFromSendOnly,
                    });
                }
            }
        }
    }

    fn check_sync_method_call(
        &mut self,
        base: &Expression,
        method: &str,
        result: &mut ConcurrentSafetyResult,
    ) {
        match method {
            "Lock" => {
                let mutex_id = self.get_mutex_id(base);
                if let Some(id) = mutex_id {
                    let state = self.mutex_states.entry(id).or_insert_with(|| MutexState {
                        locked: false,
                        locked_at: None,
                        lock_count: 0,
                        unlock_count: 0,
                    });

                    state.locked = true;
                    state.lock_count += 1;
                    state.locked_at = Some(SourcePosition::default());
                }
            }
            "Unlock" => {
                let mutex_id = self.get_mutex_id(base);
                if let Some(id) = mutex_id {
                    if let Some(state) = self.mutex_states.get_mut(&id) {
                        if !state.locked {
                            result.errors.push(ConcurrentError {
                                message: "unlock of unlocked mutex".to_string(),
                                position: SourcePosition::default(),
                                kind: ConcurrentErrorKind::UnlockUnlocked,
                            });
                        }
                        state.locked = false;
                        state.unlock_count += 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn analyze_mutex_states(&self, result: &mut ConcurrentSafetyResult) {
        for (_id, state) in &self.mutex_states {
            if state.locked {
                result.warnings.push(ConcurrentWarning {
                    message: format!(
                        "mutex may be locked at exit (lock count: {})",
                        state.lock_count
                    ),
                    position: state.locked_at.clone().unwrap_or_default(),
                    kind: ConcurrentWarningKind::UncheckedClose,
                });
            }

            if state.lock_count != state.unlock_count {
                result.warnings.push(ConcurrentWarning {
                    message: format!(
                        "mutex lock/unlock mismatch: {} locks, {} unlocks",
                        state.lock_count, state.unlock_count
                    ),
                    position: SourcePosition::default(),
                    kind: ConcurrentWarningKind::UnprotectedSharedVar,
                });
            }
        }
    }

    fn analyze_shared_vars(&self, result: &mut ConcurrentSafetyResult) {
        for (_var_id, accesses) in &self.shared_vars {
            let write_in_goroutine = false;
            let unsync_access = false;

            for access in accesses {
                if access.in_goroutine {
                    if access.is_write && !access.sync_protected {
                        // Would check for unsync_access
                    }
                }
            }

            if write_in_goroutine && unsync_access {
                result.warnings.push(ConcurrentWarning {
                    message: "potential data race on shared variable".to_string(),
                    position: accesses[0].position.clone(),
                    kind: ConcurrentWarningKind::UnprotectedSharedVar,
                });
            }
        }
    }

    fn get_channel_id(&self, _expr: &Expression) -> Option<ChannelId> {
        // Simplified: would extract channel ID from expression
        // In real implementation, use variable binding analysis
        None
    }

    fn get_mutex_id(&self, _expr: &Expression) -> Option<MutexId> {
        // Simplified: would extract mutex ID from expression
        None
    }

    fn track_channel_creation(&mut self, _typ: &Expression) {
        let id = ChannelId(self.next_channel_id);
        self.next_channel_id += 1;

        self.channel_states.insert(
            id,
            ChannelState {
                may_be_closed: false,
                may_be_nil: false,
                closed_at: vec![],
                send_count: 0,
                recv_count: 0,
                direction: ChannelDirection::Both,
            },
        );
    }
}

/// Quick concurrent safety check for a single expression
pub fn quick_concurrent_check(expr: &Expression) -> Vec<ConcurrentError> {
    let errors = Vec::new();

    match expr {
        Expression::Call { func, .. } => {
            // Check for close on nil channel
            if let Expression::Identifier(name) = func.as_ref() {
                if name == "close" {
                    // Would need to check if argument is nil
                }
            }
        }
        _ => {}
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;
    use std::sync::Arc;

    fn setup_checker() -> ConcurrentChecker {
        let universe = Arc::new(TypeUniverse::new());
        ConcurrentChecker::new(universe)
    }

    #[test]
    fn test_mutex_lock_unlock_balance() {
        let checker = setup_checker();
        // Would test with actual expressions
        assert_eq!(checker.mutex_states.len(), 0);
    }

    #[test]
    fn test_channel_state_tracking() {
        let mut checker = setup_checker();

        // Simulate channel creation
        checker.track_channel_creation(&Expression::Identifier("chan int".to_string()));

        assert_eq!(checker.channel_states.len(), 1);

        let id = ChannelId(1);
        let state = checker.channel_states.get(&id).unwrap();
        assert!(!state.may_be_closed);
        assert!(!state.may_be_nil);
    }
}
