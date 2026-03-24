//! 对比测试：salsa-rs vs 简化版 salsa
#![allow(clippy::all, unused_imports, unused_variables)]
//!
//! 测试维度：
//! - 冷启动时间
//! - 增量更新时间
//! - 缓存命中率
//! - 内存占用

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::time::{Duration, Instant};

// salsa-rs (完整实现)
use salsa::Setter;
use wootype::salsa_full::*;

// 简化版 salsa
use wootype::salsa::{BinaryOp, Expression, FunctionBody, IncrementalDb, InputManager, Statement};

/// 创建测试用的 Go 代码
fn create_test_source(num_functions: usize) -> String {
    let mut content = String::from("package main\n\n");
    for i in 0..num_functions {
        content.push_str(&format!(
            "func Func{}() int {{\n    return {}\n}}\n\n",
            i, i
        ));
    }
    content
}

/// 创建简化版 salsa 的测试数据
fn setup_legacy_db(num_functions: usize) -> IncrementalDb {
    let db = IncrementalDb::new();
    for i in 0..num_functions {
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
        db.set_function(format!("func{}", i), body);
    }
    db
}

/// Benchmark 1: 冷启动对比
fn bench_cold_start_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold_start_comparison");

    for size in [10, 100, 1000].iter() {
        // salsa-rs 冷启动
        group.bench_with_input(BenchmarkId::new("salsa_rs", size), size, |b, &size| {
            b.iter_batched(
                || create_test_source(size),
                |content| {
                    let db = TypeDatabase::new();
                    let source =
                        SourceFile::new(&db, std::path::PathBuf::from("test.go"), content, 1);
                    let parsed = parse_file(&db, source);
                    black_box(parsed.functions(&db).len());
                },
                criterion::BatchSize::SmallInput,
            );
        });

        // 简化版 salsa 冷启动
        group.bench_with_input(BenchmarkId::new("legacy_salsa", size), size, |b, &size| {
            b.iter(|| {
                let db = setup_legacy_db(size);
                for i in 0..size {
                    let result = db.infer_function(&format!("func{}", i));
                    black_box(result.return_type);
                }
            });
        });
    }

    group.finish();
}

/// Benchmark 2: 增量更新对比
fn bench_incremental_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_comparison");

    for size in [10, 100, 1000].iter() {
        // salsa-rs 增量更新
        group.bench_with_input(BenchmarkId::new("salsa_rs", size), size, |b, &size| {
            b.iter_batched(
                || {
                    let content = create_test_source(size);
                    let db = TypeDatabase::new();
                    let source =
                        SourceFile::new(&db, std::path::PathBuf::from("test.go"), content, 1);
                    // 预解析
                    let _ = parse_file(&db, source);
                    (db, source)
                },
                |(mut db, source)| {
                    // 修改一个函数
                    let old_content = source.content(&db);
                    let new_content = old_content.replace("func Func5()", "func Func5Modified()");
                    source.set_content(&mut db).to(new_content);

                    // 增量重新解析
                    let parsed = parse_file(&db, source);
                    black_box(parsed.functions(&db).len());
                },
                criterion::BatchSize::SmallInput,
            );
        });

        // 简化版 salsa 增量更新
        group.bench_with_input(BenchmarkId::new("legacy_salsa", size), size, |b, &size| {
            b.iter_batched(
                || {
                    let db = setup_legacy_db(size);
                    // 预热缓存
                    for i in 0..size {
                        let _ = db.infer_function(&format!("func{}", i));
                    }
                    (db, size)
                },
                |(db, size)| {
                    // 修改一个函数
                    let idx = size / 2;
                    let new_body = FunctionBody {
                        statements: vec![],
                        return_expr: Some(Expression::FloatLiteral(3.14)),
                    };
                    db.set_function(format!("func{}", idx), new_body);

                    // 重新检查
                    let result = db.infer_function(&format!("func{}", idx));
                    black_box(result.return_type);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark 3: 缓存查询性能对比
fn bench_cached_query_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("cached_query_comparison");

    // salsa-rs 缓存查询
    group.bench_function("salsa_rs_cached", |b| {
        let content = create_test_source(100);
        let db = TypeDatabase::new();
        let source = SourceFile::new(&db, std::path::PathBuf::from("test.go"), content, 1);
        // 预解析
        let _ = file_symbols(&db, source);

        b.iter(|| {
            // 应该立即返回缓存结果
            let symbols = file_symbols(&db, source);
            black_box(symbols.exports(&db).len());
        });
    });

    // 简化版 salsa 缓存查询
    group.bench_function("legacy_salsa_cached", |b| {
        let db = setup_legacy_db(100);
        // 预热
        for i in 0..100 {
            let _ = db.infer_function(&format!("func{}", i));
        }

        b.iter(|| {
            let result = db.infer_function("func50");
            black_box(result.return_type);
        });
    });

    group.finish();
}

/// Benchmark 4: LSP 实时响应对比
fn bench_lsp_response_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsp_response_comparison");

    // salsa-rs 单字符输入
    group.bench_function("salsa_rs_single_char", |b| {
        let content = r#"package main

func main() {
    x := 42
}
"#
        .to_string();

        let mut db = TypeDatabase::new();
        let source = SourceFile::new(&db, std::path::PathBuf::from("test.go"), content, 1);

        b.iter(|| {
            source.apply_change(
                &mut db,
                TextChange {
                    start: 42,
                    end: 42,
                    new_text: "0".to_string(),
                },
            );
            let parsed = parse_file(&db, source);
            black_box(parsed.functions(&db).len());
        });
    });

    // 简化版 salsa 单字符输入
    group.bench_function("legacy_salsa_single_char", |b| {
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
            let change = wootype::salsa::IncrementalChange {
                file: path.clone(),
                range: wootype::salsa::ChangeRange {
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

/// Benchmark 5: 多文件项目对比
fn bench_multi_file_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_file_comparison");

    // salsa-rs 多文件
    group.bench_function("salsa_rs_multi_file", |b| {
        b.iter_batched(
            || {
                let db = TypeDatabase::new();
                let files: Vec<_> = (0..10)
                    .map(|i| {
                        SourceFile::new(
                            &db,
                            std::path::PathBuf::from(format!("file{}.go", i)),
                            create_test_source(10),
                            1,
                        )
                    })
                    .collect();
                (db, files)
            },
            |(db, files)| {
                for source in &files {
                    let parsed = parse_file(&db, *source);
                    black_box(parsed.functions(&db).len());
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // 简化版不支持真正的多文件，使用单文件模拟
    group.bench_function("legacy_salsa_multi_file", |b| {
        b.iter(|| {
            for i in 0..10 {
                let db = setup_legacy_db(10);
                for j in 0..10 {
                    let result = db.infer_function(&format!("func{}", j));
                    black_box(result.return_type);
                }
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cold_start_comparison,
    bench_incremental_comparison,
    bench_cached_query_comparison,
    bench_lsp_response_comparison,
    bench_multi_file_comparison
);
criterion_main!(benches);
