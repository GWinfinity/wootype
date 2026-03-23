//! Benchmark for full Salsa (salsa-rs) integration
//!
//! Demonstrates:
//! - Cold parse (first time)
//! - Incremental re-parse (after small change)
//! - Query caching (re-executing same query)
//! - LSP-like real-time response

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

use wootype::salsa_full::*;
use salsa::Setter;

/// Setup: Create a source file with N functions
fn setup_source_file(num_functions: usize) -> String {
    let mut content = String::from("package main\n\n");
    
    for i in 0..num_functions {
        content.push_str(&format!(
            "func Func{}() int {{\n    return {}\n}}\n\n",
            i, i
        ));
    }
    
    content
}

/// Setup: Create database with source file
fn setup_database(content: String) -> (TypeDatabase, SourceFile) {
    let db = TypeDatabase::new();
    let source = SourceFile::new(
        &db,
        std::path::PathBuf::from("test.go"),
        content,
        1,
    );
    (db, source)
}

/// Benchmark cold parse (first time)
fn bench_cold_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("salsa_cold_parse");
    group.measurement_time(Duration::from_secs(10));
    
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || setup_source_file(size),
                |content| {
                    let (db, source) = setup_database(content);
                    let parsed = parse_file(&db, source);
                    black_box(parsed.functions(&db).len());
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    
    group.finish();
}

/// Benchmark incremental re-parse after small change
fn bench_incremental_reparse(c: &mut Criterion) {
    let mut group = c.benchmark_group("salsa_incremental_reparse");
    group.measurement_time(Duration::from_secs(10));
    
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || {
                    let content = setup_source_file(size);
                    let (db, source) = setup_database(content);
                    // Pre-warm: parse once
                    let _ = parse_file(&db, source);
                    (db, source)
                },
                |(mut db, source)| {
                    // Make a small change (add comment)
                    let old_content = source.content(&db);
                    let new_content = format!("{}\n// comment", old_content);
                    source.set_content(&mut db).to(new_content);
                    source.set_version(&mut db).to(2);
                    
                    // Re-parse (incremental)
                    let parsed = parse_file(&db, source);
                    black_box(parsed.functions(&db).len());
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    
    group.finish();
}

/// Benchmark query caching (re-executing same query)
fn bench_query_caching(c: &mut Criterion) {
    let mut group = c.benchmark_group("salsa_query_caching");
    
    group.bench_function("cached_file_symbols", |b| {
        let content = setup_source_file(100);
        let (db, source) = setup_database(content);
        
        // Pre-warm
        let _ = file_symbols(&db, source);
        
        b.iter(|| {
            // This should be nearly instant due to memoization
            let symbols = file_symbols(&db, source);
            black_box(symbols.exports(&db).len());
        });
    });
    
    group.bench_function("cached_type_check", |b| {
        let content = setup_source_file(100);
        let (db, source) = setup_database(content);
        
        // Pre-warm
        let _ = type_check_file(&db, source);
        
        b.iter(|| {
            // Re-execute cached query
            let result = type_check_file(&db, source);
            black_box(result.success(&db));
        });
    });
    
    group.finish();
}

/// Benchmark LSP-like real-time response
fn bench_lsp_response(c: &mut Criterion) {
    let mut group = c.benchmark_group("salsa_lsp_response");
    
    // Simulate typing one character
    group.bench_function("single_char_typing", |b| {
        let content = r#"package main

func main() {
    x := 42
}
"#.to_string();
        
        let (mut db, source) = setup_database(content);
        
        b.iter(|| {
            // Apply incremental change
            source.apply_change(&mut db, TextChange {
                start: 42, // Position after "42"
                end: 42,
                new_text: "0".to_string(), // Type "0" to make "420"
            });
            
            // Re-parse and get symbols
            let parsed = parse_file(&db, source);
            black_box(parsed.functions(&db).len());
        });
    });
    
    // Completions request
    group.bench_function("completions_request", |b| {
        let content = setup_source_file(100);
        let (db, source) = setup_database(content);
        
        // Pre-warm
        let _ = file_symbols(&db, source);
        
        b.iter(|| {
            let completions = completions_at(&db, source, 50);
            black_box(completions.len());
        });
    });
    
    group.finish();
}

/// Benchmark scalability with large files
fn bench_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("salsa_scalability");
    group.measurement_time(Duration::from_secs(5));
    
    // Parse increasingly large files
    for size in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(BenchmarkId::new("parse", size), size, |b, &size| {
            let content = setup_source_file(size);
            b.iter_batched(
                || setup_database(content.clone()),
                |(db, source)| {
                    let parsed = parse_file(&db, source);
                    black_box(parsed.functions(&db).len());
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    
    // Symbol indexing scalability
    for size in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(BenchmarkId::new("symbols", size), size, |b, &size| {
            let content = setup_source_file(size);
            b.iter_batched(
                || setup_database(content.clone()),
                |(db, source)| {
                    let symbols = file_symbols(&db, source);
                    black_box(symbols.all_symbols(&db).len());
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    
    group.finish();
}

/// Compare cold vs incremental performance
fn bench_cold_vs_incremental(c: &mut Criterion) {
    let mut group = c.benchmark_group("salsa_cold_vs_incremental");
    group.measurement_time(Duration::from_secs(10));
    
    let sizes = vec![10, 100, 1000];
    
    for size in &sizes {
        // Cold parse
        group.bench_with_input(
            BenchmarkId::new("cold", size),
            size,
            |b, &size| {
                b.iter_batched(
                    || setup_source_file(size),
                    |content| {
                        let (db, source) = setup_database(content);
                        let parsed = parse_file(&db, source);
                        black_box(parsed.functions(&db).len());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
        
        // Incremental (after one function change)
        group.bench_with_input(
            BenchmarkId::new("incremental", size),
            size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let content = setup_source_file(size);
                        let (db, source) = setup_database(content);
                        // Pre-warm
                        let _ = parse_file(&db, source);
                        (db, source)
                    },
                    |(mut db, source)| {
                        // Change one function
                        let old_content = source.content(&db);
                        let new_content = old_content.replace(
                            "func Func5()",
                            "func Func5Modified()"
                        );
                        source.set_content(&mut db).to(new_content);
                        
                        // Re-parse incrementally
                        let parsed = parse_file(&db, source);
                        black_box(parsed.functions(&db).len());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

/// Memory efficiency benchmark
fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("salsa_memory");
    
    group.bench_function("multiple_revisions", |b| {
        let content = setup_source_file(100);
        let (mut db, source) = setup_database(content);
        
        b.iter(|| {
            for i in 0..10 {
                let old_content = source.content(&db);
                let new_content = format!("{}\n// rev {}", old_content, i);
                source.set_content(&mut db).to(new_content);
                
                // Parse each revision
                let _ = parse_file(&db, source);
            }
            black_box(source.version(&db));
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_cold_parse,
    bench_incremental_reparse,
    bench_query_caching,
    bench_lsp_response,
    bench_scalability,
    bench_cold_vs_incremental,
    bench_memory_efficiency
);
criterion_main!(benches);
