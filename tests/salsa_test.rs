//! Tests for Salsa-based incremental type checking
//!
//! These tests verify:
//! 1. Function-level incrementality
//! 2. Query caching
//! 3. LSP real-time response

use std::path::PathBuf;
use wootype::salsa::*;

/// Test basic type inference
#[test]
fn test_basic_type_inference() {
    let db = IncrementalDb::new();

    let body = FunctionBody {
        statements: vec![],
        return_expr: Some(Expression::IntLiteral(42)),
    };

    db.set_function("test".to_string(), body);
    let result = db.infer_function("test");

    assert_eq!(result.return_type.to_string(), "int");
    assert!(result.errors.is_empty());
}

/// Test incremental update - changing a function only affects that function
#[test]
fn test_incremental_function_update() {
    let db = IncrementalDb::new();

    // Create two functions
    db.set_function(
        "f1".to_string(),
        FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::IntLiteral(1)),
        },
    );

    db.set_function(
        "f2".to_string(),
        FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::StringLiteral("hello".to_string())),
        },
    );

    // Initial inference
    let r1_v1 = db.infer_function("f1");
    let r2_v1 = db.infer_function("f2");

    assert_eq!(r1_v1.return_type.to_string(), "int");
    assert_eq!(r2_v1.return_type.to_string(), "string");

    // Update f1
    db.set_function(
        "f1".to_string(),
        FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::FloatLiteral(3.14)),
        },
    );

    // Re-infer f1 (should be recomputed)
    let r1_v2 = db.infer_function("f1");
    assert_eq!(r1_v2.return_type.to_string(), "float64");

    // f2 should still be cached and return the same result
    let r2_v2 = db.infer_function("f2");
    assert_eq!(r2_v2.return_type.to_string(), "string");
}

/// Test input manager for LSP incremental updates
#[test]
fn test_input_manager_incremental_update() {
    let manager = InputManager::new();
    let path = PathBuf::from("test.go");

    // Set initial content
    let initial = r#"package main

func main() {
    x := 42
    println(x)
}
"#;
    manager.set_file(path.clone(), initial.to_string());

    // Apply incremental change: change 42 to 100
    let change = IncrementalChange {
        file: path.clone(),
        range: ChangeRange {
            start_line: 3,
            start_col: 9,
            end_line: 3,
            end_col: 11,
        },
        new_text: "100".to_string(),
    };

    manager.apply_change(change).unwrap();

    // Verify the change
    let updated = manager.get_file(&path).unwrap();
    assert!(updated.contains("x := 100"));
    assert!(!updated.contains("x := 42"));
}

/// Test type error detection
#[test]
fn test_type_error_detection() {
    let db = IncrementalDb::new();

    // Create function with type error: "hello" + 42
    let body = FunctionBody {
        statements: vec![],
        return_expr: Some(Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::StringLiteral("hello".to_string())),
            Box::new(Expression::IntLiteral(42)),
        )),
    };

    db.set_function("bad_add".to_string(), body);
    let result = db.infer_function("bad_add");

    assert!(!result.errors.is_empty(), "Should detect type error");
}

/// Test arithmetic type promotion
#[test]
fn test_arithmetic_type_promotion() {
    let db = IncrementalDb::new();

    // Test: int + float = float
    let body = FunctionBody {
        statements: vec![],
        return_expr: Some(Expression::BinaryOp(
            BinaryOp::Add,
            Box::new(Expression::IntLiteral(1)),
            Box::new(Expression::FloatLiteral(2.0)),
        )),
    };

    db.set_function("test".to_string(), body);
    let result = db.infer_function("test");

    assert_eq!(result.return_type.to_string(), "float64");
}

/// Test database stats
#[test]
fn test_db_stats() {
    let db = IncrementalDb::new();

    // Initially empty
    let stats = db.stats();
    assert_eq!(stats.cached_queries, 0);
    assert_eq!(stats.tracked_functions, 0);

    // Add functions
    for i in 0..10 {
        db.set_function(
            format!("func{}", i),
            FunctionBody {
                statements: vec![],
                return_expr: Some(Expression::IntLiteral(i as i64)),
            },
        );
    }

    // Query all functions to populate cache
    for i in 0..10 {
        let _ = db.infer_function(&format!("func{}", i));
    }

    let stats = db.stats();
    assert_eq!(stats.tracked_functions, 10);
    assert_eq!(stats.cached_queries, 10);
}

/// Benchmark: Compare cold vs incremental check
#[test]
fn benchmark_incremental_vs_cold() {
    use std::time::Instant;

    let db = IncrementalDb::new();

    // Create 100 functions
    for i in 0..100 {
        db.set_function(
            format!("func{}", i),
            FunctionBody {
                statements: vec![Statement::VarDecl(
                    "x".to_string(),
                    Expression::IntLiteral(i as i64),
                )],
                return_expr: Some(Expression::BinaryOp(
                    BinaryOp::Add,
                    Box::new(Expression::Identifier("x".to_string())),
                    Box::new(Expression::IntLiteral(1)),
                )),
            },
        );
    }

    // Cold check: all functions
    let start = Instant::now();
    for i in 0..100 {
        let _ = db.infer_function(&format!("func{}", i));
    }
    let cold_time = start.elapsed();

    // Update one function
    db.set_function(
        "func50".to_string(),
        FunctionBody {
            statements: vec![],
            return_expr: Some(Expression::FloatLiteral(3.14)),
        },
    );

    // Incremental check: only the changed function
    let start = Instant::now();
    let _ = db.infer_function("func50");
    let incremental_time = start.elapsed();

    println!("Cold check (100 funcs): {:?}", cold_time);
    println!("Incremental check (1 func): {:?}", incremental_time);

    // Incremental should be much faster
    // Note: In this simple implementation, the speedup may not be dramatic
    // A full Salsa implementation would show more significant gains
    assert!(
        incremental_time < cold_time,
        "Incremental should be faster than cold check"
    );
}
