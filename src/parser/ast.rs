//! Go AST representation
//!
//! Simplified AST for type extraction.

use crate::core::{SymbolId, TypeId};
use std::sync::Arc;

/// Go source file AST
#[derive(Debug, Clone)]
pub struct GoFile {
    pub package: String,
    pub imports: Vec<ImportSpec>,
    pub decls: Vec<Decl>,
}

/// Import specification
#[derive(Debug, Clone)]
pub struct ImportSpec {
    pub path: String,
    pub alias: Option<String>,
}

/// Declaration
#[derive(Debug, Clone)]
pub enum Decl {
    Type(TypeSpec),
    Func(FuncDecl),
    Var(VarSpec),
    Const(ConstSpec),
}

/// Type declaration
#[derive(Debug, Clone)]
pub struct TypeSpec {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub underlying: TypeExpr,
}

/// Function declaration
#[derive(Debug, Clone)]
pub struct FuncDecl {
    pub name: String,
    pub recv: Option<Field>,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Field>,
    pub results: Vec<Field>,
    pub body: Option<BlockStmt>,
}

/// Variable declaration
#[derive(Debug, Clone)]
pub struct VarSpec {
    pub names: Vec<String>,
    pub typ: Option<TypeExpr>,
    pub values: Vec<Expr>,
}

/// Constant declaration
#[derive(Debug, Clone)]
pub struct ConstSpec {
    pub names: Vec<String>,
    pub typ: Option<TypeExpr>,
    pub values: Vec<Expr>,
}

/// Type parameter
#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    pub constraint: TypeExpr,
}

/// Field (parameter or struct field)
#[derive(Debug, Clone)]
pub struct Field {
    pub names: Vec<String>,
    pub typ: TypeExpr,
    pub tag: Option<String>,
}

/// Type expression
#[derive(Debug, Clone)]
pub enum TypeExpr {
    Ident(String),
    Selector {
        pkg: String,
        name: String,
    },
    Pointer(Box<TypeExpr>),
    Slice(Box<TypeExpr>),
    Array {
        len: Option<Box<Expr>>,
        elem: Box<TypeExpr>,
    },
    Map {
        key: Box<TypeExpr>,
        value: Box<TypeExpr>,
    },
    Chan {
        dir: ChanDir,
        elem: Box<TypeExpr>,
    },
    Func {
        params: Vec<Field>,
        results: Vec<Field>,
    },
    Struct(Vec<Field>),
    Interface(Vec<InterfaceElem>),
    Generic {
        base: Box<TypeExpr>,
        args: Vec<TypeExpr>,
    },
    Tuple(Vec<TypeExpr>),
}

/// Channel direction
#[derive(Debug, Clone, Copy)]
pub enum ChanDir {
    Send,
    Recv,
    Both,
}

/// Interface element
#[derive(Debug, Clone)]
pub enum InterfaceElem {
    Method(MethodSpec),
    Type(TypeElem),
}

/// Method specification
#[derive(Debug, Clone)]
pub struct MethodSpec {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Field>,
    pub results: Vec<Field>,
}

/// Type element (embedded or type list)
#[derive(Debug, Clone)]
pub enum TypeElem {
    Type(TypeExpr),
    Approximation(TypeExpr),
    Union(Vec<TypeExpr>),
}

/// Expression
#[derive(Debug, Clone)]
pub enum Expr {
    Ident(String),
    BasicLit(BasicLit),
    CompositeLit {
        typ: Box<TypeExpr>,
        elems: Vec<KeyValue>,
    },
    Selector {
        x: Box<Expr>,
        sel: String,
    },
    Index {
        x: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        x: Box<Expr>,
        low: Option<Box<Expr>>,
        high: Option<Box<Expr>>,
        max: Option<Box<Expr>>,
    },
    TypeAssert {
        x: Box<Expr>,
        typ: TypeExpr,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        variadic: bool,
    },
    Unary {
        op: UnaryOp,
        x: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        x: Box<Expr>,
        y: Box<Expr>,
    },
    FuncLit {
        typ: TypeExpr,
        body: BlockStmt,
    },
}

/// Basic literal
#[derive(Debug, Clone)]
pub enum BasicLit {
    Int(String),
    Float(String),
    Imag(String),
    Char(String),
    String(String),
}

/// Unary operator
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Add,
    Sub,
    Not,
    Xor,
    Mul,
    And,
    Arrow, // <-
}

/// Binary operator
#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    AndNot,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Land,
    Lor, // && ||
}

/// Key-value pair
#[derive(Debug, Clone)]
pub struct KeyValue {
    pub key: Option<Expr>,
    pub value: Expr,
}

/// Statement block
#[derive(Debug, Clone)]
pub struct BlockStmt {
    pub stmts: Vec<Stmt>,
}

/// Statement
#[derive(Debug, Clone)]
pub enum Stmt {
    Decl(Decl),
    Labeled {
        label: String,
        stmt: Box<Stmt>,
    },
    Expr(Expr),
    Send {
        chan: Expr,
        value: Expr,
    },
    IncDec {
        x: Expr,
        inc: bool,
    },
    Assign {
        lhs: Vec<Expr>,
        rhs: Vec<Expr>,
        op: Option<AssignOp>,
    },
    Go(Expr),
    Defer(Expr),
    Return(Vec<Expr>),
    Branch {
        op: BranchOp,
        label: Option<String>,
    },
    Block(BlockStmt),
    If {
        cond: Expr,
        body: BlockStmt,
        else_: Option<Box<Stmt>>,
    },
    Switch {
        tag: Option<Expr>,
        body: BlockStmt,
    },
    TypeSwitch {
        assign: Box<Stmt>,
        body: BlockStmt,
    },
    Select(Vec<CommClause>),
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        post: Option<Box<Stmt>>,
        body: BlockStmt,
    },
    Range {
        key: Option<Expr>,
        value: Option<Expr>,
        range: Expr,
        body: BlockStmt,
    },
}

/// Assignment operator
#[derive(Debug, Clone, Copy)]
pub enum AssignOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    AndNot,
}

/// Branch operation
#[derive(Debug, Clone, Copy)]
pub enum BranchOp {
    Break,
    Continue,
    Goto,
    Fallthrough,
}

/// Communication clause (for select)
#[derive(Debug, Clone)]
pub struct CommClause {
    pub comm: Option<Stmt>,
    pub body: Vec<Stmt>,
}

/// Go AST
pub struct GoAst {
    files: Vec<GoFile>,
}

impl GoAst {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn add_file(&mut self, file: GoFile) {
        self.files.push(file);
    }

    pub fn files(&self) -> &[GoFile] {
        &self.files
    }

    /// Extract all type declarations
    pub fn extract_types(&self) -> Vec<&TypeSpec> {
        let mut types = Vec::new();
        for file in &self.files {
            for decl in &file.decls {
                if let Decl::Type(spec) = decl {
                    types.push(spec);
                }
            }
        }
        types
    }

    /// Extract all function declarations
    pub fn extract_funcs(&self) -> Vec<&FuncDecl> {
        let mut funcs = Vec::new();
        for file in &self.files {
            for decl in &file.decls {
                if let Decl::Func(func) = decl {
                    funcs.push(func);
                }
            }
        }
        funcs
    }
}

impl Default for GoAst {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_creation() {
        let mut ast = GoAst::new();

        let file = GoFile {
            package: "main".to_string(),
            imports: vec![],
            decls: vec![],
        };

        ast.add_file(file);
        assert_eq!(ast.files().len(), 1);
    }
}
