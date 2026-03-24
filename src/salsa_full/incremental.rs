//! Incremental update optimization
//!
//! Fine-grained dependency tracking and parallel incremental computation

use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Incremental change tracking
#[derive(Debug, Clone)]
pub struct ChangeSet {
    pub changed_files: HashSet<std::path::PathBuf>,
    pub changed_symbols: HashSet<crate::salsa_full::Symbol>,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    FileContent,
    FileAdded,
    FileDeleted,
    SymbolModified,
}

/// Dependency graph for fine-grained invalidation
pub struct DependencyGraph {
    /// File -> symbols defined in file
    file_to_symbols: DashMap<std::path::PathBuf, HashSet<String>>,
    /// Symbol -> files that depend on it
    symbol_dependents: DashMap<String, HashSet<std::path::PathBuf>>,
    /// File -> files that import it
    file_imports: DashMap<std::path::PathBuf, HashSet<std::path::PathBuf>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            file_to_symbols: DashMap::new(),
            symbol_dependents: DashMap::new(),
            file_imports: DashMap::new(),
        }
    }

    /// Register a symbol definition
    pub fn register_symbol(&self, file: &std::path::PathBuf, symbol: String) {
        self.file_to_symbols
            .entry(file.clone())
            .or_default()
            .insert(symbol);
    }

    /// Register a dependency
    pub fn add_dependency(&self, from: &std::path::PathBuf, symbol: &str) {
        self.symbol_dependents
            .entry(symbol.to_string())
            .or_default()
            .insert(from.clone());
    }

    /// Compute all affected files for a change set
    pub fn affected_files(&self, changes: &ChangeSet) -> HashSet<std::path::PathBuf> {
        let mut affected = HashSet::new();
        let mut queue: Vec<_> = changes.changed_files.iter().cloned().collect();

        while let Some(file) = queue.pop() {
            if affected.contains(&file) {
                continue;
            }

            affected.insert(file.clone());

            // Find files that import this file
            if let Some(dependents) = self.file_imports.get(&file) {
                for dep in dependents.iter() {
                    if !affected.contains(dep) {
                        queue.push(dep.clone());
                    }
                }
            }
        }

        affected
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel incremental processor
pub struct IncrementalProcessor {
    graph: Arc<DependencyGraph>,
    cache: DashMap<String, Arc<str>>,
}

impl IncrementalProcessor {
    pub fn new(graph: Arc<DependencyGraph>) -> Self {
        Self {
            graph,
            cache: DashMap::new(),
        }
    }

    /// Process changes in parallel
    pub fn process_changes(&self, changes: ChangeSet) -> Vec<String> {
        use rayon::prelude::*;

        let affected = self.graph.affected_files(&changes);

        affected
            .into_par_iter()
            .map(|file| format!("Processed: {:?}", file))
            .collect()
    }
}
