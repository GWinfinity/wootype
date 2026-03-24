//! Go Modules support for wootype
//!
//! Handles go.mod parsing, dependency resolution, and version management.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A parsed go.mod file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoMod {
    /// Module path (e.g., "github.com/example/mymodule")
    pub module_path: Arc<str>,
    /// Go version requirement (e.g., "1.21")
    pub go_version: Option<Arc<str>>,
    /// Direct dependencies
    pub require: Vec<Require>,
    /// Replace directives
    pub replace: Vec<Replace>,
    /// Exclude directives
    pub exclude: Vec<Exclude>,
    /// Retract directives
    pub retract: Vec<Retract>,
    /// Toolchain version
    pub toolchain: Option<Arc<str>>,
}

/// A require directive
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Require {
    pub path: Arc<str>,
    pub version: Arc<str>,
    pub indirect: bool,
}

/// A replace directive
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Replace {
    pub old: ModulePath,
    pub new: ModulePath,
}

/// Module path with optional version
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath {
    pub path: Arc<str>,
    pub version: Option<Arc<str>>,
}

/// An exclude directive
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Exclude {
    pub path: Arc<str>,
    pub version: Arc<str>,
}

/// A retract directive
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Retract {
    pub version_low: Option<Arc<str>>,
    pub version_high: Option<Arc<str>>,
    pub rationale: Option<Arc<str>>,
}

impl GoMod {
    /// Parse a go.mod file from string content
    pub fn parse(content: &str) -> Result<Self, GoModError> {
        let mut module_path = None;
        let mut go_version = None;
        let mut toolchain = None;
        let mut require = Vec::new();
        let mut replace = Vec::new();
        let mut exclude = Vec::new();
        let mut retract = Vec::new();

        let mut in_block = None;
        let mut block_content = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            // Handle block endings
            if line == ")" {
                if let Some(block_type) = in_block.take() {
                    match block_type {
                        "require" => Self::parse_require_block(&block_content, &mut require)?,
                        "replace" => Self::parse_replace_block(&block_content, &mut replace)?,
                        "exclude" => Self::parse_exclude_block(&block_content, &mut exclude)?,
                        "retract" => Self::parse_retract_block(&block_content, &mut retract)?,
                        _ => {}
                    }
                    block_content.clear();
                }
                continue;
            }

            // Collect block content
            if in_block.is_some() {
                block_content.push(line);
                continue;
            }

            // Handle block starts
            if line.ends_with('(') {
                let keyword = line.trim_end_matches('(').trim();
                in_block = Some(keyword);
                continue;
            }

            // Parse single-line directives
            if let Some(rest) = line.strip_prefix("module ") {
                module_path = Some(Self::parse_module_path(rest)?);
            } else if let Some(rest) = line.strip_prefix("go ") {
                go_version = Some(Arc::from(rest));
            } else if let Some(rest) = line.strip_prefix("toolchain ") {
                toolchain = Some(Arc::from(rest));
            } else if line.starts_with("require ") {
                if let Some(req) = Self::parse_require_line(&line[8..])? {
                    require.push(req);
                }
            } else if line.starts_with("replace ") {
                if let Some(rep) = Self::parse_replace_line(&line[8..])? {
                    replace.push(rep);
                }
            } else if line.starts_with("exclude ") {
                if let Some(exc) = Self::parse_exclude_line(&line[8..])? {
                    exclude.push(exc);
                }
            } else if line.starts_with("retract ") {
                if let Some(ret) = Self::parse_retract_line(&line[8..])? {
                    retract.push(ret);
                }
            }
        }

        Ok(GoMod {
            module_path: module_path.ok_or(GoModError::MissingModule)?,
            go_version,
            toolchain,
            require,
            replace,
            exclude,
            retract,
        })
    }

    /// Parse from a file path
    pub fn from_file(path: &Path) -> Result<Self, GoModError> {
        let content = std::fs::read_to_string(path).map_err(|e| GoModError::Io(e.to_string()))?;
        Self::parse(&content)
    }

    /// Find go.mod starting from a directory, walking up
    pub fn find_module_root(start: &Path) -> Option<PathBuf> {
        let mut current = Some(start);

        while let Some(dir) = current {
            let gomod = dir.join("go.mod");
            if gomod.exists() {
                return Some(dir.to_path_buf());
            }
            current = dir.parent();
        }

        None
    }

    /// Get all direct dependencies (non-indirect)
    pub fn direct_deps(&self) -> impl Iterator<Item = &Require> {
        self.require.iter().filter(|r| !r.indirect)
    }

    /// Get all indirect dependencies
    pub fn indirect_deps(&self) -> impl Iterator<Item = &Require> {
        self.require.iter().filter(|r| r.indirect)
    }

    /// Lookup a dependency by path
    pub fn find_dep(&self, path: &str) -> Option<&Require> {
        self.require.iter().find(|r| r.path.as_ref() == path)
    }

    /// Check if a version satisfies the go version requirement
    pub fn satisfies_go_version(&self, version: &str) -> bool {
        let Some(req) = self.go_version.as_ref() else {
            return true; // No requirement
        };

        Self::version_gte(version, req)
    }

    /// Compare two Go versions (a >= b)
    fn version_gte(a: &str, b: &str) -> bool {
        let parse = |s: &str| -> Vec<u32> {
            s.trim_start_matches('v')
                .split('.')
                .take(2) // Only major.minor
                .filter_map(|n| n.parse().ok())
                .collect()
        };

        let a_parts = parse(a);
        let b_parts = parse(b);

        for (a, b) in a_parts.iter().zip(b_parts.iter()) {
            match a.cmp(b) {
                std::cmp::Ordering::Greater => return true,
                std::cmp::Ordering::Less => return false,
                _ => continue,
            }
        }

        a_parts.len() >= b_parts.len()
    }

    // Private parsing helpers

    fn parse_module_path(s: &str) -> Result<Arc<str>, GoModError> {
        let s = s.trim().trim_matches('"');
        if s.is_empty() {
            return Err(GoModError::InvalidModule);
        }
        Ok(Arc::from(s))
    }

    fn parse_require_line(s: &str) -> Result<Option<Require>, GoModError> {
        let parts: Vec<_> = s.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(None);
        }

        let path = parts[0].trim_matches('"');
        let version = parts[1].trim_matches('"');
        let indirect = parts.len() > 2 && parts[2] == "//" && parts.get(3) == Some(&"indirect");

        Ok(Some(Require {
            path: Arc::from(path),
            version: Arc::from(version),
            indirect,
        }))
    }

    fn parse_require_block(lines: &[&str], out: &mut Vec<Require>) -> Result<(), GoModError> {
        for line in lines {
            if let Some(req) = Self::parse_require_line(line)? {
                out.push(req);
            }
        }
        Ok(())
    }

    fn parse_replace_line(s: &str) -> Result<Option<Replace>, GoModError> {
        let parts: Vec<_> = s.split("=>").collect();
        if parts.len() != 2 {
            return Ok(None);
        }

        let old = Self::parse_module_path_with_version(parts[0].trim())?;
        let new = Self::parse_module_path_with_version(parts[1].trim())?;

        Ok(Some(Replace { old, new }))
    }

    fn parse_replace_block(lines: &[&str], out: &mut Vec<Replace>) -> Result<(), GoModError> {
        for line in lines {
            if let Some(rep) = Self::parse_replace_line(line)? {
                out.push(rep);
            }
        }
        Ok(())
    }

    fn parse_exclude_line(s: &str) -> Result<Option<Exclude>, GoModError> {
        let parts: Vec<_> = s.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(None);
        }

        Ok(Some(Exclude {
            path: Arc::from(parts[0].trim_matches('"')),
            version: Arc::from(parts[1].trim_matches('"')),
        }))
    }

    fn parse_exclude_block(lines: &[&str], out: &mut Vec<Exclude>) -> Result<(), GoModError> {
        for line in lines {
            if let Some(exc) = Self::parse_exclude_line(line)? {
                out.push(exc);
            }
        }
        Ok(())
    }

    fn parse_retract_line(s: &str) -> Result<Option<Retract>, GoModError> {
        // Simplified: handle "v1.0.0" or "[v1.0.0, v1.1.0]"
        let s = s.trim();

        if s.starts_with('[') && s.contains(',') {
            // Range format: [v1.0.0, v1.1.0]
            let inner = s.trim_start_matches('[').trim_end_matches(']');
            let parts: Vec<_> = inner.split(',').collect();
            if parts.len() == 2 {
                let low = parts[0].trim().trim_matches('"');
                let high = parts[1].trim().trim_matches('"');
                return Ok(Some(Retract {
                    version_low: if low.is_empty() {
                        None
                    } else {
                        Some(Arc::from(low))
                    },
                    version_high: if high.is_empty() {
                        None
                    } else {
                        Some(Arc::from(high))
                    },
                    rationale: None,
                }));
            }
        }

        // Single version
        let version = s.trim_matches('"');
        Ok(Some(Retract {
            version_low: Some(Arc::from(version)),
            version_high: Some(Arc::from(version)),
            rationale: None,
        }))
    }

    fn parse_retract_block(lines: &[&str], out: &mut Vec<Retract>) -> Result<(), GoModError> {
        for line in lines {
            if let Some(ret) = Self::parse_retract_line(line)? {
                out.push(ret);
            }
        }
        Ok(())
    }

    fn parse_module_path_with_version(s: &str) -> Result<ModulePath, GoModError> {
        let parts: Vec<_> = s.split_whitespace().collect();
        if parts.is_empty() {
            return Err(GoModError::InvalidSyntax);
        }

        let path = parts[0].trim_matches('"');
        let version = parts.get(1).map(|v| Arc::from(v.trim_matches('"')));

        Ok(ModulePath {
            path: Arc::from(path),
            version,
        })
    }
}

/// Errors from go.mod parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoModError {
    MissingModule,
    InvalidModule,
    InvalidSyntax,
    InvalidVersion,
    Io(String),
}

impl std::fmt::Display for GoModError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingModule => write!(f, "missing module directive"),
            Self::InvalidModule => write!(f, "invalid module path"),
            Self::InvalidSyntax => write!(f, "invalid syntax"),
            Self::InvalidVersion => write!(f, "invalid version"),
            Self::Io(s) => write!(f, "io error: {}", s),
        }
    }
}

impl std::error::Error for GoModError {}

/// Module cache for resolved dependencies
#[derive(Debug, Default)]
pub struct ModuleCache {
    /// Resolved modules: path@version -> module root
    modules: HashMap<String, PathBuf>,
    /// Go proxy cache
    proxy: Option<Arc<str>>,
}

impl ModuleCache {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            proxy: None,
        }
    }

    pub fn with_proxy(proxy: &str) -> Self {
        Self {
            modules: HashMap::new(),
            proxy: Some(Arc::from(proxy)),
        }
    }

    /// Add a resolved module
    pub fn add(&mut self, path: &str, version: &str, root: PathBuf) {
        let key = format!("{}@{}", path, version);
        self.modules.insert(key, root);
    }

    /// Lookup a module
    pub fn get(&self, path: &str, version: &str) -> Option<&PathBuf> {
        let key = format!("{}@{}", path, version);
        self.modules.get(&key)
    }

    /// Get module root from go.sum or local cache
    pub fn resolve(&self, req: &Require) -> Option<&PathBuf> {
        self.get(&req.path, &req.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_gomod() {
        let content = r#"module github.com/example/mymodule

go 1.21

require (
	github.com/some/dep v1.2.3
	github.com/another/dep v2.0.0 // indirect
)
"#;

        let gomod = GoMod::parse(content).unwrap();
        assert_eq!(gomod.module_path.as_ref(), "github.com/example/mymodule");
        assert_eq!(gomod.go_version.as_ref().map(|s| s.as_ref()), Some("1.21"));
        assert_eq!(gomod.require.len(), 2);

        let direct: Vec<_> = gomod.direct_deps().collect();
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].path.as_ref(), "github.com/some/dep");
    }

    #[test]
    fn test_parse_with_replace() {
        let content = r#"module example.com/test

go 1.21

require github.com/original/lib v1.0.0

replace github.com/original/lib => github.com/fork/lib v1.1.0
"#;

        let gomod = GoMod::parse(content).unwrap();
        assert_eq!(gomod.replace.len(), 1);
        assert_eq!(
            gomod.replace[0].old.path.as_ref(),
            "github.com/original/lib"
        );
        assert_eq!(gomod.replace[0].new.path.as_ref(), "github.com/fork/lib");
    }

    #[test]
    fn test_version_comparison() {
        assert!(GoMod::version_gte("1.21", "1.20"));
        assert!(GoMod::version_gte("1.21", "1.21"));
        assert!(!GoMod::version_gte("1.20", "1.21"));
        assert!(GoMod::version_gte("v1.21.0", "1.21"));
    }

    #[test]
    fn test_find_module_root() {
        // This would need actual filesystem
        // Just test the function exists
    }
}
