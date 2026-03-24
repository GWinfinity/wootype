//! Streaming validation for AI token-by-token generation
//!
//! Adapts to AI generation patterns with real-time feedback.

use super::error::{ErrorSeverity, SoftError, ValidationError};
use super::infer::{LookaheadContext, TypeInference};
use crate::core::{SharedUniverse, TypeId};
use crate::query::QueryEngine;

use parking_lot::RwLock;

use tokio::sync::{broadcast, mpsc};

/// Events in the validation stream
#[derive(Debug, Clone)]
pub enum ValidationEvent {
    /// Token received (expression fragment)
    Token {
        text: String,
        position: SourcePosition,
    },
    /// Complete expression
    Expression {
        expr: Expression,
        position: SourcePosition,
    },
    /// Type annotation available
    TypeAnnotation {
        expr_id: ExpressionId,
        inferred_type: TypeId,
        confidence: f32,
    },
    /// Validation complete for expression
    Validated {
        expr_id: ExpressionId,
        result: ValidationResult,
    },
    /// Error detected
    Error {
        expr_id: ExpressionId,
        error: ValidationError,
    },
    /// Checkpoint for rollback
    Checkpoint { id: CheckpointId },
}

/// Source position in file
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SourcePosition {
    pub line: u32,
    pub column: u32,
    pub offset: u32,
}

impl SourcePosition {
    pub fn new(line: u32, column: u32, offset: u32) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }
}

/// Unique expression ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExpressionId(u64);

impl ExpressionId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Unique checkpoint ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CheckpointId(u64);

impl CheckpointId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Expression AST node (simplified)
#[derive(Debug, Clone)]
pub enum Expression {
    Identifier(String),
    Literal(LiteralValue),
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expression>,
    },
    Call {
        func: Box<Expression>,
        args: Vec<Expression>,
    },
    Selector {
        base: Box<Expression>,
        field: String,
    },
    Index {
        base: Box<Expression>,
        index: Box<Expression>,
    },
    TypeAssertion {
        expr: Box<Expression>,
        typ: TypeId,
    },
    Composite {
        typ: TypeId,
        elements: Vec<Expression>,
    },
}

#[derive(Debug, Clone)]
pub enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Nil,
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    Pos,
    BitNot,
    Deref,
    Addr,
    Recv, // Channel receive operation (<-ch)
}

/// Validation result
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Valid {
        typ: TypeId,
    },
    Invalid {
        errors: Vec<ValidationError>,
    },
    Partial {
        typ: Option<TypeId>,
        issues: Vec<SoftError>,
    },
    Unknown,
}

/// Streaming validation pipeline
pub struct ValidationStream {
    universe: SharedUniverse,
    query_engine: QueryEngine,
    inference: TypeInference,

    // Event channels
    input_tx: mpsc::Sender<ValidationEvent>,
    output_rx: broadcast::Receiver<StreamOutput>,

    // State
    expressions: RwLock<im::HashMap<ExpressionId, ExpressionState>>,
    checkpoints: RwLock<im::HashMap<CheckpointId, CheckpointState>>,
    expr_counter: std::sync::atomic::AtomicU64,
    checkpoint_counter: std::sync::atomic::AtomicU64,
}

/// Internal expression state
#[derive(Debug, Clone)]
struct ExpressionState {
    expr: Expression,
    position: SourcePosition,
    inferred_type: Option<TypeId>,
    validation_result: Option<ValidationResult>,
}

/// Checkpoint for rollback
#[derive(Debug, Clone)]
struct CheckpointState {
    expressions: im::HashMap<ExpressionId, ExpressionState>,
    scope: crate::core::symbol::Scope,
}

/// Output from validation stream
#[derive(Debug, Clone)]
pub struct StreamOutput {
    pub event: ValidationEvent,
    pub timestamp: std::time::Instant,
    pub latency_us: u64,
}

impl ValidationStream {
    pub fn new(universe: SharedUniverse) -> Self {
        let (input_tx, mut input_rx) = mpsc::channel(1024);
        let (output_tx, output_rx) = broadcast::channel(1024);

        let query_engine = QueryEngine::new(universe.clone());
        let inference = TypeInference::new(universe.clone());

        let stream = Self {
            universe: universe.clone(),
            query_engine,
            inference,
            input_tx,
            output_rx,
            expressions: RwLock::new(im::HashMap::new()),
            checkpoints: RwLock::new(im::HashMap::new()),
            expr_counter: std::sync::atomic::AtomicU64::new(1),
            checkpoint_counter: std::sync::atomic::AtomicU64::new(1),
        };

        // Spawn processing task
        let stream_clone = stream.clone_ref();
        tokio::spawn(async move {
            while let Some(event) = input_rx.recv().await {
                let start = std::time::Instant::now();
                stream_clone.process_event(event.clone()).await;
                let latency = start.elapsed().as_micros() as u64;

                let _ = output_tx.send(StreamOutput {
                    event,
                    timestamp: start,
                    latency_us: latency,
                });
            }
        });

        stream
    }

    fn clone_ref(&self) -> Self {
        Self {
            universe: self.universe.clone(),
            query_engine: QueryEngine::new(self.universe.clone()),
            inference: TypeInference::new(self.universe.clone()),
            input_tx: self.input_tx.clone(),
            output_rx: self.output_rx.resubscribe(),
            expressions: RwLock::new(self.expressions.read().clone()),
            checkpoints: RwLock::new(self.checkpoints.read().clone()),
            expr_counter: std::sync::atomic::AtomicU64::new(
                self.expr_counter.load(std::sync::atomic::Ordering::SeqCst),
            ),
            checkpoint_counter: std::sync::atomic::AtomicU64::new(
                self.checkpoint_counter
                    .load(std::sync::atomic::Ordering::SeqCst),
            ),
        }
    }

    /// Submit event to validation stream
    pub async fn submit(
        &self,
        event: ValidationEvent,
    ) -> Result<(), mpsc::error::SendError<ValidationEvent>> {
        self.input_tx.send(event).await
    }

    /// Get output receiver
    pub fn subscribe(&self) -> broadcast::Receiver<StreamOutput> {
        self.output_rx.resubscribe()
    }

    /// Process validation event
    async fn process_event(&self, event: ValidationEvent) {
        match event {
            ValidationEvent::Token { text, position } => {
                self.process_token(&text, position).await;
            }
            ValidationEvent::Expression { expr, position } => {
                self.process_expression(expr, position).await;
            }
            ValidationEvent::Checkpoint { id } => {
                self.create_checkpoint(id).await;
            }
            _ => {}
        }
    }

    async fn process_token(&self, _text: &str, _position: SourcePosition) {
        // Token-level processing for look-ahead inference
        // Analyze partial tokens to predict completion
    }

    async fn process_expression(&self, expr: Expression, position: SourcePosition) {
        let id = self.next_expr_id();

        // Infer type with look-ahead
        let inferred = self.inference.infer(&expr, &LookaheadContext::default());

        // Validate expression
        let result = self.validate_expression(&expr, inferred.as_ref());

        // Store state
        let state = ExpressionState {
            expr: expr.clone(),
            position,
            inferred_type: inferred,
            validation_result: Some(result.clone()),
        };

        {
            let mut expressions = self.expressions.write();
            *expressions = expressions.update(id, state);
        }

        // Emit validation result
        // This would typically be sent back via output channel
    }

    fn validate_expression(
        &self,
        expr: &Expression,
        expected_type: Option<&TypeId>,
    ) -> ValidationResult {
        match expr {
            Expression::Identifier(name) => self.validate_identifier(name, expected_type),
            Expression::Binary { op, left, right } => {
                self.validate_binary_op(*op, left, right, expected_type)
            }
            Expression::Call { func, args } => self.validate_call(func, args, expected_type),
            Expression::Selector { base, field } => {
                self.validate_selector(base, field, expected_type)
            }
            _ => ValidationResult::Unknown,
        }
    }

    fn validate_identifier(&self, name: &str, expected: Option<&TypeId>) -> ValidationResult {
        // Look up identifier in scope
        // For now, return partial with soft error
        ValidationResult::Partial {
            typ: expected.copied(),
            issues: vec![SoftError {
                message: format!("Unresolved identifier: {}", name),
                suggestion: None,
                severity: ErrorSeverity::Hint,
            }],
        }
    }

    fn validate_binary_op(
        &self,
        op: BinaryOp,
        left: &Expression,
        right: &Expression,
        expected: Option<&TypeId>,
    ) -> ValidationResult {
        let left_result = self.validate_expression(left, None);
        let right_result = self.validate_expression(right, None);

        // Check operator compatibility
        match (left_result, right_result) {
            (
                ValidationResult::Valid { typ: left_type },
                ValidationResult::Valid { typ: right_type },
            ) => {
                if self.types_compatible_for_op(op, left_type, right_type) {
                    ValidationResult::Valid {
                        typ: self.result_type_for_op(op, left_type, right_type),
                    }
                } else {
                    ValidationResult::Invalid {
                        errors: vec![ValidationError::TypeMismatch {
                            expected: left_type,
                            found: right_type,
                        }],
                    }
                }
            }
            (ValidationResult::Partial { .. }, _) | (_, ValidationResult::Partial { .. }) => {
                ValidationResult::Partial {
                    typ: expected.copied(),
                    issues: vec![],
                }
            }
            _ => ValidationResult::Unknown,
        }
    }

    fn types_compatible_for_op(&self, op: BinaryOp, left: TypeId, right: TypeId) -> bool {
        // Simplified compatibility check
        left == right || self.is_numeric_op(op)
    }

    fn is_numeric_op(&self, op: BinaryOp) -> bool {
        matches!(
            op,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
        )
    }

    fn result_type_for_op(&self, _op: BinaryOp, left: TypeId, _right: TypeId) -> TypeId {
        left // Simplified
    }

    fn validate_call(
        &self,
        func: &Expression,
        args: &[Expression],
        expected: Option<&TypeId>,
    ) -> ValidationResult {
        // Validate function expression
        let _func_result = self.validate_expression(func, None);

        // Validate arguments
        for arg in args {
            let _ = self.validate_expression(arg, None);
        }

        // Check arity and types
        ValidationResult::Partial {
            typ: expected.copied(),
            issues: vec![],
        }
    }

    fn validate_selector(
        &self,
        base: &Expression,
        field: &str,
        expected: Option<&TypeId>,
    ) -> ValidationResult {
        let base_result = self.validate_expression(base, None);

        // Check if base type has field
        match base_result {
            ValidationResult::Valid { typ: _ } => {
                // Look up field on type
                ValidationResult::Partial {
                    typ: expected.copied(),
                    issues: vec![SoftError {
                        message: format!("Field lookup: .{}", field),
                        suggestion: None,
                        severity: ErrorSeverity::Hint,
                    }],
                }
            }
            _ => base_result,
        }
    }

    async fn create_checkpoint(&self, id: CheckpointId) {
        let expressions = self.expressions.read().clone();
        let scope = self.universe.current_scope();

        let checkpoint = CheckpointState { expressions, scope };

        let mut checkpoints = self.checkpoints.write();
        *checkpoints = checkpoints.update(id, checkpoint);
    }

    /// Rollback to checkpoint
    pub fn rollback(&self, checkpoint_id: CheckpointId) -> bool {
        if let Some(checkpoint) = self.checkpoints.read().get(&checkpoint_id).cloned() {
            *self.expressions.write() = checkpoint.expressions.clone();
            // Restore scope
            true
        } else {
            false
        }
    }

    fn next_expr_id(&self) -> ExpressionId {
        ExpressionId::new(
            self.expr_counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        )
    }

    /// Create a new checkpoint and return its ID
    pub async fn checkpoint(&self) -> CheckpointId {
        let id = CheckpointId::new(
            self.checkpoint_counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        let _: Result<(), _> = self.submit(ValidationEvent::Checkpoint { id }).await;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_stream_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let stream = ValidationStream::new(universe);

        let expr = Expression::Identifier("x".to_string());
        stream
            .submit(ValidationEvent::Expression {
                expr,
                position: SourcePosition::default(),
            })
            .await
            .unwrap();
    }
}
