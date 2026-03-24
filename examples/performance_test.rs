//! Performance test for wootype
//!
//! Measures key performance metrics:
//! - Type lookup latency
//! - Symbol interning speed
//! - Cache hit performance
//! - Incremental update speed

use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    println!("=== wootype Performance Test ===\n");

    // Test 1: Type Lookup
    test_type_lookup();

    // Test 2: Symbol Interning
    test_symbol_interning();

    // Test 3: Cache Operations
    test_cache_operations();

    // Test 4: Incremental Type Check
    test_incremental_check();

    // Test 5: Cross-package Navigation
    test_cross_package_nav();

    println!("\n=== Performance Test Complete ===");
}

fn test_type_lookup() {
    use wootype::core::{TypeId, TypeUniverse};
    use wootype::query::QueryEngine;

    println!("1. Type Lookup Performance");

    let universe = Arc::new(TypeUniverse::new());
    let engine = QueryEngine::new(universe);

    // Warm up
    for i in 1..100 {
        let _ = engine.get_type(TypeId(i));
    }

    // Measure
    let iterations = 1_000_000;
    let start = Instant::now();

    for i in 1..iterations {
        let _ = engine.get_type(TypeId(i % 100));
    }

    let elapsed = start.elapsed();
    let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;

    println!("   Iterations: {}", iterations);
    println!("   Total time: {:?}", elapsed);
    println!("   Per lookup: {:.2} ns", ns_per_op);
    println!();
}

fn test_symbol_interning() {
    use wootype::core::symbol::SymbolTable;

    println!("2. Symbol Interning Performance");

    let table = SymbolTable::new();

    // Measure
    let iterations = 100_000;
    let start = Instant::now();

    for i in 0..iterations {
        let name = format!("symbol_{}", i);
        let _ = table.intern(&name);
    }

    let elapsed = start.elapsed();
    let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;
    let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

    println!("   Iterations: {}", iterations);
    println!("   Total time: {:?}", elapsed);
    println!("   Per intern: {:.2} ns", ns_per_op);
    println!("   Ops/sec: {:.2}M", ops_per_sec / 1_000_000.0);
    println!();
}

fn test_cache_operations() {
    use wootype::query::cache::QueryCache;

    println!("3. Cache Operations Performance");

    let cache = QueryCache::<String, i32>::new(1000);

    // Pre-populate
    for i in 0..100 {
        cache.insert(format!("key_{}", i), i);
    }

    // Measure reads
    let iterations = 1_000_000;
    let start = Instant::now();

    for i in 0..iterations {
        let key = format!("key_{}", i % 100);
        let _ = cache.get(&key);
    }

    let elapsed = start.elapsed();
    let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;

    println!("   Cache reads: {}", iterations);
    println!("   Total time: {:?}", elapsed);
    println!("   Per read: {:.2} ns", ns_per_op);
    println!();
}

fn test_incremental_check() {
    use wootype::salsa::*;

    println!("4. Incremental Type Check Performance");

    let db = IncrementalDb::new();
    let num_functions = 1000;

    // Setup functions
    for i in 0..num_functions {
        let body = FunctionBody {
            statements: vec![Statement::VarDecl(
                "x".to_string(),
                Expression::IntLiteral(i as i64),
            )],
            return_expr: Some(Expression::IntLiteral(i as i64)),
        };
        db.set_function(format!("func{}", i), body);
    }

    // Cold check
    let start = Instant::now();
    for i in 0..num_functions {
        let _ = db.infer_function(&format!("func{}", i));
    }
    let cold_time = start.elapsed();

    // Incremental check (change one function)
    let start = Instant::now();
    let new_body = FunctionBody {
        statements: vec![],
        return_expr: Some(Expression::FloatLiteral(3.14)),
    };
    db.set_function("func500".to_string(), new_body);
    let _ = db.infer_function("func500");
    let incremental_time = start.elapsed();

    println!("   Functions: {}", num_functions);
    println!("   Cold check: {:?}", cold_time);
    println!("   Incremental: {:?}", incremental_time);
    println!(
        "   Speedup: {:.0}x",
        cold_time.as_nanos() as f64 / incremental_time.as_nanos() as f64
    );
    println!();
}

fn test_cross_package_nav() {
    use wootype::core::xpackage::SymbolLocation;
    use wootype::core::{CrossPackageIndex, CrossPackageNavigator, SymbolId};

    println!("5. Cross-package Navigation Performance");

    let index = Arc::new(CrossPackageIndex::new());
    let navigator = CrossPackageNavigator::new(index.clone());

    // Register symbols
    let num_symbols = 10000;
    for i in 0..num_symbols {
        let symbol = SymbolId::new(i);
        let location = SymbolLocation {
            package: Arc::from(format!("github.com/example/pkg{}", i % 100)),
            file: std::path::PathBuf::from(format!("/project/file{}.go", i)),
            line: (i % 1000) as usize,
            column: (i % 80) as usize,
        };
        index.register_symbol(symbol, location);
    }

    // Measure lookups
    let iterations = 1_000_000;
    let start = Instant::now();

    for i in 0..iterations {
        let symbol = SymbolId::new((i % num_symbols) as u32);
        let _ = navigator.goto_definition(symbol);
    }

    let elapsed = start.elapsed();
    let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;

    println!("   Registered symbols: {}", num_symbols);
    println!("   Lookups: {}", iterations);
    println!("   Total time: {:?}", elapsed);
    println!("   Per lookup: {:.2} ns", ns_per_op);
    println!();
}
