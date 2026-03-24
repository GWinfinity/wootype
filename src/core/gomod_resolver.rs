//! Go Modules 完整解析与依赖管理

use super::gomod::{GoMod, GoModError, Replace};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 模块版本信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleVersion {
    pub path: Arc<str>,
    pub version: Arc<str>,
    pub is_indirect: bool,
    pub is_replaced: bool,
}

/// 模块来源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSource {
    Local(PathBuf),
    Proxy(Arc<str>),
    Replaced(PathBuf),
    Stdlib,
}

/// Go 工作区配置
#[derive(Debug, Clone, Default)]
pub struct GoWork {
    pub go_version: Option<Arc<str>>,
    pub use_dirs: Vec<WorkUse>,
    pub replace: Vec<Replace>,
}

#[derive(Debug, Clone)]
pub struct WorkUse {
    pub path: PathBuf,
    pub module_path: Option<Arc<str>>,
}

/// 完整模块解析器
pub struct ModuleResolver {
    modules: DashMap<String, ModuleNode>,
    root_gomod: RwLock<Option<GoMod>>,
    gowork: RwLock<Option<GoWork>>,
    module_root: RwLock<Option<PathBuf>>,
}

#[derive(Debug, Clone)]
pub struct ModuleNode {
    pub info: ModuleVersion,
    pub source: ModuleSource,
}

impl Default for ModuleResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleResolver {
    pub fn new() -> Self {
        Self {
            modules: DashMap::new(),
            root_gomod: RwLock::new(None),
            gowork: RwLock::new(None),
            module_root: RwLock::new(None),
        }
    }

    pub fn from_directory(path: &Path) -> Result<Self, GoModError> {
        let resolver = Self::new();

        if let Some(gomod_path) = GoMod::find_module_root(path) {
            let gomod = GoMod::from_file(&gomod_path.join("go.mod"))?;
            *resolver.root_gomod.write() = Some(gomod);
            *resolver.module_root.write() = Some(gomod_path);
        }

        Ok(resolver)
    }

    pub fn module_path(&self) -> Option<Arc<str>> {
        self.root_gomod
            .read()
            .as_ref()
            .map(|g| g.module_path.clone())
    }

    pub fn resolve_import(&self, import_path: &str) -> Option<ModuleSource> {
        // Check if it's a standard library package
        if is_stdlib(import_path) {
            return Some(ModuleSource::Stdlib);
        }

        // Check replace directives first
        if let Some(ref gomod) = *self.root_gomod.read() {
            for replace in &gomod.replace {
                if replace.old.path.as_ref() == import_path {
                    return Some(ModuleSource::Replaced(PathBuf::from(
                        replace.new.path.as_ref(),
                    )));
                }
            }
        }

        None
    }
}

fn is_stdlib(path: &str) -> bool {
    let std_prefixes = [
        "builtin", "bytes", "context", "crypto", "database", "debug", "encoding", "errors", "flag",
        "fmt", "go", "hash", "html", "image", "io", "log", "math", "mime", "net", "os", "path",
        "plugin", "reflect", "regexp", "runtime", "sort", "strconv", "strings", "sync", "syscall",
        "testing", "text", "time", "unicode", "unsafe",
    ];

    std_prefixes
        .iter()
        .any(|&prefix| path == prefix || path.starts_with(&format!("{}/", prefix)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdlib_detection() {
        assert!(is_stdlib("fmt"));
        assert!(is_stdlib("net/http"));
        assert!(!is_stdlib("github.com/example/foo"));
    }
}
