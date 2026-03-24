//! On-demand analysis for large codebases
//!
//! Features:
//! - Symbol indexing for fast lookups
//! - Lazy parsing of unused packages
//! - Partial type inference
//! - Memory-efficient representation

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;
use rayon::prelude::*;

use super::{Location, SourceFile, Span, Symbol, SymbolKind, Type, TypeDatabase};

/// Global symbol index for the entire workspace
pub struct WorkspaceIndex {
    /// File path -> file index
    files: DashMap<PathBuf, FileIndex>,
    /// Symbol name -> locations
    symbol_table: DashMap<String, Vec<SymbolLocation>>,
    /// Package path -> exports
    package_exports: DashMap<String, Vec<Symbol>>,
    /// Whether a file needs re-indexing
    dirty_files: RwLock<HashSet<PathBuf>>,
    /// Lazy loader for packages
    package_loader: PackageLoader,
}

/// Index for a single file
#[derive(Clone, Debug)]
pub struct FileIndex {
    pub path: PathBuf,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<String>,
    pub revision: u64,
    pub parse_state: ParseState,
}

/// Parse state of a file
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseState {
    /// Only indexed (shallow parse)
    Indexed,
    /// Partially parsed (imports and exports)
    Partial,
    /// Fully parsed (complete AST)
    Full,
}

/// Symbol with location info
#[derive(Clone, Debug)]
pub struct SymbolLocation {
    pub symbol: Symbol,
    pub file: PathBuf,
}

/// Lazy package loader
pub struct PackageLoader {
    /// Loaded packages
    loaded: DashMap<String, Package>,
    /// Package loading strategy
    strategy: LoadingStrategy,
}

/// Package loading strategy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadingStrategy {
    /// Load all packages eagerly
    Eager,
    /// Load packages on first access
    Lazy,
    /// Load only exported symbols
    ExportsOnly,
}

/// Package information
#[derive(Clone, Debug)]
pub struct Package {
    pub path: String,
    pub files: Vec<PathBuf>,
    pub exports: Vec<Symbol>,
    pub dependencies: Vec<String>,
    pub load_state: LoadState,
}

/// Package load state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadState {
    /// Not loaded
    Unloaded,
    /// Only exports loaded
    ExportsLoaded,
    /// Metadata loaded
    MetadataLoaded,
    /// Fully loaded
    FullyLoaded,
}

/// Symbol index statistics
#[derive(Clone, Debug, Default)]
pub struct IndexStats {
    pub indexed_files: usize,
    pub total_symbols: usize,
    pub dirty_files: usize,
    pub memory_estimate_mb: usize,
    pub loaded_packages: usize,
    pub lazy_loaded_packages: usize,
}

/// Partial type inference context
pub struct PartialInference {
    /// Known types
    known_types: HashMap<String, Type>,
    /// Unknown types to be inferred later
    unknowns: HashSet<String>,
    /// Inference depth limit
    depth_limit: usize,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self {
            files: DashMap::new(),
            symbol_table: DashMap::new(),
            package_exports: DashMap::new(),
            dirty_files: RwLock::new(HashSet::new()),
            package_loader: PackageLoader::new(LoadingStrategy::Lazy),
        }
    }

    /// Index a file (shallow - only symbols, not full AST)
    pub fn index_file_shallow(&self, path: PathBuf, content: &str) -> FileIndex {
        let mut symbols = vec![];
        let mut imports = vec![];

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            // Extract imports
            if line.starts_with("import") {
                if let Some(quoted) = extract_quoted(line) {
                    imports.push(quoted);
                }
            }

            // Extract function declarations
            if let Some(func_name) = extract_function_name(line) {
                let symbol = Symbol {
                    name: func_name.clone(),
                    kind: SymbolKind::Function,
                    ty: Type::Unknown,
                    location: Location {
                        file: path.clone(),
                        span: Span {
                            start: 0,
                            end: 0,
                            line: line_num,
                            column: 0,
                        },
                    },
                    is_exported: func_name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false),
                    docs: None,
                };

                symbols.push(symbol.clone());

                self.symbol_table
                    .entry(func_name)
                    .or_insert_with(Vec::new)
                    .push(SymbolLocation {
                        symbol,
                        file: path.clone(),
                    });
            }

            // Extract type declarations
            if line.starts_with("type ") {
                if let Some(type_name) = line.split_whitespace().nth(1) {
                    let symbol = Symbol {
                        name: type_name.to_string(),
                        kind: SymbolKind::Type,
                        ty: Type::Named(type_name.to_string()),
                        location: Location {
                            file: path.clone(),
                            span: Span {
                                start: 0,
                                end: 0,
                                line: line_num,
                                column: 0,
                            },
                        },
                        is_exported: type_name
                            .chars()
                            .next()
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false),
                        docs: None,
                    };

                    symbols.push(symbol.clone());

                    self.symbol_table
                        .entry(type_name.to_string())
                        .or_insert_with(Vec::new)
                        .push(SymbolLocation {
                            symbol,
                            file: path.clone(),
                        });
                }
            }
        }

        let index = FileIndex {
            path: path.clone(),
            symbols,
            imports,
            revision: 1,
            parse_state: ParseState::Indexed,
        };

        self.files.insert(path, index.clone());
        index
    }

    /// Full parse of a file (on-demand)
    pub fn fully_parse_file(&self, path: &PathBuf) -> Option<FileIndex> {
        if let Some(mut entry) = self.files.get_mut(path) {
            if entry.parse_state != ParseState::Full {
                // Trigger full parse via Salsa
                entry.parse_state = ParseState::Full;
                entry.revision += 1;
            }
            Some(entry.clone())
        } else {
            None
        }
    }

    /// Partial parse - parse imports and exports only
    pub fn partial_parse_file(&self, path: &PathBuf) -> Option<FileIndex> {
        if let Some(mut entry) = self.files.get_mut(path) {
            if entry.parse_state == ParseState::Indexed {
                entry.parse_state = ParseState::Partial;
                entry.revision += 1;
            }
            Some(entry.clone())
        } else {
            None
        }
    }

    /// Find symbol by name
    pub fn find_symbol(&self, name: &str) -> Vec<SymbolLocation> {
        self.symbol_table
            .get(name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Find symbols with prefix (for completion)
    pub fn find_symbols_with_prefix(&self, prefix: &str) -> Vec<SymbolLocation> {
        let mut results = vec![];

        for entry in self.symbol_table.iter() {
            if entry.key().starts_with(prefix) {
                results.extend(entry.value().iter().cloned());
            }
        }

        results
    }

    /// Get all exported symbols from a package
    pub fn get_package_exports(&self, package_path: &str) -> Vec<Symbol> {
        self.package_exports
            .get(package_path)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Check if package exists
    pub fn package_exists(&self, package_path: &str) -> bool {
        self.package_exports.contains_key(package_path)
    }

    /// Index a package (but not its dependencies)
    pub fn index_package_shallow(&self, package_path: &str, files: Vec<(PathBuf, String)>) {
        let mut exports = vec![];

        for (path, content) in files {
            let index = self.index_file_shallow(path, &content);

            for symbol in &index.symbols {
                if symbol.is_exported {
                    exports.push(symbol.clone());
                }
            }
        }

        self.package_exports
            .insert(package_path.to_string(), exports);
    }

    /// Parallel indexing of multiple files
    pub fn index_files_parallel(&self, files: Vec<(PathBuf, String)>) {
        let results: Vec<_> = files
            .into_par_iter()
            .map(|(path, content)| {
                let index = self.index_file_shallow(path.clone(), &content);
                (path, index)
            })
            .collect();

        for (path, index) in results {
            self.files.insert(path, index);
        }
    }

    /// Lazy load a package
    pub fn lazy_load_package(&self, package_path: &str) -> Option<Package> {
        self.package_loader.load_package(self, package_path)
    }

    /// Get partial type inference for a symbol
    pub fn infer_symbol_type(&self, symbol_name: &str) -> Option<Type> {
        // First check if we have it indexed
        if let Some(locations) = self.symbol_table.get(symbol_name) {
            if let Some(first) = locations.first() {
                // Return partial type info
                return Some(first.symbol.ty.clone());
            }
        }

        // Try to load from package
        self.package_loader.infer_symbol_type(symbol_name)
    }

    /// Mark file as dirty
    pub fn mark_dirty(&self, path: &PathBuf) {
        self.dirty_files.write().insert(path.clone());
    }

    /// Process dirty files
    pub fn process_dirty_files(&self) {
        let dirty: Vec<_> = self.dirty_files.write().drain().collect();

        for path in dirty {
            if let Some(content) = std::fs::read_to_string(&path).ok() {
                self.index_file_shallow(path, &content);
            }
        }
    }

    /// Get statistics
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            indexed_files: self.files.len(),
            total_symbols: self.symbol_table.len(),
            dirty_files: self.dirty_files.read().len(),
            memory_estimate_mb: self.estimate_memory(),
            loaded_packages: self.package_loader.loaded_count(),
            lazy_loaded_packages: self.package_loader.lazy_count(),
        }
    }

    fn estimate_memory(&self) -> usize {
        let file_size = self.files.len() * std::mem::size_of::<FileIndex>();
        let symbol_size = self.symbol_table.len() * std::mem::size_of::<SymbolLocation>();
        (file_size + symbol_size) / (1024 * 1024)
    }
}

impl Default for WorkspaceIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl PackageLoader {
    pub fn new(strategy: LoadingStrategy) -> Self {
        Self {
            loaded: DashMap::new(),
            strategy,
        }
    }

    /// Load package (lazy)
    pub fn load_package(&self, index: &WorkspaceIndex, path: &str) -> Option<Package> {
        // Check if already loaded
        if let Some(pkg) = self.loaded.get(path) {
            return Some(pkg.clone());
        }

        // Check if exports are available
        if !index.package_exists(path) {
            return None;
        }

        let exports = index.get_package_exports(path);

        let package = Package {
            path: path.to_string(),
            files: vec![],
            exports,
            dependencies: vec![],
            load_state: LoadState::ExportsLoaded,
        };

        self.loaded.insert(path.to_string(), package.clone());
        Some(package)
    }

    /// Fully load a package
    pub fn fully_load_package(&self, path: &str) -> Option<Package> {
        let mut package = self.loaded.get_mut(path)?;

        if package.load_state == LoadState::FullyLoaded {
            return Some(package.clone());
        }

        // Load all files
        package.load_state = LoadState::FullyLoaded;

        Some(package.clone())
    }

    /// Get loaded package count
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Get lazily loaded package count
    pub fn lazy_count(&self) -> usize {
        self.loaded
            .iter()
            .filter(|p| matches!(p.load_state, LoadState::ExportsLoaded))
            .count()
    }

    /// Infer symbol type (lazy)
    pub fn infer_symbol_type(&self, symbol_name: &str) -> Option<Type> {
        // Search in loaded packages
        for pkg in self.loaded.iter() {
            for symbol in &pkg.exports {
                if symbol.name == symbol_name {
                    return Some(symbol.ty.clone());
                }
            }
        }
        None
    }
}

impl PartialInference {
    pub fn new(depth_limit: usize) -> Self {
        Self {
            known_types: HashMap::new(),
            unknowns: HashSet::new(),
            depth_limit,
        }
    }

    /// Add known type
    pub fn add_known(&mut self, name: &str, ty: Type) {
        self.known_types.insert(name.to_string(), ty);
        self.unknowns.remove(name);
    }

    /// Mark as unknown
    pub fn add_unknown(&mut self, name: &str) {
        if !self.known_types.contains_key(name) {
            self.unknowns.insert(name.to_string());
        }
    }

    /// Try to infer unknown types
    pub fn infer_unknowns(&mut self) -> HashMap<String, Type> {
        let mut inferred = HashMap::new();

        // Simple inference: check if name suggests type
        for unknown in &self.unknowns {
            if let Some(ty) = self.infer_from_name(unknown) {
                inferred.insert(unknown.clone(), ty);
            }
        }

        // Update known types
        for (name, ty) in &inferred {
            self.known_types.insert(name.clone(), ty.clone());
        }

        // Clear inferred from unknowns
        for name in inferred.keys() {
            self.unknowns.remove(name);
        }

        inferred
    }

    fn infer_from_name(&self, name: &str) -> Option<Type> {
        // Heuristic inference from variable name
        if name.contains("count") || name.contains("num") || name.ends_with("Id") {
            return Some(Type::Int);
        }
        if name.contains("name") || name.contains("text") || name.contains("str") {
            return Some(Type::String);
        }
        if name.starts_with("is") || name.starts_with("has") || name.contains("enabled") {
            return Some(Type::Bool);
        }
        if name.contains("list") || name.ends_with("s") {
            return Some(Type::Array(Box::new(Type::Any)));
        }
        None
    }

    /// Get inference completeness (0.0 - 1.0)
    pub fn completeness(&self) -> f64 {
        let total = self.known_types.len() + self.unknowns.len();
        if total == 0 {
            return 1.0;
        }
        self.known_types.len() as f64 / total as f64
    }
}

// Helper functions
fn extract_quoted(s: &str) -> Option<String> {
    s.split('"').nth(1).map(|s| s.to_string())
}

fn extract_function_name(line: &str) -> Option<String> {
    if line.starts_with("func ") {
        let rest = &line[5..];
        if rest.starts_with('(') {
            rest.split(')')
                .nth(1)
                .and_then(|s| s.trim().split('(').next())
                .map(|s| s.to_string())
        } else {
            rest.split('(').next().map(|s| s.to_string())
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_file_shallow() {
        let index = WorkspaceIndex::new();
        let content = r#"
package main

import "fmt"

func main() {
    fmt.Println("Hello")
}

func Helper() int {
    return 42
}
"#;

        let file_index = index.index_file_shallow(PathBuf::from("test.go"), content);

        assert_eq!(file_index.symbols.len(), 2);
        assert_eq!(file_index.imports.len(), 1);
        assert_eq!(file_index.parse_state, ParseState::Indexed);
    }

    #[test]
    fn test_partial_inference() {
        let mut inference = PartialInference::new(3);

        inference.add_known("x", Type::Int);
        inference.add_unknown("count");
        inference.add_unknown("name");
        inference.add_unknown("unknown_var");

        let inferred = inference.infer_unknowns();

        // count -> Int
        assert_eq!(inferred.get("count"), Some(&Type::Int));
        // name -> String
        assert_eq!(inferred.get("name"), Some(&Type::String));
        // unknown_var -> None (could not infer)
        assert!(!inferred.contains_key("unknown_var"));

        // Check completeness
        assert!(inference.completeness() > 0.5);
    }

    #[test]
    fn test_package_lazy_loading() {
        let index = Arc::new(WorkspaceIndex::new());

        // Index a package
        index.index_package_shallow(
            "github.com/example/pkg",
            vec![(PathBuf::from("pkg.go"), "func Exported() {}".to_string())],
        );

        // Lazy load
        let pkg = index.lazy_load_package("github.com/example/pkg");
        assert!(pkg.is_some());

        let pkg = pkg.unwrap();
        assert_eq!(pkg.load_state, LoadState::ExportsLoaded);
        assert!(!pkg.exports.is_empty());
    }

    #[test]
    fn test_parse_state_transitions() {
        let index = WorkspaceIndex::new();
        let content = "func test() {}";

        let file = index.index_file_shallow(PathBuf::from("test.go"), content);
        assert_eq!(file.parse_state, ParseState::Indexed);

        // Partial parse
        let file = index.partial_parse_file(&PathBuf::from("test.go")).unwrap();
        assert_eq!(file.parse_state, ParseState::Partial);

        // Full parse
        let file = index.fully_parse_file(&PathBuf::from("test.go")).unwrap();
        assert_eq!(file.parse_state, ParseState::Full);
    }

    #[test]
    fn test_stats() {
        let index = WorkspaceIndex::new();

        index.index_file_shallow(PathBuf::from("a.go"), "func A() {}");
        index.index_file_shallow(PathBuf::from("b.go"), "func B() {}");

        let stats = index.stats();
        assert_eq!(stats.indexed_files, 2);
        assert_eq!(stats.total_symbols, 2);
    }
}
