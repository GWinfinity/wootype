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

use parking_lot::RwLock;
use dashmap::DashMap;
use rayon::prelude::*;

use super::{Type, Symbol, SymbolKind, Location, Span, SourceFile, TypeDatabase};

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
}

/// Index for a single file
#[derive(Clone, Debug)]
pub struct FileIndex {
    pub path: PathBuf,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<String>,
    pub revision: u64,
    pub fully_parsed: bool,  // false = only indexed, not fully parsed
}

/// Symbol with location info
#[derive(Clone, Debug)]
pub struct SymbolLocation {
    pub symbol: Symbol,
    pub file: PathBuf,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self {
            files: DashMap::new(),
            symbol_table: DashMap::new(),
            package_exports: DashMap::new(),
            dirty_files: RwLock::new(HashSet::new()),
        }
    }
    
    /// Index a file (shallow - only symbols, not full AST)
    pub fn index_file_shallow(&self, path: PathBuf, content: &str) -> FileIndex {
        let mut symbols = vec![];
        let mut imports = vec![];
        
        // Quick regex-based symbol extraction
        // In production, use tree-sitter or similar
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
                    ty: Type::Unknown, // Not resolved yet
                    location: Location {
                        file: path.clone(),
                        span: Span {
                            start: 0,
                            end: 0,
                            line: line_num,
                            column: 0,
                        },
                    },
                    is_exported: func_name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false),
                    docs: None,
                };
                
                symbols.push(symbol.clone());
                
                // Add to symbol table
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
                        is_exported: type_name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false),
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
            fully_parsed: false,
        };
        
        self.files.insert(path, index.clone());
        index
    }
    
    /// Full parse of a file (on-demand)
    pub fn fully_parse_file(&self, path: &PathBuf) -> Option<FileIndex> {
        if let Some(mut entry) = self.files.get_mut(path) {
            if !entry.fully_parsed {
                // Trigger full parse via Salsa
                // This would call into the salsa database
                entry.fully_parsed = true;
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
    
    /// Check if a package exists in the index
    pub fn package_exists(&self, package_path: &str) -> bool {
        self.package_exports.contains_key(package_path)
    }
    
    /// Index a package (but not its dependencies)
    pub fn index_package_shallow(&self, package_path: &str, files: Vec<(PathBuf, String)>) {
        let mut exports = vec![];
        
        for (path, content) in files {
            let index = self.index_file_shallow(path, &content);
            
            // Collect exports
            for symbol in &index.symbols {
                if symbol.is_exported {
                    exports.push(symbol.clone());
                }
            }
        }
        
        self.package_exports.insert(package_path.to_string(), exports);
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
    
    /// Mark file as dirty (needs re-indexing)
    pub fn mark_dirty(&self, path: &PathBuf) {
        self.dirty_files.write().insert(path.clone());
    }
    
    /// Process dirty files
    pub fn process_dirty_files(&self) {
        let dirty: Vec<_> = self.dirty_files.write().drain().collect();
        
        for path in dirty {
            // Re-index the file
            // In a real implementation, we'd re-read the file content
        }
    }
    
    /// Memory-efficient partial index for very large files
    pub fn index_file_partial(&self, path: PathBuf, content: &str, max_lines: usize) -> FileIndex {
        let partial_content: String = content.lines().take(max_lines).collect::<Vec<_>>().join("\n");
        
        let mut index = self.index_file_shallow(path.clone(), &partial_content);
        index.fully_parsed = false;  // Mark as partial
        
        index
    }
    
    /// Get statistics
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            indexed_files: self.files.len(),
            total_symbols: self.symbol_table.len(),
            dirty_files: self.dirty_files.read().len(),
            memory_estimate_mb: self.estimate_memory(),
        }
    }
    
    fn estimate_memory(&self) -> usize {
        // Rough estimate
        let file_size = self.files.len() * std::mem::size_of::<FileIndex>();
        let symbol_size = self.symbol_table.len() * std::mem::size_of::<SymbolLocation>();
        (file_size + symbol_size) / (1024 * 1024)
    }
}

#[derive(Clone, Debug)]
pub struct IndexStats {
    pub indexed_files: usize,
    pub total_symbols: usize,
    pub dirty_files: usize,
    pub memory_estimate_mb: usize,
}

impl Default for WorkspaceIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazy package loader
pub struct PackageLoader {
    /// Loaded packages
    loaded: DashMap<String, Package>,
    /// Package index (not fully loaded)
    index: Arc<WorkspaceIndex>,
}

#[derive(Clone, Debug)]
pub struct Package {
    pub path: String,
    pub files: Vec<PathBuf>,
    pub exports: Vec<Symbol>,
    pub fully_loaded: bool,
}

impl PackageLoader {
    pub fn new(index: Arc<WorkspaceIndex>) -> Self {
        Self {
            loaded: DashMap::new(),
            index,
        }
    }
    
    /// Get package (loading if necessary)
    pub fn get_package(&self, path: &str) -> Option<Package> {
        // Check if already loaded
        if let Some(pkg) = self.loaded.get(path) {
            return Some(pkg.clone());
        }
        
        // Check if package exists in index
        // (A package exists if it has exports or is explicitly tracked)
        if !self.index.package_exists(path) {
            return None;
        }
        
        // Load from index (shallow)
        let exports = self.index.get_package_exports(path);
        
        let package = Package {
            path: path.to_string(),
            files: vec![], // Would be populated
            exports,
            fully_loaded: false,
        };
        
        self.loaded.insert(path.to_string(), package.clone());
        Some(package)
    }
    
    /// Fully load a package (including all files)
    pub fn fully_load_package(&self, path: &str) -> Option<Package> {
        // First get shallow package
        let mut package = self.get_package(path)?;
        
        if package.fully_loaded {
            return Some(package);
        }
        
        // Load all files
        for file_path in &package.files {
            self.index.fully_parse_file(file_path);
        }
        
        package.fully_loaded = true;
        self.loaded.insert(path.to_string(), package.clone());
        
        Some(package)
    }
    
    /// Lazy load: only load when symbol is accessed
    pub fn resolve_symbol(&self, package_path: &str, symbol_name: &str) -> Option<Symbol> {
        // Check if package needs full loading
        let pkg = self.get_package(package_path)?;
        
        // Find symbol in exports
        if let Some(symbol) = pkg.exports.iter().find(|s| s.name == symbol_name) {
            // If symbol is a type, we might need to fully load the package
            if matches!(symbol.kind, SymbolKind::Type | SymbolKind::Interface) {
                self.fully_load_package(package_path);
            }
            return Some(symbol.clone());
        }
        
        None
    }
}

/// Helper functions
fn extract_quoted(s: &str) -> Option<String> {
    s.split('"').nth(1).map(|s| s.to_string())
}

fn extract_function_name(line: &str) -> Option<String> {
    if line.starts_with("func ") {
        let rest = &line[5..];
        // Handle methods: func (r Receiver) MethodName(...)
        if rest.starts_with('(') {
            rest.split(')').nth(1)
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
        
        assert_eq!(file_index.symbols.len(), 2); // main and Helper
        assert_eq!(file_index.imports.len(), 1); // fmt
        assert!(!file_index.fully_parsed);
    }
    
    #[test]
    fn test_find_symbol() {
        let index = WorkspaceIndex::new();
        let content = "func test() {}";
        
        index.index_file_shallow(PathBuf::from("test.go"), content);
        
        let results = index.find_symbol("test");
        assert_eq!(results.len(), 1);
    }
    
    #[test]
    fn test_package_loader() {
        let index = Arc::new(WorkspaceIndex::new());
        let loader = PackageLoader::new(index);
        
        // Initially empty
        assert!(loader.get_package("nonexistent").is_none());
    }
}