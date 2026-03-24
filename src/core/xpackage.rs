//! Cross-package code navigation

use super::symbol::SymbolId;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub package: Arc<str>,
    pub file: std::path::PathBuf,
    pub line: usize,
    pub column: usize,
}

pub struct CrossPackageIndex {
    symbol_locations: DashMap<SymbolId, SymbolLocation>,
    packages: DashMap<Arc<str>, PackageNode>,
}

#[derive(Debug, Clone)]
pub struct PackageNode {
    pub path: Arc<str>,
    pub imports: std::collections::HashSet<Arc<str>>,
    pub exports: Vec<SymbolId>,
}

impl CrossPackageIndex {
    pub fn new() -> Self {
        Self {
            symbol_locations: DashMap::new(),
            packages: DashMap::new(),
        }
    }

    pub fn register_symbol(&self, symbol: SymbolId, location: SymbolLocation) {
        self.symbol_locations.insert(symbol, location);
    }

    pub fn find_definition(&self, symbol: SymbolId) -> Option<SymbolLocation> {
        self.symbol_locations.get(&symbol).map(|l| l.clone())
    }

    /// Get all registered package paths
    pub fn all_packages(&self) -> Vec<Arc<str>> {
        self.packages.iter().map(|e| e.key().clone()).collect()
    }
}

impl Default for CrossPackageIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Cross-package navigation helper
pub struct CrossPackageNavigator {
    index: Arc<CrossPackageIndex>,
}

impl CrossPackageNavigator {
    pub fn new(index: Arc<CrossPackageIndex>) -> Self {
        Self { index }
    }

    pub fn goto_definition(&self, symbol: SymbolId) -> Option<SymbolLocation> {
        self.index.find_definition(symbol)
    }
}
