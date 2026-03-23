//! Full Salsa integration for incremental type checking
//!
//! This module provides complete salsa-rs integration with:
//! - #[salsa::input] for file inputs
//! - #[salsa::tracked] for queries and intermediate results
//! - #[salsa::db] for database
//!
//! Based on https://github.com/salsa-rs/salsa

use std::collections::BTreeMap;
use std::path::PathBuf;

// Re-export all submodules
pub mod database;
pub mod inputs;
pub mod queries;
pub mod diagnostics;
pub mod on_demand;
pub mod gradual;
pub mod metrics;

// Re-export commonly used types from inputs
pub use inputs::{SourceFile, FileDigest, PackageManifest, TextChange};

// Re-export commonly used types from queries
pub use queries::{
    ParsedFile, Function, SymbolIndex, InferredType, TypeCheckResult,
    CompletionItem, Interface, Statement, Expr, BinaryOp, ParseError, Import,
    FunctionSignature, CompletionKind, OrderedFloat,
    parse_file, file_symbols, type_check_file, infer_function_type,
    completions_at, check_implements, resolve_symbol_at,
};

// Re-export from database
pub use database::TypeDatabase;

// Re-export other modules
pub use diagnostics::{RichDiagnostic, FileCache, type_error_to_diagnostic, render_diagnostic, Severity, Color};
pub use on_demand::{WorkspaceIndex, PackageLoader, IndexStats};
pub use gradual::{GradualChecker, GradualMode, PythonInterop, RuntimeTag};
pub use metrics::{MetricsCollector, MetricsSnapshot, PerformanceBudget, Timer};

/// Common types used throughout the module
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Float,
    String,
    Bool,
    Any,
    Unit,
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Func(Vec<Type>, Box<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Named(String),
    Struct(BTreeMap<String, Type>),
    Tuple(Vec<Type>),
    Tensor,
    Unknown,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::String => write!(f, "string"),
            Type::Bool => write!(f, "bool"),
            Type::Any => write!(f, "any"),
            Type::Unit => write!(f, "()"),
            Type::Array(t) => write!(f, "[]{}", t),
            Type::Map(k, v) => write!(f, "map[{}, {}]", k, v),
            Type::Func(args, ret) => {
                write!(f, "fn(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", arg)?;
                }
                write!(f, ") -> {}", ret)
            }
            Type::Option(t) => write!(f, "?{}", t),
            Type::Result(ok, err) => write!(f, "result[{}, {}]", ok, err),
            Type::Named(n) => write!(f, "{}", n),
            Type::Struct(_) => write!(f, "struct"),
            Type::Tuple(elems) => {
                write!(f, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", e)?;
                }
                write!(f, ")")
            }
            Type::Tensor => write!(f, "Tensor"),
            Type::Unknown => write!(f, "unknown"),
        }
    }
}

/// Source location
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    pub file: PathBuf,
    pub span: Span,
}

/// Source span
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

/// Symbol information
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub ty: Type,
    pub location: Location,
    pub is_exported: bool,
    pub docs: Option<String>,
}

/// Kind of symbol
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Variable,
    Type,
    Interface,
    Constant,
    Module,
}

/// Type error
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
    pub error_type: ErrorType,
}

/// Specific error type
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum ErrorType {
    TypeMismatch { expected: Type, found: Type },
    UnknownIdentifier(String),
    UnknownField { ty: Type, field: String },
    WrongArity { expected: usize, found: usize },
    NotCallable(Type),
    InvalidOperation { op: String, ty: Type },
    MissingReturn,
    UnreachableCode,
    Generic(String),
}

/// Convenience function to create a new database
pub fn create_database() -> TypeDatabase {
    TypeDatabase::new()
}

/// Convenience function to create a configured gradual checker
pub fn create_gradual_checker(mode: GradualMode) -> GradualChecker {
    GradualChecker::new(mode)
}

/// Convenience function to create metrics collector
pub fn create_metrics() -> MetricsCollector {
    MetricsCollector::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_type_display() {
        assert_eq!(Type::Int.to_string(), "int");
        assert_eq!(Type::Array(Box::new(Type::Int)).to_string(), "[]int");
        assert_eq!(Type::Func(vec![Type::Int, Type::Int], Box::new(Type::Bool)).to_string(), "fn(int, int) -> bool");
    }
    
    #[test]
    fn test_create_database() {
        let db = create_database();
        let _ = db.metrics();
    }
    
    #[test]
    fn test_end_to_end() {
        use salsa::Setter;
        
        let mut db = create_database();
        
        // Create a source file
        let source = SourceFile::new(
            &db,
            PathBuf::from("test.go"),
            "func main() {}\nfunc Add(a int, b int) int { return a + b }".to_string(),
            1,
        );
        
        // Parse it
        let parsed = parse_file(&db, source);
        assert_eq!(parsed.functions(&db).len(), 2);
        
        // Get symbols
        let symbols = file_symbols(&db, source);
        assert_eq!(symbols.exports(&db).len(), 1); // Only Add is exported
        
        // Type check
        let result = type_check_file(&db, source);
        assert!(result.success(&db));
        
        // Modify and verify incremental behavior
        source.set_content(&mut db).to("func main() {}".to_string());
        source.set_version(&mut db).to(2);
        
        let parsed2 = parse_file(&db, source);
        assert_eq!(parsed2.functions(&db).len(), 1);
    }
}
