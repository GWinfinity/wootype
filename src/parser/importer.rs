//! Package importer for Go packages
//!
//! Imports types from Go packages into the type universe.

use super::ast::GoFile;
use super::converter::TypeConverter;
use crate::core::{PackageInfo, SharedUniverse, SymbolId, TypeId, TypeUniverse};

use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Package import result
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub package_path: String,
    pub types_imported: usize,
    pub functions_imported: usize,
    pub variables_imported: usize,
    pub errors: Vec<ImportError>,
}

/// Import error
#[derive(Debug, Clone)]
pub struct ImportError {
    pub kind: ImportErrorKind,
    pub message: String,
    pub file: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum ImportErrorKind {
    NotFound,
    ParseError,
    ConversionError,
    CircularImport,
}

/// Package importer
pub struct PackageImporter {
    universe: SharedUniverse,
    converter: TypeConverter,
    /// Cache of imported packages
    import_cache: DashMap<String, Arc<ImportedPackage>>,
    /// GOPATH
    gopath: PathBuf,
    /// Module cache
    module_cache: PathBuf,
}

/// Imported package data
#[derive(Debug, Clone)]
pub struct ImportedPackage {
    pub info: PackageInfo,
    pub files: Vec<GoFile>,
}

impl PackageImporter {
    pub fn new(universe: SharedUniverse) -> Self {
        Self {
            converter: TypeConverter::new(universe.clone()),
            universe,
            import_cache: DashMap::new(),
            gopath: Self::detect_gopath(),
            module_cache: Self::detect_module_cache(),
        }
    }

    /// Detect GOPATH from environment
    fn detect_gopath() -> PathBuf {
        std::env::var("GOPATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = dirs::home_dir().unwrap_or_default();
                home.join("go")
            })
    }

    /// Detect module cache
    fn detect_module_cache() -> PathBuf {
        std::env::var("GOMODCACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| Self::detect_gopath().join("pkg/mod"))
    }

    /// Import a package by path
    pub async fn import(&self, package_path: &str) -> Result<ImportResult, ImportError> {
        // Check cache
        if let Some(cached) = self.import_cache.get(package_path) {
            return Ok(ImportResult {
                package_path: package_path.to_string(),
                types_imported: cached.info.exports.len(),
                functions_imported: 0,
                variables_imported: 0,
                errors: vec![],
            });
        }

        info!("Importing package: {}", package_path);

        // Find package location
        let pkg_dir = self.find_package_dir(package_path).await?;

        // Parse package files
        let files = self.parse_package(&pkg_dir).await?;

        // Convert types
        let mut types_imported = 0;
        let mut functions_imported = 0;
        let mut errors = Vec::new();

        for file in &files {
            for decl in &file.decls {
                match self.converter.convert_decl(decl).await {
                    Ok(Some(_type_id)) => {
                        types_imported += 1;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        errors.push(ImportError {
                            kind: ImportErrorKind::ConversionError,
                            message: e.to_string(),
                            file: None,
                        });
                    }
                }
            }
        }

        // Register package
        let exports: Vec<SymbolId> = files
            .iter()
            .flat_map(|f| f.decls.iter())
            .filter_map(|d| self.extract_exported_symbol(d))
            .collect();

        let info = PackageInfo {
            path: package_path.into(),
            name: files
                .first()
                .map(|f| f.package.clone().into())
                .unwrap_or_else(|| package_path.into()),
            exports,
            imports: files
                .iter()
                .flat_map(|f| f.imports.iter())
                .map(|i| i.path.clone().into())
                .collect(),
        };

        self.universe.register_package(info.clone());

        // Cache result
        let imported = ImportedPackage { info, files };
        self.import_cache
            .insert(package_path.to_string(), Arc::new(imported));

        info!("Imported {} types from {}", types_imported, package_path);

        Ok(ImportResult {
            package_path: package_path.to_string(),
            types_imported,
            functions_imported,
            variables_imported: 0,
            errors,
        })
    }

    /// Find package directory
    async fn find_package_dir(&self, package_path: &str) -> Result<PathBuf, ImportError> {
        // Try module cache first
        let module_path = self.module_cache.join(package_path);
        if module_path.exists() {
            return Ok(module_path);
        }

        // Try GOPATH
        let gopath_path = self.gopath.join("src").join(package_path);
        if gopath_path.exists() {
            return Ok(gopath_path);
        }

        // Try stdlib
        let goroot = std::env::var("GOROOT").unwrap_or_default();
        if !goroot.is_empty() {
            let stdlib_path = PathBuf::from(goroot).join("src").join(package_path);
            if stdlib_path.exists() {
                return Ok(stdlib_path);
            }
        }

        Err(ImportError {
            kind: ImportErrorKind::NotFound,
            message: format!("Package not found: {}", package_path),
            file: None,
        })
    }

    /// Parse all Go files in package directory
    async fn parse_package(&self, dir: &Path) -> Result<Vec<GoFile>, ImportError> {
        let mut files = Vec::new();

        let mut entries = tokio::fs::read_dir(dir).await.map_err(|e| ImportError {
            kind: ImportErrorKind::NotFound,
            message: e.to_string(),
            file: Some(dir.to_string_lossy().to_string()),
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| ImportError {
            kind: ImportErrorKind::ParseError,
            message: e.to_string(),
            file: None,
        })? {
            let path = entry.path();
            if path.extension() == Some(std::ffi::OsStr::new("go")) {
                // Skip test files in initial import
                if let Some(stem) = path.file_stem() {
                    let stem = stem.to_string_lossy();
                    if stem.ends_with("_test") {
                        continue;
                    }
                }

                match self.parse_file(&path).await {
                    Ok(file) => files.push(file),
                    Err(e) => {
                        warn!("Failed to parse {:?}: {}", path, e.message);
                    }
                }
            }
        }

        if files.is_empty() {
            return Err(ImportError {
                kind: ImportErrorKind::NotFound,
                message: "No Go files found in package".to_string(),
                file: Some(dir.to_string_lossy().to_string()),
            });
        }

        Ok(files)
    }

    /// Parse a single Go file
    async fn parse_file(&self, path: &Path) -> Result<GoFile, ImportError> {
        let source = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ImportError {
                kind: ImportErrorKind::ParseError,
                message: e.to_string(),
                file: Some(path.to_string_lossy().to_string()),
            })?;

        // Parse source
        self.parse_source(&source, path)
    }

    /// Parse Go source code
    fn parse_source(&self, source: &str, path: &Path) -> Result<GoFile, ImportError> {
        // Simplified parsing - would use actual Go parser
        // For now, return placeholder

        Ok(GoFile {
            package: "main".to_string(),
            imports: vec![],
            decls: vec![],
        })
    }

    /// Extract exported symbol from declaration
    fn extract_exported_symbol(&self, decl: &super::ast::Decl) -> Option<SymbolId> {
        use super::ast::Decl;

        match decl {
            Decl::Type(spec) if Self::is_exported(&spec.name) => {
                Some(self.universe.symbols().intern(&spec.name))
            }
            Decl::Func(func) if Self::is_exported(&func.name) => {
                Some(self.universe.symbols().intern(&func.name))
            }
            _ => None,
        }
    }

    /// Check if identifier is exported (starts with uppercase)
    fn is_exported(name: &str) -> bool {
        name.chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
    }

    /// Get cached package
    pub fn get_cached(&self, package_path: &str) -> Option<Arc<ImportedPackage>> {
        self.import_cache.get(package_path).map(|p| p.clone())
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        self.import_cache.clear();
    }

    /// Preload common packages
    pub async fn preload_stdlib(&self) -> Vec<ImportResult> {
        let stdlib_packages = vec![
            "fmt", "os", "io", "strings", "bytes", "time", "sync", "context", "errors", "sort",
        ];

        let mut results = Vec::new();
        for pkg in stdlib_packages {
            match self.import(pkg).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Failed to preload {}: {:?}", pkg, e);
                }
            }
        }

        results
    }
}

/// Import error type
#[derive(Debug)]
pub enum ImporterError {
    Import(ImportError),
    Io(std::io::Error),
}

impl std::fmt::Display for ImporterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Import(e) => write!(f, "Import error: {:?}", e),
            Self::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for ImporterError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    #[tokio::test]
    async fn test_importer_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let importer = PackageImporter::new(universe);

        // Verify GOPATH detection
        assert!(!importer.gopath.as_os_str().is_empty());
    }

    #[test]
    fn test_is_exported() {
        assert!(PackageImporter::is_exported("Exported"));
        assert!(!PackageImporter::is_exported("unexported"));
        assert!(!PackageImporter::is_exported("_private"));
    }
}
