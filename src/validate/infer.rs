//! Type inference with look-ahead context
//!
//! Predicts types based on context to provide AI guidance.

use super::stream::{BinaryOp, Expression, LiteralValue, UnaryOp};
use crate::core::{PrimitiveType, SharedUniverse, TypeId, TypeKind};
use im::HashMap as ImHashMap;

/// Type inference engine with look-ahead support
pub struct TypeInference {
    universe: SharedUniverse,
    // Variable type bindings
    bindings: ImHashMap<String, TypeId>,
}

/// Context for look-ahead type inference
#[derive(Debug, Clone, Default)]
pub struct LookaheadContext {
    /// Expected return type (if in function context)
    pub expected_return: Option<TypeId>,
    /// Expected type for assignment target
    pub assignment_target: Option<TypeId>,
    /// Available identifiers in scope
    pub scope_bindings: ImHashMap<String, TypeId>,
    /// Function parameter types (if in call context)
    pub call_params: Vec<TypeId>,
    /// Current parameter index (if in call context)
    pub param_index: usize,
    /// Previous expression type (for chain inference)
    pub previous_type: Option<TypeId>,
    /// Sibling expression types
    pub sibling_types: Vec<TypeId>,
}

impl LookaheadContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create context expecting a specific type
    pub fn expecting(expected: TypeId) -> Self {
        Self {
            assignment_target: Some(expected),
            ..Default::default()
        }
    }

    /// Update with new binding
    pub fn with_binding(mut self, name: impl Into<String>, typ: TypeId) -> Self {
        self.scope_bindings = self.scope_bindings.update(name.into(), typ);
        self
    }

    /// Update for function call context
    pub fn in_call(mut self, params: Vec<TypeId>, index: usize) -> Self {
        self.call_params = params;
        self.param_index = index;
        self
    }
}

/// Inference result with confidence score
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub type_id: TypeId,
    pub confidence: f32,
    pub alternatives: Vec<(TypeId, f32)>,
}

impl TypeInference {
    pub fn new(universe: SharedUniverse) -> Self {
        Self {
            universe,
            bindings: ImHashMap::new(),
        }
    }

    /// Infer type of expression with look-ahead context
    pub fn infer(&self, expr: &Expression, ctx: &LookaheadContext) -> Option<TypeId> {
        match expr {
            Expression::Identifier(name) => self.infer_identifier(name, ctx),
            Expression::Literal(lit) => self.infer_literal(lit),
            Expression::Binary { op, left, right } => self.infer_binary_op(op, left, right, ctx),
            Expression::Unary { op, operand } => self.infer_unary_op(op, operand, ctx),
            Expression::Call { func, args } => self.infer_call(func, args, ctx),
            Expression::Selector { base, field } => self.infer_selector(base, field, ctx),
            Expression::TypeAssertion { expr: _, typ } => Some(*typ),
            Expression::Composite { typ, elements: _ } => Some(*typ),
            _ => ctx.assignment_target,
        }
    }

    fn infer_identifier(&self, name: &str, ctx: &LookaheadContext) -> Option<TypeId> {
        // Check context bindings first
        if let Some(&typ) = ctx.scope_bindings.get(name) {
            return Some(typ);
        }

        // Check local bindings
        if let Some(&typ) = self.bindings.get(name) {
            return Some(typ);
        }

        // Check universe symbols
        let symbol = self.universe.symbols().lookup(None, name)?;
        let entity = self.universe.lookup_by_symbol(symbol);

        // TODO: Map entity to type
        ctx.assignment_target
    }

    fn infer_literal(&self, lit: &LiteralValue) -> Option<TypeId> {
        let prim = match lit {
            LiteralValue::Int(_) => PrimitiveType::UntypedInt,
            LiteralValue::Float(_) => PrimitiveType::UntypedFloat,
            LiteralValue::String(_) => PrimitiveType::UntypedString,
            LiteralValue::Bool(_) => PrimitiveType::UntypedBool,
            LiteralValue::Nil => PrimitiveType::UntypedNil,
        };

        // Find primitive type in universe
        // Simplified: return first type as placeholder
        self.universe.get_type(TypeId(2)).map(|_| TypeId(2))
    }

    fn infer_binary_op(
        &self,
        op: &BinaryOp,
        left: &Expression,
        right: &Expression,
        ctx: &LookaheadContext,
    ) -> Option<TypeId> {
        let left_type = self.infer(left, ctx)?;
        let right_type = self.infer(right, ctx)?;

        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                // Numeric operations
                Some(self.unify_numeric(left_type, right_type)?)
            }
            BinaryOp::And | BinaryOp::Or => {
                // Boolean operations
                Some(left_type) // Should be bool
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                // Comparison operations return bool
                self.universe.get_type(TypeId(1)).map(|_| TypeId(1)) // bool
            }
            _ => Some(left_type),
        }
    }

    fn infer_unary_op(
        &self,
        op: &UnaryOp,
        operand: &Expression,
        ctx: &LookaheadContext,
    ) -> Option<TypeId> {
        let operand_type = self.infer(operand, ctx)?;

        match op {
            UnaryOp::Not => {
                // ! operator returns bool
                self.universe.get_type(TypeId(1)).map(|_| TypeId(1))
            }
            UnaryOp::Neg | UnaryOp::Pos => {
                // Numeric operators preserve type
                Some(operand_type)
            }
            UnaryOp::Addr => {
                // & operator creates pointer
                // Would create pointer type
                Some(operand_type)
            }
            _ => Some(operand_type),
        }
    }

    fn infer_call(
        &self,
        func: &Expression,
        _args: &[Expression],
        ctx: &LookaheadContext,
    ) -> Option<TypeId> {
        let func_type = self.infer(func, ctx)?;

        // Look up function type and extract return type
        if let Some(typ) = self.universe.get_type(func_type) {
            match &typ.kind {
                TypeKind::Func { results, .. } => {
                    // Return first result type
                    results.first().map(|r| r.typ)
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn infer_selector(
        &self,
        base: &Expression,
        _field: &str,
        ctx: &LookaheadContext,
    ) -> Option<TypeId> {
        let base_type = self.infer(base, ctx)?;

        // Look up field on base type
        if let Some(typ) = self.universe.get_type(base_type) {
            match &typ.kind {
                TypeKind::Struct { fields } => {
                    // Find field and return its type
                    fields
                        .iter()
                        .find(|f| f.name.as_ref() == _field)
                        .map(|f| f.typ)
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn unify_numeric(&self, left: TypeId, right: TypeId) -> Option<TypeId> {
        // Simple numeric unification
        // In full implementation, would check type compatibility
        if left == right {
            Some(left)
        } else {
            // Default to larger type
            Some(left)
        }
    }

    /// Look-ahead inference: predict likely type for incomplete expression
    pub fn lookahead_predict(&self, partial: &str, ctx: &LookaheadContext) -> Vec<(TypeId, f32)> {
        let mut predictions = Vec::new();

        // Based on context, predict likely types
        if let Some(expected) = ctx.assignment_target {
            predictions.push((expected, 0.9));
        }

        // Check if partial matches any known identifiers
        // This would use prefix matching against symbol table

        predictions
    }

    /// Infer type for function parameter at given index
    pub fn infer_param_type(
        &self,
        func_expr: &Expression,
        param_index: usize,
        ctx: &LookaheadContext,
    ) -> Option<TypeId> {
        let func_type = self.infer(func_expr, ctx)?;

        if let Some(typ) = self.universe.get_type(func_type) {
            match &typ.kind {
                TypeKind::Func { params, .. } => params.get(param_index).map(|p| p.typ),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Bind variable name to type
    pub fn bind(&mut self, name: impl Into<String>, typ: TypeId) {
        self.bindings = self.bindings.update(name.into(), typ);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;
    use std::sync::Arc;

    fn setup_inference() -> TypeInference {
        let universe = Arc::new(TypeUniverse::new());
        TypeInference::new(universe)
    }

    #[test]
    fn test_literal_inference() {
        let inference = setup_inference();

        let lit = Expression::Literal(LiteralValue::Int(42));
        let typ = inference.infer(&lit, &LookaheadContext::default());

        // Should infer some type
        assert!(typ.is_some());
    }

    #[test]
    fn test_lookahead_context() {
        let ctx = LookaheadContext::new()
            .with_binding("x", TypeId(1))
            .in_call(vec![TypeId(2), TypeId(3)], 0);

        assert_eq!(ctx.scope_bindings.get("x"), Some(&TypeId(1)));
        assert_eq!(ctx.param_index, 0);
    }
}
