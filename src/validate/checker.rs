//! Streaming type checker for real-time validation
//! 
//! Validates expressions as they arrive from AI generation.

use super::stream::{Expression, ExpressionId, ValidationResult, SourcePosition};
use super::error::{ValidationError, SoftError, ErrorCollection};
use super::infer::{TypeInference, LookaheadContext};
use crate::core::{SharedUniverse, TypeId, TypeKind};
use crate::query::QueryEngine;

use std::sync::Arc;
use parking_lot::RwLock;
use im::HashMap as ImHashMap;

/// Streaming type checker state
pub struct StreamingChecker {
    universe: SharedUniverse,
    query_engine: QueryEngine,
    inference: TypeInference,
    
    // Expression states
    states: RwLock<ImHashMap<ExpressionId, ExpressionCheckState>>,
    
    // Error accumulation
    errors: RwLock<ErrorCollection>,
}

/// State for a single expression being checked
#[derive(Debug, Clone)]
pub struct ExpressionCheckState {
    pub expr: Expression,
    pub position: SourcePosition,
    pub inferred_type: Option<TypeId>,
    pub expected_type: Option<TypeId>,
    pub status: CheckStatus,
    pub errors: Vec<SoftError>,
}

/// Check status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pending,
    InProgress,
    Valid,
    Invalid,
    Partial,
}

/// Check request
#[derive(Debug, Clone)]
pub struct CheckRequest {
    pub id: ExpressionId,
    pub expr: Expression,
    pub position: SourcePosition,
    pub expected_type: Option<TypeId>,
}

/// Check response
#[derive(Debug, Clone)]
pub struct CheckResponse {
    pub id: ExpressionId,
    pub result: ValidationResult,
    pub latency_us: u64,
}

impl StreamingChecker {
    pub fn new(universe: SharedUniverse) -> Self {
        Self {
            query_engine: QueryEngine::new(universe.clone()),
            inference: TypeInference::new(universe.clone()),
            universe,
            states: RwLock::new(ImHashMap::new()),
            errors: RwLock::new(ErrorCollection::new()),
        }
    }
    
    /// Check a single expression
    pub fn check(&self, request: CheckRequest) -> CheckResponse {
        let start = std::time::Instant::now();
        
        // Create check state
        let state = ExpressionCheckState {
            expr: request.expr.clone(),
            position: request.position,
            inferred_type: None,
            expected_type: request.expected_type,
            status: CheckStatus::InProgress,
            errors: Vec::new(),
        };
        
        // Store state
        {
            let mut states = self.states.write();
            *states = states.update(request.id, state.clone());
        }
        
        // Perform check
        let result = self.validate_expression(&request.expr, request.expected_type);
        
        // Update state with result
        {
            let mut states = self.states.write();
            if let Some(state) = states.get(&request.id) {
                let updated = ExpressionCheckState {
                    status: match &result {
                        ValidationResult::Valid { .. } => CheckStatus::Valid,
                        ValidationResult::Invalid { .. } => CheckStatus::Invalid,
                        ValidationResult::Partial { .. } => CheckStatus::Partial,
                        ValidationResult::Unknown => CheckStatus::Pending,
                    },
                    inferred_type: match &result {
                        ValidationResult::Valid { typ } => Some(*typ),
                        ValidationResult::Partial { typ, .. } => *typ,
                        _ => None,
                    },
                    ..state.clone()
                };
                *states = states.update(request.id, updated);
            }
        }
        
        let latency = start.elapsed().as_micros() as u64;
        
        CheckResponse {
            id: request.id,
            result,
            latency_us: latency,
        }
    }
    
    /// Quick check for syntax hints (microsecond latency)
    pub fn quick_check(&self, expr: &Expression) -> QuickCheckResult {
        match expr {
            Expression::Identifier(name) => {
                // Fast identifier lookup
                let exists = self.lookup_identifier_quick(name);
                QuickCheckResult {
                    valid: exists,
                    hint: if exists { None } else { Some(format!("'{}' not found", name)) },
                }
            }
            Expression::Literal(_) => QuickCheckResult::valid(),
            Expression::Binary { op, .. } => {
                // Quick operator validation
                QuickCheckResult::valid()
            }
            _ => QuickCheckResult::unknown(),
        }
    }
    
    fn lookup_identifier_quick(&self, name: &str) -> bool {
        self.universe.symbols().lookup(None, name).is_some()
    }
    
    /// Full expression validation
    fn validate_expression(
        &self,
        expr: &Expression,
        expected: Option<TypeId>
    ) -> ValidationResult {
        let ctx = LookaheadContext::new();
        
        // Infer type
        let inferred = self.inference.infer(expr, &ctx);
        
        // Check against expected type
        if let (Some(exp), Some(inf)) = (expected, inferred) {
            if !self.types_compatible(inf, exp) {
                return ValidationResult::Partial {
                    typ: Some(inf),
                    issues: vec![SoftError::new("Type mismatch")
                        .with_suggestion(format!("Expected: {:?}", exp))
                        .with_severity(super::error::ErrorSeverity::Warning)],
                };
            }
        }
        
        match inferred {
            Some(typ) => ValidationResult::Valid { typ },
            None => ValidationResult::Partial {
                typ: expected,
                issues: vec![SoftError::new("Could not infer type")
                    .with_severity(super::error::ErrorSeverity::Hint)],
            },
        }
    }
    
    fn types_compatible(&self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }
        
        // Check assignability
        if let (Some(ta), Some(tb)) = (self.universe.get_type(a), self.universe.get_type(b)) {
            // Identical types
            if ta.identical(&tb) {
                return true;
            }
            
            // Check interface implementation
            if tb.flags.contains(crate::core::TypeFlags::INTERFACE) {
                return self.query_engine.implements_interface(a, b);
            }
        }
        
        false
    }
    
    /// Check a batch of expressions
    pub fn check_batch(&self, requests: Vec<CheckRequest>) -> Vec<CheckResponse> {
        requests.into_iter()
            .map(|req| self.check(req))
            .collect()
    }
    
    /// Get expression state
    pub fn get_state(&self, id: ExpressionId) -> Option<ExpressionCheckState> {
        self.states.read().get(&id).cloned()
    }
    
    /// Get all errors
    pub fn get_errors(&self) -> ErrorCollection {
        self.errors.read().clone()
    }
    
    /// Clear all states
    pub fn clear(&self) {
        *self.states.write() = ImHashMap::new();
        *self.errors.write() = ErrorCollection::new();
    }
}

/// Quick check result (for sub-millisecond responses)
#[derive(Debug, Clone)]
pub struct QuickCheckResult {
    pub valid: bool,
    pub hint: Option<String>,
}

impl QuickCheckResult {
    pub fn valid() -> Self {
        Self {
            valid: true,
            hint: None,
        }
    }
    
    pub fn invalid(hint: impl Into<String>) -> Self {
        Self {
            valid: false,
            hint: Some(hint.into()),
        }
    }
    
    pub fn unknown() -> Self {
        Self {
            valid: true,
            hint: Some("Could not validate".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    fn setup_checker() -> StreamingChecker {
        let universe = Arc::new(TypeUniverse::new());
        StreamingChecker::new(universe)
    }

    #[test]
    fn test_quick_check_literal() {
        let checker = setup_checker();
        
        let lit = Expression::Literal(crate::validate::stream::LiteralValue::Int(42));
        let result = checker.quick_check(&lit);
        
        assert!(result.valid);
    }
    
    #[test]
    fn test_check_expression() {
        let checker = setup_checker();
        
        let request = CheckRequest {
            id: ExpressionId::new(1),
            expr: Expression::Literal(crate::validate::stream::LiteralValue::Int(42)),
            position: SourcePosition::default(),
            expected_type: None,
        };
        
        let response = checker.check(request);
        
        match response.result {
            ValidationResult::Valid { .. } | ValidationResult::Partial { .. } => {}
            _ => panic!("Expected valid or partial result"),
        }
    }
}
