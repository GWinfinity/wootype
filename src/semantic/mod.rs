//! 语义操作系统 (Semantic OS)
//!
//! 完整替代 gopls 的类型相关功能，支持复杂语义查询。
//!
//! # 核心模块
//!
//! - `queries`: 语义查询（定义、引用、实现等）
//! - `checks`: 一致性检查（接口实现、类型兼容等）
//! - `operations`: 重构操作（重命名、提取等）
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use wootype::semantic::{SemanticOS, Position};
//!
//! let os = SemanticOS::new(db);
//!
//! // 类型查询
//! let ty = os.type_at(file, position)?;
//!
//! // 跳转到定义
//! let def = os.goto_definition(file, position)?;
//!
//! // 查找实现
//! let impls = os.find_implementations(interface)?;
//! ```

pub mod queries;
pub mod checks;
pub mod operations;

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::salsa_full::{TypeDatabase, SourceFile, Symbol, Type, Location};

// Re-export main types
pub use self::queries::{QueryEngine, SymbolInfo};
pub use self::checks::{CheckEngine, InterfaceCheckResult, CompatibilityResult};
pub use self::operations::{OperationEngine, WorkspaceEdit, TextEdit};

/// 光标位置
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

impl Position {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

/// 代码范围
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn contains(&self, pos: Position) -> bool {
        pos.line >= self.start.line
            && pos.line <= self.end.line
            && (pos.line != self.start.line || pos.character >= self.start.character)
            && (pos.line != self.end.line || pos.character <= self.end.character)
    }
}

/// 文档位置
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DocumentLocation {
    pub path: PathBuf,
    pub range: Range,
}

/// 语义信息
#[derive(Clone, Debug)]
pub struct SemanticInfo {
    pub symbol: Symbol,
    pub ty: Type,
    pub location: DocumentLocation,
    pub documentation: Option<String>,
}

/// 查询结果
#[derive(Clone, Debug)]
pub struct QueryResult<T> {
    pub data: T,
    pub duration_ms: f64,
    pub from_cache: bool,
}

/// 语义操作系统主入口
pub struct SemanticOS {
    db: TypeDatabase,
    query_engine: Mutex<QueryEngine>,
    check_engine: Mutex<CheckEngine>,
}

impl SemanticOS {
    /// 创建新的语义操作系统实例
    pub fn new(db: TypeDatabase) -> Self {
        Self {
            db,
            query_engine: Mutex::new(QueryEngine::new()),
            check_engine: Mutex::new(CheckEngine::new()),
        }
    }

    /// 获取底层数据库
    pub fn db(&self) -> &TypeDatabase {
        &self.db
    }

    /// 获取光标位置的类型信息
    pub fn type_at(&self, file: &Path, position: Position) -> Option<QueryResult<Type>> {
        let start = std::time::Instant::now();
        
        let engine = self.query_engine.lock().ok()?;
        let result = engine.type_at(file, position);
        
        Some(QueryResult {
            data: result?,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            from_cache: false,
        })
    }

    /// 跳转到定义
    pub fn goto_definition(
        &self,
        file: &Path,
        position: Position,
    ) -> Option<QueryResult<DocumentLocation>> {
        let start = std::time::Instant::now();
        
        let engine = self.query_engine.lock().ok()?;
        let result = engine.goto_definition(file, position)?;
        
        Some(QueryResult {
            data: result,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            from_cache: false,
        })
    }

    /// 查找所有引用
    pub fn find_references(
        &self,
        file: &Path,
        position: Position,
        include_declaration: bool,
    ) -> Option<QueryResult<Vec<DocumentLocation>>> {
        let start = std::time::Instant::now();
        
        let engine = self.query_engine.lock().ok()?;
        let result = engine.find_references(file, position, include_declaration);
        
        Some(QueryResult {
            data: result,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            from_cache: false,
        })
    }

    /// 查找接口实现
    pub fn find_implementations(
        &self,
        file: &Path,
        position: Position,
    ) -> Option<QueryResult<Vec<DocumentLocation>>> {
        let start = std::time::Instant::now();
        
        let engine = self.query_engine.lock().ok()?;
        let result = engine.find_implementations(file, position);
        
        Some(QueryResult {
            data: result,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            from_cache: false,
        })
    }

    /// 检查跨模块接口实现一致性
    pub fn check_interface_consistency(
        &self,
        interface_file: &Path,
        interface_name: &str,
    ) -> Option<QueryResult<checks::InterfaceCheckResult>> {
        let start = std::time::Instant::now();
        
        let engine = self.check_engine.lock().ok()?;
        let result = engine.check_interface_implementations(interface_file, interface_name);
        
        Some(QueryResult {
            data: result,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            from_cache: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_contains() {
        let range = Range::new(
            Position::new(1, 5),
            Position::new(3, 10),
        );

        assert!(range.contains(Position::new(2, 0)));
        assert!(range.contains(Position::new(1, 5)));
        assert!(range.contains(Position::new(3, 10)));
        assert!(!range.contains(Position::new(0, 0)));
        assert!(!range.contains(Position::new(4, 0)));
    }

    #[test]
    fn test_semantic_os_creation() {
        let db = TypeDatabase::new();
        let os = SemanticOS::new(db);
        // Just verify it creates without panicking
        let _ = os.db().metrics();
    }
}
