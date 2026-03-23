//! Basic usage example for Wooftype Phase 1
//! 
//! This example demonstrates the core functionality of the type system.

use std::sync::Arc;

fn main() {
    println!("\n========================================");
    println!("   Wooftype Phase 1 - Basic Usage Demo");
    println!("========================================\n");
    
    // 1. Create Type Universe
    println!("1. Creating Type Universe...");
    let universe = Arc::new(wooftype::core::TypeUniverse::new());
    println!("   ✓ Universe created with {} primitive types\n", universe.type_count());
    
    // 2. Create Query Engine
    println!("2. Creating Query Engine...");
    let engine = wooftype::query::QueryEngine::new(universe.clone());
    println!("   ✓ Query Engine ready\n");
    
    // 3. Query primitive types
    println!("3. Querying primitive types...");
    for i in 1..=5 {
        let type_id = wooftype::core::TypeId(i);
        if let Some(typ) = engine.get_type(type_id) {
            println!("   ✓ Type[{}]: {:?}", i, typ.kind);
        }
    }
    println!();
    
    // 4. Test symbol interning
    println!("4. Testing symbol interning...");
    let symbols = wooftype::core::symbol::SymbolTable::new();
    let sym1 = symbols.intern("MyType");
    let sym2 = symbols.intern("MyType");
    let sym3 = symbols.intern("OtherType");
    
    assert_eq!(sym1, sym2, "Same symbol should return same ID");
    assert_ne!(sym1, sym3, "Different symbols should return different IDs");
    println!("   ✓ 'MyType' -> ID {:?}", sym1);
    println!("   ✓ 'OtherType' -> ID {:?}", sym3);
    println!("   ✓ Total symbols: {}\n", symbols.len());
    
    // 5. Test type fingerprints
    println!("5. Testing type fingerprints...");
    use wooftype::core::types::PrimitiveType;
    
    let int_fp = PrimitiveType::Int.fingerprint();
    let int_fp2 = PrimitiveType::Int.fingerprint();
    let string_fp = PrimitiveType::String.fingerprint();
    
    println!("   ✓ Int fingerprint: {:016x}", int_fp.0);
    println!("   ✓ String fingerprint: {:016x}", string_fp.0);
    assert_eq!(int_fp, int_fp2, "Same type, same fingerprint");
    assert_ne!(int_fp, string_fp, "Different types, different fingerprints");
    println!("   ✓ Fingerprint consistency verified\n");
    
    // 6. Test query cache
    println!("6. Testing query cache...");
    use wooftype::query::cache::QueryCache;
    
    let cache = QueryCache::<String, String>::new(100);
    cache.insert("query:int".to_string(), "integer type".to_string());
    cache.insert("query:string".to_string(), "string type".to_string());
    
    if let Some(result) = cache.get(&"query:int".to_string()) {
        println!("   ✓ Cache hit: 'query:int' -> '{}'", result);
    }
    
    let stats = cache.stats();
    println!("   ✓ Cache stats: {} entries, {:.1}% hit rate\n", 
             stats.size, stats.hit_rate * 100.0);
    
    // 7. Test error handling
    println!("7. Testing soft error handling...");
    use wooftype::validate::error::{ErrorCollection, SoftError, ErrorSeverity};
    
    let mut errors = ErrorCollection::new();
    errors.add_soft_error(
        SoftError::new("Type mismatch in expression")
            .with_suggestion("Consider using int64 instead")
            .with_severity(ErrorSeverity::Warning)
    );
    
    println!("   ✓ Soft errors collected: {}", errors.len());
    for error in errors.iter_soft_errors() {
        println!("     - [{}] {}", error.severity.as_str(), error.message);
        if let Some(suggestion) = &error.suggestion {
            println!("       Suggestion: {}", suggestion);
        }
    }
    println!();
    
    println!("========================================");
    println!("   Demo completed successfully! ✓");
    println!("========================================\n");
}
