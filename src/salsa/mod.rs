//! Salsa-inspired incremental type checking
//!
//! Simplified implementation of Salsa concepts for wootype.
//! Uses a query-based system with memoization.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use parking_lot::RwLock;

pub mod db;
pub mod inputs;
pub mod lsp;

pub use db::*;
pub use inputs::*;
pub use lsp::*;

/// Query key for memoization
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum QueryKey {
    ParseFile(String),        // file path
    InferFunction(String),    // function name
    ResolveImport(String),    // import path
}

/// Query result storage
#[derive(Clone, Debug)]
pub enum QueryValue {
    ParseResult(ParseResult),
    FunctionType(FunctionTypeResult),
    ImportResolution(ImportResolution),
}

/// Incremental database - the core of our type system
pub struct IncrementalDb {
    /// Input storage
    inputs: Arc<RwLock<InputStorage>>,
    /// Query cache
    cache: Arc<RwLock<HashMap<QueryKey, QueryValue>>>,
    /// Dependency tracking: which queries depend on which inputs
    dependencies: Arc<RwLock<HashMap<QueryKey, Vec<String>>>>, // query -> input files
    /// Input versions for invalidation
    versions: Arc<RwLock<HashMap<String, u64>>>,
}

#[derive(Default)]
struct InputStorage {
    files: HashMap<String, String>, // path -> content
    functions: HashMap<String, FunctionDef>, // name -> definition
}

#[derive(Clone, Debug)]
struct FunctionDef {
    body: FunctionBody,
    version: u64,
}

impl IncrementalDb {
    pub fn new() -> Self {
        Self {
            inputs: Arc::new(RwLock::new(InputStorage::default())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            dependencies: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Set a source file input
    pub fn set_file(&self, path: String, content: String) {
        let mut inputs = self.inputs.write();
        let mut versions = self.versions.write();
        
        // Check if content actually changed
        let changed = inputs.files.get(&path) != Some(&content);
        
        if changed {
            inputs.files.insert(path.clone(), content);
            let new_version = versions.get(&path).unwrap_or(&0) + 1;
            versions.insert(path.clone(), new_version);
            
            // Invalidate dependent queries
            self.invalidate_dependent_queries(&path);
        }
    }
    
    /// Set a function definition
    pub fn set_function(&self, name: String, body: FunctionBody) {
        let mut inputs = self.inputs.write();
        let mut versions = self.versions.write();
        
        let new_version = versions.get(&name).unwrap_or(&0) + 1;
        versions.insert(name.clone(), new_version);
        
        inputs.functions.insert(name.clone(), FunctionDef {
            body,
            version: new_version,
        });
        
        // Invalidate this specific function's queries
        self.invalidate_query(&QueryKey::InferFunction(name));
    }
    
    /// Get or compute a query
    pub fn query<F>(&self, key: QueryKey, compute: F) -> QueryValue
    where
        F: FnOnce(&InputStorage) -> QueryValue,
    {
        // Check cache first
        {
            let cache = self.cache.read();
            if let Some(value) = cache.get(&key) {
                tracing::debug!("Cache hit for {:?}", key);
                return value.clone();
            }
        }
        
        // Compute
        tracing::debug!("Cache miss for {:?}", key);
        let inputs = self.inputs.read();
        let value = compute(&inputs);
        drop(inputs);
        
        // Store in cache
        let mut cache = self.cache.write();
        cache.insert(key.clone(), value.clone());
        
        // Track dependency
        let mut deps = self.dependencies.write();
        deps.insert(key, vec![]); // Would track actual dependencies
        
        value
    }
    
    /// Parse a file (cached query)
    pub fn parse_file(&self, path: &str) -> ParseResult {
        let key = QueryKey::ParseFile(path.to_string());
        
        match self.query(key, |inputs| {
            let content = inputs.files.get(path)
                .cloned()
                .unwrap_or_default();
            
            // Parse the file
            let result = parse_go(&content);
            QueryValue::ParseResult(result)
        }) {
            QueryValue::ParseResult(r) => r,
            _ => unreachable!(),
        }
    }
    
    /// Infer function type (cached query)
    pub fn infer_function(&self, name: &str) -> FunctionTypeResult {
        let key = QueryKey::InferFunction(name.to_string());
        
        match self.query(key, |inputs| {
            let func_def = inputs.functions.get(name)
                .cloned()
                .expect("Function not found");
            
            let result = infer_function_body(&func_def.body);
            QueryValue::FunctionType(result)
        }) {
            QueryValue::FunctionType(r) => r,
            _ => unreachable!(),
        }
    }
    
    /// Invalidate queries dependent on an input
    fn invalidate_dependent_queries(&self, input: &str) {
        let deps = self.dependencies.read();
        let to_invalidate: Vec<_> = deps
            .iter()
            .filter(|(_, inputs)| inputs.contains(&input.to_string()))
            .map(|(key, _)| key.clone())
            .collect();
        drop(deps);
        
        let mut cache = self.cache.write();
        for key in to_invalidate {
            tracing::debug!("Invalidating query {:?}", key);
            cache.remove(&key);
        }
    }
    
    /// Invalidate a specific query
    fn invalidate_query(&self, key: &QueryKey) {
        let mut cache = self.cache.write();
        cache.remove(key);
    }
    
    /// Get stats
    pub fn stats(&self) -> DbStats {
        DbStats {
            cached_queries: self.cache.read().len(),
            tracked_inputs: self.inputs.read().files.len(),
            tracked_functions: self.inputs.read().functions.len(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DbStats {
    pub cached_queries: usize,
    pub tracked_inputs: usize,
    pub tracked_functions: usize,
}

impl Default for IncrementalDb {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Type System Types
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Unknown,
    Unit,
    Bool,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    String,
    Function(Vec<Type>, Box<Type>),
    Struct(String, Vec<(String, Type)>),
    Interface(String, Vec<String>), // method names
    Pointer(Box<Type>),
    Slice(Box<Type>),
    Array(Box<Type>, usize),
    Map(Box<Type>, Box<Type>),
    Chan(Box<Type>),
    Named(String),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unknown => write!(f, "unknown"),
            Type::Unit => write!(f, "()"),
            Type::Bool => write!(f, "bool"),
            Type::Int => write!(f, "int"),
            Type::Int8 => write!(f, "int8"),
            Type::Int16 => write!(f, "int16"),
            Type::Int32 => write!(f, "int32"),
            Type::Int64 => write!(f, "int64"),
            Type::Uint => write!(f, "uint"),
            Type::Uint8 => write!(f, "uint8"),
            Type::Uint16 => write!(f, "uint16"),
            Type::Uint32 => write!(f, "uint32"),
            Type::Uint64 => write!(f, "uint64"),
            Type::Float32 => write!(f, "float32"),
            Type::Float64 => write!(f, "float64"),
            Type::String => write!(f, "string"),
            Type::Function(params, ret) => {
                let params_str: Vec<_> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "fn({}) {}", params_str.join(", "), ret)
            }
            Type::Struct(name, _) => write!(f, "{}", name),
            Type::Interface(name, _) => write!(f, "{}", name),
            Type::Pointer(t) => write!(f, "*{}", t),
            Type::Slice(t) => write!(f, "[]{}", t),
            Type::Array(t, n) => write!(f, "[{}]{}", n, t),
            Type::Map(k, v) => write!(f, "map[{}]{}", k, v),
            Type::Chan(t) => write!(f, "chan {}", t),
            Type::Named(n) => write!(f, "{}", n),
        }
    }
}

impl Type {
    fn is_numeric(&self) -> bool {
        matches!(self, 
            Type::Int | Type::Int8 | Type::Int16 | Type::Int32 | Type::Int64 |
            Type::Uint | Type::Uint8 | Type::Uint16 | Type::Uint32 | Type::Uint64 |
            Type::Float32 | Type::Float64
        )
    }
}

// ============================================================================
// Parsing (simplified)
// ============================================================================

#[derive(Clone, Debug, Default)]
pub struct ParseResult {
    pub ast: GoAst,
    pub errors: Vec<String>,
    pub imports: Vec<Import>,
}

#[derive(Clone, Debug, Default)]
pub struct GoAst;

impl GoAst {
    pub fn empty() -> Self {
        Self
    }
}

#[derive(Clone, Debug)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
}

fn parse_go(source: &str) -> ParseResult {
    // Simplified parser - in production would use tree-sitter or similar
    let mut imports = Vec::new();
    
    for line in source.lines() {
        let line = line.trim();
        if line.starts_with("import") {
            // Very naive import parsing
            if let Some(quoted) = line.split('"').nth(1) {
                imports.push(Import {
                    path: quoted.to_string(),
                    alias: None,
                });
            }
        }
    }
    
    ParseResult {
        ast: GoAst::empty(),
        errors: vec![],
        imports,
    }
}

// ============================================================================
// Type Inference
// ============================================================================

#[derive(Clone, Debug)]
pub struct FunctionTypeResult {
    pub return_type: Type,
    pub param_types: Vec<(String, Type)>,
    pub local_types: HashMap<String, Type>,
    pub errors: Vec<TypeError>,
}

#[derive(Clone, Debug)]
pub struct TypeError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct FunctionBody {
    pub statements: Vec<Statement>,
    pub return_expr: Option<Expression>,
}

#[derive(Clone, Debug)]
pub enum Statement {
    VarDecl(String, Expression),
    Assign(String, Expression),
    Expr(Expression),
}

#[derive(Clone, Debug)]
pub enum Expression {
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    Identifier(String),
    BinaryOp(BinaryOp, Box<Expression>, Box<Expression>),
    Call(Box<Expression>, Vec<Expression>),
}

#[derive(Clone, Debug)]
pub enum BinaryOp {
    Add, Sub, Mul, Div,
    Eq, Ne, Lt, Gt,
    And, Or,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
            BinaryOp::Eq => write!(f, "=="),
            BinaryOp::Ne => write!(f, "!="),
            BinaryOp::Lt => write!(f, "<"),
            BinaryOp::Gt => write!(f, ">"),
            BinaryOp::And => write!(f, "&&"),
            BinaryOp::Or => write!(f, "||"),
        }
    }
}

fn infer_function_body(body: &FunctionBody) -> FunctionTypeResult {
    let mut checker = FunctionChecker::new();
    
    for stmt in &body.statements {
        checker.check_statement(stmt);
    }
    
    let return_type = body.return_expr
        .as_ref()
        .map(|e| checker.infer_expr(e))
        .unwrap_or(Type::Unit);
    
    FunctionTypeResult {
        return_type,
        param_types: vec![],
        local_types: checker.local_types,
        errors: checker.errors,
    }
}

struct FunctionChecker {
    local_types: HashMap<String, Type>,
    errors: Vec<TypeError>,
}

impl FunctionChecker {
    fn new() -> Self {
        Self {
            local_types: HashMap::new(),
            errors: Vec::new(),
        }
    }
    
    fn check_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::VarDecl(name, init) => {
                let ty = self.infer_expr(init);
                self.local_types.insert(name.clone(), ty);
            }
            Statement::Assign(name, expr) => {
                let expr_ty = self.infer_expr(expr);
                if let Some(var_ty) = self.local_types.get(name) {
                    if var_ty != &expr_ty && !is_assignable(var_ty, &expr_ty) {
                        self.errors.push(TypeError {
                            message: format!("Cannot assign {} to variable of type {}", 
                                expr_ty, var_ty),
                            line: 0,
                            column: 0,
                        });
                    }
                }
            }
            Statement::Expr(expr) => {
                self.infer_expr(expr);
            }
        }
    }
    
    fn infer_expr(&mut self, expr: &Expression) -> Type {
        match expr {
            Expression::IntLiteral(_) => Type::Int,
            Expression::FloatLiteral(_) => Type::Float64,
            Expression::StringLiteral(_) => Type::String,
            Expression::BoolLiteral(_) => Type::Bool,
            Expression::Identifier(name) => {
                self.local_types.get(name).cloned().unwrap_or(Type::Unknown)
            }
            Expression::BinaryOp(op, lhs, rhs) => {
                self.check_binary_op(op, lhs, rhs)
            }
            Expression::Call(func, args) => {
                self.check_call(func, args)
            }
        }
    }
    
    fn check_binary_op(&mut self, op: &BinaryOp, lhs: &Expression, rhs: &Expression) -> Type {
        let lhs_ty = self.infer_expr(lhs);
        let rhs_ty = self.infer_expr(rhs);
        
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                if lhs_ty.is_numeric() && rhs_ty.is_numeric() {
                    // Type promotion
                    if matches!(lhs_ty, Type::Float64 | Type::Float32) || 
                       matches!(rhs_ty, Type::Float64 | Type::Float32) {
                        Type::Float64
                    } else {
                        Type::Int
                    }
                } else {
                    self.errors.push(TypeError {
                        message: format!("Cannot apply {} to {} and {}", op, lhs_ty, rhs_ty),
                        line: 0,
                        column: 0,
                    });
                    Type::Unknown
                }
            }
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt => {
                Type::Bool
            }
            BinaryOp::And | BinaryOp::Or => {
                if lhs_ty == Type::Bool && rhs_ty == Type::Bool {
                    Type::Bool
                } else {
                    self.errors.push(TypeError {
                        message: format!("Logical operators require bool, got {} and {}", 
                            lhs_ty, rhs_ty),
                        line: 0,
                        column: 0,
                    });
                    Type::Unknown
                }
            }
        }
    }
    
    fn check_call(&mut self, func: &Expression, args: &[Expression]) -> Type {
        // Simplified - would look up function signature
        for arg in args {
            self.infer_expr(arg);
        }
        Type::Unknown
    }
}

fn is_assignable(target: &Type, source: &Type) -> bool {
    match (target, source) {
        (Type::Unknown, _) => true,
        (_, Type::Unknown) => true,
        (a, b) if a == b => true,
        (Type::Float64, Type::Int) => true,
        (Type::Float64, Type::Float32) => true,
        _ => false,
    }
}

// ============================================================================
// Import Resolution
// ============================================================================

#[derive(Clone, Debug)]
pub struct ImportResolution {
    pub resolved: Vec<ResolvedImport>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ResolvedImport {
    pub path: String,
    pub package_type: Type,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_inference() {
        let db = IncrementalDb::new();
        
        let body = FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::IntLiteral(42)),
        };
        
        db.set_function("test".to_string(), body);
        let result = db.infer_function("test");
        
        assert_eq!(result.return_type, Type::Int);
        assert!(result.errors.is_empty());
    }
    
    #[test]
    fn test_incremental_update() {
        let db = IncrementalDb::new();
        
        // Set up two functions
        db.set_function("f1".to_string(), FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::IntLiteral(1)),
        });
        db.set_function("f2".to_string(), FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::StringLiteral("hi".to_string())),
        });
        
        // Initial check
        let r1_v1 = db.infer_function("f1");
        let r2_v1 = db.infer_function("f2");
        
        assert_eq!(r1_v1.return_type, Type::Int);
        assert_eq!(r2_v1.return_type, Type::String);
        
        // Check stats - both should be cached
        let stats = db.stats();
        assert_eq!(stats.cached_queries, 2);
        
        // Update f1
        db.set_function("f1".to_string(), FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::FloatLiteral(3.14)),
        });
        
        // f1 should be recomputed
        let r1_v2 = db.infer_function("f1");
        assert_eq!(r1_v2.return_type, Type::Float64);
        
        // f2 should still return cached result
        let r2_v2 = db.infer_function("f2");
        assert_eq!(r2_v2.return_type, Type::String);
    }
    
    #[test]
    fn test_type_error() {
        let db = IncrementalDb::new();
        
        // "hello" + 42 is a type error
        let body = FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::StringLiteral("hello".to_string())),
                Box::new(Expression::IntLiteral(42)),
            )),
        };
        
        db.set_function("bad".to_string(), body);
        let result = db.infer_function("bad");
        
        assert!(!result.errors.is_empty());
    }
}
