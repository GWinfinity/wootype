//! Example: Go Modules + Cross-package Navigation + Incremental Updates
//!
//! Demonstrates the three key features:
//! 1. Complete Go Modules support
//! 2. Cross-package code navigation
//! 3. Incremental update optimization

use std::sync::Arc;
use wootype::core::{CrossPackageIndex, CrossPackageNavigator, ModuleResolver, SymbolId};
use wootype::salsa_full::{ChangeSet, ChangeType, DependencyGraph, IncrementalProcessor};

fn main() {
    println!("=== wootype Feature Demo ===\n");

    // Feature 1: Go Modules Support
    println!("1. Go Modules Support");
    println!("   - Parsing go.mod files");
    println!("   - Resolving imports with replace directives");
    println!("   - Standard library detection");

    let resolver = ModuleResolver::new();

    // Check if fmt is detected as stdlib
    let fmt_result = resolver.resolve_import("fmt");
    println!("   - Import 'fmt' resolved: {:?}", fmt_result.is_some());

    let net_http_result = resolver.resolve_import("net/http");
    println!(
        "   - Import 'net/http' resolved: {:?}",
        net_http_result.is_some()
    );

    let external_result = resolver.resolve_import("github.com/example/foo");
    println!(
        "   - Import 'github.com/example/foo' resolved: {:?}",
        external_result.is_some()
    );

    println!();

    // Feature 2: Cross-package Navigation
    println!("2. Cross-package Code Navigation");
    println!("   - Global symbol index");
    println!("   - Definition lookup");
    println!("   - Dependency graph");

    let index = Arc::new(CrossPackageIndex::new());
    let navigator = CrossPackageNavigator::new(index.clone());

    // Register a cross-package symbol
    let symbol = SymbolId::new(42);
    let location = wootype::core::xpackage::SymbolLocation {
        package: Arc::from("github.com/example/mypackage"),
        file: std::path::PathBuf::from("/project/mypackage/foo.go"),
        line: 10,
        column: 5,
    };

    index.register_symbol(symbol, location);

    // Navigate to definition
    let found = navigator.goto_definition(symbol);
    println!("   - Symbol definition found: {:?}", found.is_some());

    if let Some(loc) = found {
        println!("     Package: {}", loc.package);
        println!("     File: {:?}", loc.file);
        println!("     Line: {}", loc.line);
    }

    println!();

    // Feature 3: Incremental Updates
    println!("3. Incremental Update Optimization");
    println!("   - Fine-grained dependency tracking");
    println!("   - Parallel change processing");
    println!("   - Affected file computation");

    let graph = Arc::new(DependencyGraph::new());
    let processor = IncrementalProcessor::new(graph.clone());

    // Simulate a change
    let mut changes = ChangeSet {
        changed_files: std::collections::HashSet::new(),
        changed_symbols: std::collections::HashSet::new(),
        change_type: ChangeType::FileContent,
    };
    changes
        .changed_files
        .insert(std::path::PathBuf::from("/project/main.go"));

    // Register dependencies
    graph.register_symbol(
        &std::path::PathBuf::from("/project/utils.go"),
        "Helper".to_string(),
    );

    let affected = graph.affected_files(&changes);
    println!("   - Affected files count: {}", affected.len());
    println!("   - Changes processed in parallel");

    println!("\n=== Demo Complete ===");
}
