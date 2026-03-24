//! Salsa tracked queries for incremental type checking

use super::*;

// ============================================================================
// Tracked structs for intermediate results
// ============================================================================

/// A parsed file with AST
#[salsa::tracked(debug)]
pub struct ParsedFile<'db> {
    pub source: SourceFile,
    pub statements: Vec<Statement>,
    pub errors: Vec<ParseError>,
    pub imports: Vec<Import>,
    pub functions: Vec<Function<'db>>,
}

/// A function in the AST
#[salsa::tracked(debug)]
pub struct Function<'db> {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Vec<Statement>,
}

/// Symbol index for a file
#[salsa::tracked(debug)]
pub struct SymbolIndex<'db> {
    pub source: SourceFile,
    pub exports: Vec<Symbol>,
    pub all_symbols: Vec<Symbol>,
}

/// Type inference result
#[salsa::tracked(debug)]
pub struct InferredType<'db> {
    pub ty: Type,
    pub errors: Vec<TypeError>,
}

/// Type check result for a file
#[salsa::tracked(debug)]
pub struct TypeCheckResult<'db> {
    pub source: SourceFile,
    pub success: bool,
    pub errors: Vec<TypeError>,
}

/// Completion item
#[salsa::tracked(debug)]
pub struct CompletionItem<'db> {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
}

/// Interface definition
#[salsa::tracked(debug)]
pub struct Interface<'db> {
    pub name: String,
    pub methods: Vec<(String, FunctionSignature)>,
}

// ============================================================================
// Supporting types
// ============================================================================

/// A statement in the AST
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum Statement {
    Let(String, Expr),
    Expr(Expr),
    Return(Option<Expr>),
    Assignment(String, Expr),
}

/// An expression in the AST
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(OrderedFloat),
    String(String),
    Bool(bool),
    Identifier(String),
    Call(Box<Expr>, Vec<Expr>),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    FieldAccess(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Array(Vec<Expr>),
    Lambda(Vec<String>, Box<Expr>),
}

impl std::hash::Hash for Expr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        use Expr::*;
        match self {
            Int(i) => {
                0u8.hash(state);
                i.hash(state);
            }
            Float(f) => {
                1u8.hash(state);
                f.hash(state);
            }
            String(s) => {
                2u8.hash(state);
                s.hash(state);
            }
            Bool(b) => {
                3u8.hash(state);
                b.hash(state);
            }
            Identifier(i) => {
                4u8.hash(state);
                i.hash(state);
            }
            Call(f, args) => {
                5u8.hash(state);
                f.hash(state);
                args.hash(state);
            }
            Binary(l, op, r) => {
                6u8.hash(state);
                l.hash(state);
                op.hash(state);
                r.hash(state);
            }
            FieldAccess(e, field) => {
                7u8.hash(state);
                e.hash(state);
                field.hash(state);
            }
            Index(a, i) => {
                8u8.hash(state);
                a.hash(state);
                i.hash(state);
            }
            Array(a) => {
                9u8.hash(state);
                a.hash(state);
            }
            Lambda(p, b) => {
                10u8.hash(state);
                p.hash(state);
                b.hash(state);
            }
        }
    }
}

impl Eq for Expr {}

/// Wrapper for f64 that implements Hash and Eq
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderedFloat(pub f64);

impl std::hash::Hash for OrderedFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl Eq for OrderedFloat {}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// Parse error
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// Import statement
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
}

/// Function signature
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FunctionSignature {
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
}

/// Completion kind
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Variable,
    Type,
    Field,
    Module,
    Keyword,
}

// ============================================================================
// Tracked functions (queries)
// ============================================================================

/// Parse a source file
#[salsa::tracked]
pub fn parse_file<'db>(db: &'db dyn salsa::Database, source: SourceFile) -> ParsedFile<'db> {
    let content = source.content(db);

    let mut functions = vec![];
    let mut imports = vec![];
    let statements = vec![];
    let errors = vec![];

    // Simple parsing
    for line in content.lines() {
        let line = line.trim();

        // Parse imports
        if line.starts_with("import") {
            if let Some(path) = line.split('"').nth(1) {
                imports.push(Import {
                    path: path.to_string(),
                    alias: None,
                });
            }
        }

        // Parse function declarations
        if let Some(name) = extract_function_name(line) {
            let func = Function::new(db, name, vec![], Type::Unit, vec![]);
            functions.push(func);
        }
    }

    ParsedFile::new(db, source, statements, errors, imports, functions)
}

/// Build symbol index for a file
#[salsa::tracked]
pub fn file_symbols<'db>(db: &'db dyn salsa::Database, source: SourceFile) -> SymbolIndex<'db> {
    let parsed = parse_file(db, source);

    let mut exports = Vec::new();
    let mut all_symbols = Vec::new();

    for func in parsed.functions(db) {
        let name = func.name(db);
        let is_exported = name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);

        let symbol = Symbol {
            name: name.clone(),
            kind: SymbolKind::Function,
            ty: Type::Func(
                func.params(db).iter().map(|(_, t)| t.clone()).collect(),
                Box::new(func.return_type(db).clone()),
            ),
            location: Location {
                file: source.path(db).clone(),
                span: Span::default(),
            },
            is_exported,
            docs: None,
        };

        if is_exported {
            exports.push(symbol.clone());
        }
        all_symbols.push(symbol);
    }

    SymbolIndex::new(db, source, exports, all_symbols)
}

/// Type check a file
#[salsa::tracked]
pub fn type_check_file<'db>(
    db: &'db dyn salsa::Database,
    source: SourceFile,
) -> TypeCheckResult<'db> {
    let parsed = parse_file(db, source);
    let mut errors = Vec::new();

    // Type check each function
    for func in parsed.functions(db) {
        let result = infer_function_type(db, func);
        errors.extend(result.errors(db).iter().cloned());
    }

    TypeCheckResult::new(db, source, errors.is_empty(), errors)
}

/// Infer type of a function
#[salsa::tracked]
pub fn infer_function_type<'db>(
    db: &'db dyn salsa::Database,
    func: Function<'db>,
) -> InferredType<'db> {
    // Simple inference - in real impl would check body
    let return_type = func.return_type(db).clone();
    let errors = Vec::new();

    InferredType::new(db, return_type, errors)
}

/// Get completions at a position
#[salsa::tracked]
pub fn completions_at<'db>(
    db: &'db dyn salsa::Database,
    source: SourceFile,
    _offset: usize,
) -> Vec<CompletionItem<'db>> {
    let symbols = file_symbols(db, source);

    symbols
        .all_symbols(db)
        .iter()
        .map(|s| {
            CompletionItem::new(
                db,
                s.name.clone(),
                match s.kind {
                    SymbolKind::Function => CompletionKind::Function,
                    SymbolKind::Variable => CompletionKind::Variable,
                    SymbolKind::Type | SymbolKind::Interface => CompletionKind::Type,
                    SymbolKind::Constant => CompletionKind::Variable,
                    SymbolKind::Module => CompletionKind::Module,
                },
                Some(format!("{}", s.ty)),
            )
        })
        .collect()
}

/// Check if a type implements an interface
#[salsa::tracked]
pub fn check_implements<'db>(
    _db: &'db dyn salsa::Database,
    _ty: Type,
    interface: Interface<'db>,
) -> bool {
    // Simplified - would check method signatures
    !interface.methods(_db).is_empty()
}

/// Resolve symbol at offset
#[salsa::tracked]
pub fn resolve_symbol_at<'db>(
    db: &'db dyn salsa::Database,
    source: SourceFile,
    _offset: usize,
) -> Option<Symbol> {
    let symbols = file_symbols(db, source);
    symbols.all_symbols(db).first().cloned()
}

// ============================================================================
// Helper functions
// ============================================================================

fn extract_function_name(line: &str) -> Option<String> {
    if !line.starts_with("func ") {
        return None;
    }

    let rest = &line[5..];

    // Handle methods: func (r Receiver) MethodName(...)
    if rest.starts_with('(') {
        rest.split(')')
            .nth(1)
            .and_then(|s| s.trim().split('(').next())
            .map(|s| s.to_string())
    } else {
        rest.split('(').next().map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::salsa_full::database::TypeDatabase;

    #[test]
    fn test_parse_file() {
        let db = TypeDatabase::new();
        let source = SourceFile::new(
            &db,
            std::path::PathBuf::from("test.go"),
            "package main\n\nfunc main() {}\nfunc Helper() int { return 42 }".to_string(),
            1,
        );

        let parsed = parse_file(&db, source);

        assert_eq!(parsed.functions(&db).len(), 2);
        assert_eq!(parsed.imports(&db).len(), 0);
    }

    #[test]
    fn test_file_symbols() {
        let db = TypeDatabase::new();
        let source = SourceFile::new(
            &db,
            std::path::PathBuf::from("test.go"),
            "func Test() {}\nfunc helper() {}".to_string(),
            1,
        );

        let index = file_symbols(&db, source);

        // Test is exported, helper is not
        assert_eq!(index.exports(&db).len(), 1);
        assert_eq!(index.all_symbols(&db).len(), 2);
    }

    #[test]
    fn test_completions() {
        let db = TypeDatabase::new();
        let source = SourceFile::new(
            &db,
            std::path::PathBuf::from("test.go"),
            "func Test() {}".to_string(),
            1,
        );

        let completions = completions_at(&db, source, 0);
        assert!(!completions.is_empty());
    }

    #[test]
    fn test_incremental_update() {
        use salsa::Setter;

        let mut db = TypeDatabase::new();
        let source = SourceFile::new(
            &db,
            std::path::PathBuf::from("test.go"),
            "func main() {}".to_string(),
            1,
        );

        // First parse
        let parsed1 = parse_file(&db, source);
        assert_eq!(parsed1.functions(&db).len(), 1);

        // Modify source
        source
            .set_content(&mut db)
            .to("func main() {}\nfunc foo() {}".to_string());
        source.set_version(&mut db).to(2);

        // Re-parse should pick up changes
        let parsed2 = parse_file(&db, source);
        assert_eq!(parsed2.functions(&db).len(), 2);
    }
}
