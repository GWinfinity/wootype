//! Benchmark for incremental type checking
//!
//! Compares:
//! - Cold check (full type check)
//! - Incremental check (only changed functions)
//! - LSP real-time response latency

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::time::Duration;

use wootype::salsa::*;

/// Setup: Create a database with N functions
fn setup_database(num_functions: usize) -> (IncrementalDb, Vec<String>) {
    let db = IncrementalDb::new();
    let mut function_names = Vec::with_capacity(num_functions);

    for i in 0..num_functions {
        let name = format!("func{}", i);
        let body = FunctionBody {
            statements: vec![Statement::VarDecl(
                "x".to_string(),
                Expression::IntLiteral(i as i64),
            )],
            return_expr: Some(Expression::BinaryOp(
                BinaryOp::Add,
                Box::new(Expression::Identifier("x".to_string())),
                Box::new(Expression::IntLiteral(1)),
            )),
        };

        db.set_function(name.clone(), body);
        function_names.push(name);
    }

    (db, function_names)
}

/// Benchmark cold check (full check)
fn bench_cold_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold_check");
    group.measurement_time(Duration::from_secs(10));

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || setup_database(size),
                |(db, function_names)| {
                    for name in &function_names {
                        let result = db.infer_function(name);
                        black_box(result.return_type);
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark incremental check
fn bench_incremental_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_check");
    group.measurement_time(Duration::from_secs(10));

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || {
                    let (db, function_names) = setup_database(size);
                    // Pre-warm cache
                    for name in &function_names {
                        let _ = db.infer_function(name);
                    }
                    (db, function_names)
                },
                |(db, function_names)| {
                    // Change one function
                    let idx = function_names.len() / 2;
                    let new_body = FunctionBody {
                        statements: vec![],
                        return_expr: Some(Expression::FloatLiteral(3.14)),
                    };
                    db.set_function(function_names[idx].clone(), new_body);

                    // Re-check only the changed function
                    let result = db.infer_function(&function_names[idx]);
                    black_box(result.return_type);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark LSP-like incremental update
fn bench_lsp_incremental(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp_incremental");

    group.bench_function("single_char_insert", |b| {
        let manager = InputManager::new();
        let path = std::path::PathBuf::from("test.go");

        let content = r#"package main

func main() {
    x := 42
}
"#
        .to_string();

        manager.set_file(path.clone(), content);

        b.iter(|| {
            // Simulate typing one character
            let change = IncrementalChange {
                file: path.clone(),
                range: ChangeRange {
                    start_line: 3,
                    start_col: 11,
                    end_line: 3,
                    end_col: 11,
                },
                new_text: "0".to_string(),
            };

            manager.apply_change(change).unwrap();
            black_box(manager.get_file(&path));
        });
    });

    group.finish();
}

/// Benchmark function-level isolation
fn bench_function_isolation(c: &mut Criterion) {
    let mut group = c.benchmark_group("function_isolation");

    group.bench_function("check_dependencies", |b| {
        let (db, function_names) = setup_database(100);

        b.iter(|| {
            // Change middle function
            let idx = 50;
            let new_body = FunctionBody {
                statements: vec![],
                return_expr: Some(Expression::StringLiteral("changed".to_string())),
            };
            db.set_function(function_names[idx].clone(), new_body);

            // Re-check the changed function
            let result = db.infer_function(&function_names[idx]);
            black_box(result.return_type);
        });
    });

    group.finish();
}

/// Compare ty-style performance claims
fn bench_ty_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("ty_comparison");
    group.measurement_time(Duration::from_secs(5));

    // Simulate ty's benchmark: PyTorch-like codebase
    group.bench_function("pytorch_like_incremental", |b| {
        let (db, function_names) = setup_database(1000);

        // Pre-warm
        for name in &function_names {
            let _ = db.infer_function(name);
        }

        b.iter(|| {
            // Change a "load-bearing" function
            let new_body = FunctionBody {
                statements: vec![],
                return_expr: Some(Expression::IntLiteral(999)),
            };
            db.set_function(function_names[0].clone(), new_body);

            // Re-check
            let result = db.infer_function(&function_names[0]);
            black_box(result.return_type);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cold_check,
    bench_incremental_check,
    bench_lsp_incremental,
    bench_function_isolation,
    bench_ty_comparison
);
criterion_main!(benches);
