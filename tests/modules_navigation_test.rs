//! Integration tests for:
//! 1. Go Modules complete support
//! 2. Cross-package code navigation
//! 3. Incremental update optimization

use std::collections::HashSet;
use std::sync::Arc;
use wootype::core::{CrossPackageIndex, CrossPackageNavigator, ModuleResolver, SymbolId};
use wootype::salsa_full::{ChangeSet, ChangeType, DependencyGraph, IncrementalProcessor};

// ===== Feature 1: Go Modules Tests =====

#[test]
fn test_stdlib_detection() {
    let resolver = ModuleResolver::new();

    // Standard library packages should be detected
    assert!(resolver.resolve_import("fmt").is_some());
    assert!(resolver.resolve_import("net/http").is_some());
    assert!(resolver.resolve_import("context").is_some());
    assert!(resolver.resolve_import("os/exec").is_some());
}

#[test]
fn test_non_stdlib_not_resolved() {
    let resolver = ModuleResolver::new();

    // External packages should not be resolved without go.mod
    assert!(resolver.resolve_import("github.com/foo/bar").is_none());
    assert!(resolver.resolve_import("example.com/mymodule").is_none());
}

#[test]
fn test_module_resolver_creation() {
    let resolver = ModuleResolver::new();
    assert!(resolver.module_path().is_none()); // No go.mod loaded
}

// ===== Feature 2: Cross-package Navigation Tests =====

#[test]
fn test_cross_package_index_creation() {
    let index = CrossPackageIndex::new();
    // Should create without panic
    let packages = index.all_packages();
    assert_eq!(packages.len(), 0);
}

#[test]
fn test_symbol_registration_and_lookup() {
    use wootype::core::xpackage::SymbolLocation;

    let index = CrossPackageIndex::new();
    let symbol = SymbolId::new(1);
    let location = SymbolLocation {
        package: Arc::from("test/package"),
        file: std::path::PathBuf::from("/test/file.go"),
        line: 10,
        column: 5,
    };

    index.register_symbol(symbol, location.clone());

    let found = index.find_definition(symbol);
    assert!(found.is_some());
    assert_eq!(found.unwrap().line, 10);
}

#[test]
fn test_cross_package_navigator() {
    use wootype::core::xpackage::SymbolLocation;

    let index = Arc::new(CrossPackageIndex::new());
    let navigator = CrossPackageNavigator::new(index.clone());

    let symbol = SymbolId::new(42);
    let location = SymbolLocation {
        package: Arc::from("github.com/example/lib"),
        file: std::path::PathBuf::from("/project/lib/helper.go"),
        line: 25,
        column: 1,
    };

    index.register_symbol(symbol, location);

    let result = navigator.goto_definition(symbol);
    assert!(result.is_some());
    assert_eq!(result.unwrap().line, 25);
}

// ===== Feature 3: Incremental Update Tests =====

#[test]
fn test_dependency_graph_creation() {
    let _graph = DependencyGraph::new();
    // Should create without panic
}

#[test]
fn test_dependency_registration() {
    let graph = DependencyGraph::new();
    let file = std::path::PathBuf::from("/test/main.go");

    graph.register_symbol(&file, "MyFunc".to_string());

    // Register dependency
    let dep_file = std::path::PathBuf::from("/test/other.go");
    graph.add_dependency(&dep_file, "MyFunc");
}

#[test]
fn test_affected_files_computation() {
    let graph = DependencyGraph::new();

    let changed_file = std::path::PathBuf::from("/test/changed.go");
    let mut changes = ChangeSet {
        changed_files: HashSet::new(),
        changed_symbols: HashSet::new(),
        change_type: ChangeType::FileContent,
    };
    changes.changed_files.insert(changed_file);

    let affected = graph.affected_files(&changes);
    assert!(affected.contains(&std::path::PathBuf::from("/test/changed.go")));
}

#[test]
fn test_incremental_processor_creation() {
    let graph = Arc::new(DependencyGraph::new());
    let _processor = IncrementalProcessor::new(graph);
    // Should create without panic
}
