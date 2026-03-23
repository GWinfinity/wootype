//! Core functionality tests for Phase 1

use std::sync::Arc;

/// Test TypeUniverse creation and basic operations
#[test]
fn test_type_universe_creation() {
    use wooftype::core::TypeUniverse;
    
    let universe = TypeUniverse::new();
    
    // Universe should have primitive types bootstrapped
    assert!(universe.type_count() > 0, "Universe should have primitive types");
    println!("✓ TypeUniverse created with {} types", universe.type_count());
}

/// Test query engine creation
#[test]
fn test_query_engine_creation() {
    use wooftype::core::TypeUniverse;
    use wooftype::query::QueryEngine;
    
    let universe = Arc::new(TypeUniverse::new());
    let engine = QueryEngine::new(universe);
    
    // Should be able to query primitives
    let result = engine.get_type(wooftype::core::TypeId(1));
    println!("✓ QueryEngine created, primitive lookup: {:?}", result.is_some());
}

/// Test symbol interning
#[test]
fn test_symbol_interning() {
    use wooftype::core::symbol::SymbolTable;
    
    let table = SymbolTable::new();
    
    let id1 = table.intern("test_symbol");
    let id2 = table.intern("test_symbol");
    
    assert_eq!(id1, id2, "Same symbol should have same ID");
    println!("✓ Symbol interning works correctly");
}

/// Test type fingerprint
#[test]
fn test_type_fingerprint() {
    use wooftype::core::types::{TypeFingerprint, PrimitiveType};
    
    let fp1 = PrimitiveType::Int.fingerprint();
    let fp2 = PrimitiveType::Int.fingerprint();
    let fp3 = PrimitiveType::String.fingerprint();
    
    assert_eq!(fp1, fp2, "Same type should have same fingerprint");
    assert_ne!(fp1, fp3, "Different types should have different fingerprints");
    println!("✓ Type fingerprinting works correctly");
}

/// Test cache operations
#[test]
fn test_query_cache() {
    use wooftype::query::cache::QueryCache;
    
    let cache = QueryCache::<String, i32>::new(100);
    
    cache.insert("key1".to_string(), 42);
    
    let result = cache.get(&"key1".to_string());
    assert_eq!(result, Some(42));
    
    let result = cache.get(&"missing".to_string());
    assert_eq!(result, None);
    
    println!("✓ Query cache works correctly");
}

/// Test error collection
#[test]
fn test_error_collection() {
    use wooftype::validate::error::{ErrorCollection, SoftError, ErrorSeverity};
    
    let mut collection = ErrorCollection::new();
    
    collection.add_soft_error(SoftError::new("Test error")
        .with_severity(ErrorSeverity::Warning));
    
    assert!(collection.has_soft_errors());
    assert_eq!(collection.len(), 1);
    
    println!("✓ Error collection works correctly");
}

/// Run all tests
fn main() {
    println!("\n========================================");
    println!("   Wooftype Phase 1 Core Tests");
    println!("========================================\n");
    
    test_type_universe_creation();
    test_query_engine_creation();
    test_symbol_interning();
    test_type_fingerprint();
    test_query_cache();
    test_error_collection();
    
    println!("\n========================================");
    println!("   All tests passed! ✓");
    println!("========================================\n");
}
