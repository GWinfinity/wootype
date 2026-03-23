//! Benchmarks for query operations
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;

fn bench_type_lookup(c: &mut Criterion) {
    use wooftype::core::{TypeUniverse, TypeId};
    use wooftype::query::QueryEngine;
    
    let universe = Arc::new(TypeUniverse::new());
    let engine = QueryEngine::new(universe);
    
    c.bench_function("type_lookup_by_id", |b| {
        b.iter(|| {
            let _ = engine.get_type(black_box(TypeId(2)));
        })
    });
}

fn bench_symbol_intern(c: &mut Criterion) {
    use wooftype::core::symbol::SymbolTable;
    
    let table = SymbolTable::new();
    
    c.bench_function("symbol_intern", |b| {
        let mut i = 0;
        b.iter(|| {
            let name = format!("symbol_{}", i);
            let _ = table.intern(&name);
            i += 1;
        })
    });
}

fn bench_cache_operations(c: &mut Criterion) {
    use wooftype::query::cache::QueryCache;
    
    let cache = QueryCache::<String, i32>::new(1000);
    
    // Pre-populate cache
    for i in 0..100 {
        cache.insert(format!("key_{}", i), i);
    }
    
    c.bench_function("cache_get", |b| {
        let mut i = 0;
        b.iter(|| {
            let key = format!("key_{}", i % 100);
            let _ = cache.get(&key);
            i += 1;
        })
    });
    
    c.bench_function("cache_insert", |b| {
        let mut i = 0;
        b.iter(|| {
            let key = format!("new_key_{}", i);
            cache.insert(key, i);
            i += 1;
        })
    });
}

fn bench_fingerprint_calculation(c: &mut Criterion) {
    use wooftype::core::types::PrimitiveType;
    
    c.bench_function("fingerprint_calc", |b| {
        b.iter(|| {
            let _ = PrimitiveType::Int.fingerprint();
        })
    });
}

fn bench_type_flags_ops(c: &mut Criterion) {
    use wooftype::core::types::TypeFlags;
    
    let flags1 = TypeFlags::BASIC | TypeFlags::COMPARABLE;
    let flags2 = TypeFlags::POINTER | TypeFlags::NILABLE;
    
    c.bench_function("type_flags_bitor", |b| {
        b.iter(|| {
            let _ = black_box(flags1) | black_box(flags2);
        })
    });
    
    c.bench_function("type_flags_contains", |b| {
        b.iter(|| {
            let _ = black_box(flags1).contains(TypeFlags::BASIC);
        })
    });
}

criterion_group!(
    benches,
    bench_type_lookup,
    bench_symbol_intern,
    bench_cache_operations,
    bench_fingerprint_calculation,
    bench_type_flags_ops
);
criterion_main!(benches);
