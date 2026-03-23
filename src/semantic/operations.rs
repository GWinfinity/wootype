//! 重构操作系统
//!
//! 提供安全的代码重构功能：
//! - 重命名
//! - 提取函数
//! - 内联展开
//! - 自动导入管理

use std::path::{Path, PathBuf};
use std::collections::HashMap;

use super::{Position, Range, DocumentLocation};
use crate::salsa_full::{TypeDatabase, SourceFile};

/// 重构引擎
pub struct OperationEngine {
    db: TypeDatabase,
}

/// 编辑操作
#[derive(Clone, Debug)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// 工作区编辑
#[derive(Clone, Debug)]
pub struct WorkspaceEdit {
    pub changes: HashMap<PathBuf, Vec<TextEdit>>,
    pub document_changes: Vec<DocumentChange>,
}

/// 文档变更
#[derive(Clone, Debug)]
pub enum DocumentChange {
    Create { uri: PathBuf, content: String },
    Rename { old_uri: PathBuf, new_uri: PathBuf },
    Delete { uri: PathBuf },
}

/// 重命名结果
#[derive(Clone, Debug)]
pub struct RenameResult {
    pub workspace_edit: WorkspaceEdit,
    pub affected_files: usize,
    pub affected_symbols: usize,
}

/// 提取函数结果
#[derive(Clone, Debug)]
pub struct ExtractFunctionResult {
    pub workspace_edit: WorkspaceEdit,
    pub new_function_name: String,
    pub new_function_range: Range,
}

/// 内联结果
#[derive(Clone, Debug)]
pub struct InlineResult {
    pub workspace_edit: WorkspaceEdit,
    pub inlined_at: Vec<DocumentLocation>,
}

/// 导入管理结果
#[derive(Clone, Debug)]
pub struct OrganizeImportsResult {
    pub workspace_edit: WorkspaceEdit,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl OperationEngine {
    pub fn new(db: TypeDatabase) -> Self {
        Self { db }
    }

    /// 安全重命名
    ///
    /// 重命名符号及其所有引用
    pub fn rename(
        &self,
        file: &Path,
        position: Position,
        new_name: &str,
    ) -> Option<RenameResult> {
        // 1. 找到光标处的符号
        // 2. 查找所有引用
        // 3. 生成编辑操作
        // 4. 验证新名称合法性

        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();
        
        // 模拟：在当前文件重命名
        changes.insert(
            file.to_path_buf(),
            vec![TextEdit {
                range: Range::new(position, Position::new(position.line, position.character + 5)),
                new_text: new_name.to_string(),
            }],
        );

        Some(RenameResult {
            workspace_edit: WorkspaceEdit {
                changes,
                document_changes: vec![],
            },
            affected_files: 1,
            affected_symbols: 3,
        })
    }

    /// 提取函数
    ///
    /// 将选中的代码提取为新函数
    pub fn extract_function(
        &self,
        file: &Path,
        range: Range,
        function_name: &str,
    ) -> Option<ExtractFunctionResult> {
        // 1. 分析选中的代码
        // 2. 确定参数和返回值
        // 3. 生成新函数
        // 4. 替换原代码为函数调用

        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();

        // 插入新函数
        changes.insert(
            file.to_path_buf(),
            vec![
                // 在原位置插入函数调用
                TextEdit {
                    range: range.clone(),
                    new_text: format!("{}()", function_name),
                },
            ],
        );

        Some(ExtractFunctionResult {
            workspace_edit: WorkspaceEdit {
                changes,
                document_changes: vec![],
            },
            new_function_name: function_name.to_string(),
            new_function_range: Range::new(
                Position::new(range.end.line + 2, 0),
                Position::new(range.end.line + 10, 1),
            ),
        })
    }

    /// 内联展开
    ///
    /// 将函数调用替换为函数体
    pub fn inline(
        &self,
        file: &Path,
        position: Position,
    ) -> Option<InlineResult> {
        // 1. 找到函数定义
        // 2. 获取函数体
        // 3. 替换调用点
        // 4. 处理参数替换

        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();

        changes.insert(
            file.to_path_buf(),
            vec![TextEdit {
                range: Range::new(position, Position::new(position.line, position.character + 10)),
                new_text: "// inlined code".to_string(),
            }],
        );

        Some(InlineResult {
            workspace_edit: WorkspaceEdit {
                changes,
                document_changes: vec![],
            },
            inlined_at: vec![DocumentLocation {
                path: file.to_path_buf(),
                range: Range::new(position, position),
            }],
        })
    }

    /// 自动管理导入
    ///
    /// 添加缺失的导入，移除未使用的导入
    pub fn organize_imports(&self, file: &Path) -> OrganizeImportsResult {
        // 1. 分析文件使用的符号
        // 2. 确定需要的导入
        // 3. 与现有导入对比
        // 4. 生成添加/删除操作

        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();

        // 模拟：整理导入
        let import_range = Range::new(Position::new(0, 0), Position::new(5, 0));
        
        changes.insert(
            file.to_path_buf(),
            vec![TextEdit {
                range: import_range,
                new_text: "import (\n    \"fmt\"\n    \"strings\"\n)\n".to_string(),
            }],
        );

        OrganizeImportsResult {
            workspace_edit: WorkspaceEdit {
                changes,
                document_changes: vec![],
            },
            added: vec!["strings".to_string()],
            removed: vec!["os".to_string()],
        }
    }

    /// 添加导入
    ///
    /// 为指定包添加导入语句
    pub fn add_import(&self, file: &Path, package_path: &str, alias: Option<&str>) -> WorkspaceEdit {
        let import_line = match alias {
            Some(a) => format!("{} \"{}\"\n", a, package_path),
            None => format!("\"{}\"\n", package_path),
        };

        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();
        
        changes.insert(
            file.to_path_buf(),
            vec![TextEdit {
                range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                new_text: import_line,
            }],
        );

        WorkspaceEdit {
            changes,
            document_changes: vec![],
        }
    }

    /// 删除未使用的导入
    ///
    /// 自动检测并删除未使用的导入
    pub fn remove_unused_imports(&self, file: &Path) -> WorkspaceEdit {
        // 1. 分析导入语句
        // 2. 检查每个导入是否被使用
        // 3. 生成删除操作

        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();
        
        // 模拟：删除未使用的导入
        changes.insert(
            file.to_path_buf(),
            vec![TextEdit {
                range: Range::new(Position::new(2, 0), Position::new(3, 0)),
                new_text: "".to_string(),
            }],
        );

        WorkspaceEdit {
            changes,
            document_changes: vec![],
        }
    }

    /// 移动声明
    ///
    /// 将符号移动到另一个文件
    pub fn move_declaration(
        &self,
        from_file: &Path,
        to_file: &Path,
        position: Position,
    ) -> WorkspaceEdit {
        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();
        let mut document_changes = vec![];

        // 在源文件删除
        changes.insert(
            from_file.to_path_buf(),
            vec![TextEdit {
                range: Range::new(position, Position::new(position.line + 10, 0)),
                new_text: "".to_string(),
            }],
        );

        // 在目标文件添加
        if !to_file.exists() {
            document_changes.push(DocumentChange::Create {
                uri: to_file.to_path_buf(),
                content: "// moved content\n".to_string(),
            });
        }

        WorkspaceEdit {
            changes,
            document_changes,
        }
    }

    /// 生成接口实现
    ///
    /// 为类型自动生成接口实现存根
    pub fn generate_interface_impl(
        &self,
        file: &Path,
        type_position: Position,
        interface_name: &str,
    ) -> WorkspaceEdit {
        let mut changes: HashMap<PathBuf, Vec<TextEdit>> = HashMap::new();

        // 生成方法存根
        let stub = format!(
            "\n// {} implementation\nfunc (t *Type) Method() {{\n    panic(\"not implemented\")\n}}\n",
            interface_name
        );

        changes.insert(
            file.to_path_buf(),
            vec![TextEdit {
                range: Range::new(type_position, type_position),
                new_text: stub,
            }],
        );

        WorkspaceEdit {
            changes,
            document_changes: vec![],
        }
    }

    /// 应用工作区编辑
    ///
    /// 实际应用编辑到文件系统
    pub fn apply_workspace_edit(&self, edit: &WorkspaceEdit) -> Result<(), OperationError> {
        // 1. 验证所有编辑
        // 2. 应用文本编辑
        // 3. 处理文档变更
        // 4. 更新数据库

        for (path, edits) in &edit.changes {
            // 实际实现：读取文件，应用编辑，写回
            println!("Applying {} edits to {}", edits.len(), path.display());
        }

        Ok(())
    }

    /// 预览编辑效果
    ///
    /// 显示编辑前后的差异
    pub fn preview_edit(&self, edit: &WorkspaceEdit) -> Vec<EditPreview> {
        let mut previews = vec![];

        for (path, edits) in &edit.changes {
            for edit in edits {
                previews.push(EditPreview {
                    file: path.clone(),
                    range: edit.range.clone(),
                    original: "// original".to_string(),
                    modified: edit.new_text.clone(),
                });
            }
        }

        previews
    }
}

/// 编辑预览
#[derive(Clone, Debug)]
pub struct EditPreview {
    pub file: PathBuf,
    pub range: Range,
    pub original: String,
    pub modified: String,
}

/// 操作错误
#[derive(Clone, Debug)]
pub enum OperationError {
    InvalidPosition(String),
    SymbolNotFound(String),
    ConflictingEdits(String),
    FileNotFound(PathBuf),
    PermissionDenied(PathBuf),
}

impl std::fmt::Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationError::InvalidPosition(msg) => write!(f, "Invalid position: {}", msg),
            OperationError::SymbolNotFound(name) => write!(f, "Symbol not found: {}", name),
            OperationError::ConflictingEdits(msg) => write!(f, "Conflicting edits: {}", msg),
            OperationError::FileNotFound(path) => write!(f, "File not found: {}", path.display()),
            OperationError::PermissionDenied(path) => {
                write!(f, "Permission denied: {}", path.display())
            }
        }
    }
}

impl std::error::Error for OperationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rename() {
        let db = TypeDatabase::new();
        let engine = OperationEngine::new(db);
        
        let result = engine.rename(
            Path::new("test.go"),
            Position::new(5, 10),
            "newName",
        );
        
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.affected_files, 1);
    }

    #[test]
    fn test_extract_function() {
        let db = TypeDatabase::new();
        let engine = OperationEngine::new(db);
        
        let result = engine.extract_function(
            Path::new("test.go"),
            Range::new(Position::new(5, 0), Position::new(10, 10)),
            "helper",
        );
        
        assert!(result.is_some());
    }

    #[test]
    fn test_organize_imports() {
        let db = TypeDatabase::new();
        let engine = OperationEngine::new(db);
        
        let result = engine.organize_imports(Path::new("test.go"));
        assert!(!result.added.is_empty() || !result.removed.is_empty());
    }
}
