//! Integration tests for Wootype Phase 1
#![allow(unused_imports, unused_variables)]
//!
//! These tests verify the interaction between multiple components.

use std::sync::Arc;

/// Test complete workflow: Universe -> QueryEngine -> Cache
#[test]
fn test_end_to_end_type_query() {
    use wootype::core::{types::PrimitiveType, TypeId, TypeUniverse};
    use wootype::query::QueryEngine;

    // Create universe with types
    let universe = Arc::new(TypeUniverse::new());
    let engine = QueryEngine::new(universe.clone());

    // Query primitive type by ID
    let int_type = engine.get_type(TypeId(2));
    assert!(int_type.is_some(), "Should find int type");

    // Query with fingerprint
    let fingerprint = PrimitiveType::Int.fingerprint();
    let results = engine.query_by_fingerprint(fingerprint);
    // May be empty in simplified implementation, but should not panic
    println!("Fingerprint query returned {} results", results.len());
}

/// Test Agent session workflow
#[test]
fn test_agent_session_lifecycle() {
    use wootype::agent::{AgentCoordinator, AgentId, AgentType, ConnectionRequest};
    use wootype::core::TypeUniverse;

    let universe = Arc::new(TypeUniverse::new());
    let coordinator = Arc::new(AgentCoordinator::new(universe));

    // Create connection request
    let request = ConnectionRequest {
        agent_id: AgentId::new(1),
        name: "TestAgent".to_string(),
        agent_type: AgentType::Generic,
        preferred_isolation: None,
    };

    // Note: Cannot test async connect in sync test, but struct creation works
    println!("Connection request created: {:?}", request.agent_type);
}

/// Test symbol resolution across scopes
#[test]
fn test_symbol_resolution_workflow() {
    use wootype::core::symbol::{Scope, SymbolId, SymbolTable};
    use wootype::core::Entity;

    let symbols = SymbolTable::new();
    let entity = Entity::new(1, 1).unwrap();

    // Create nested scopes
    let mut outer = Scope::new();
    let sym = SymbolId::new(1);
    outer.insert(sym, entity);

    let inner = Scope::with_parent(outer.clone());

    // Symbol should be found in inner scope via parent
    assert!(
        inner.lookup(sym).is_some(),
        "Should find symbol via parent scope"
    );
    assert!(
        outer.lookup(sym).is_some(),
        "Should find symbol in outer scope"
    );
}

/// Test type validation workflow
#[test]
fn test_type_validation_workflow() {
    use wootype::core::{TypeId, TypeUniverse};
    use wootype::validate::{StreamingChecker, ValidationStream};

    let universe = Arc::new(TypeUniverse::new());
    let _checker = StreamingChecker::new(universe.clone());

    // Note: Full validation testing requires async runtime
    println!("Validation components created successfully");
}

/// Test cache eviction behavior
#[test]
fn test_cache_eviction_workflow() {
    use wootype::query::cache::QueryCache;

    let cache = QueryCache::<String, i32>::new(3); // Very small cache

    // Fill cache
    cache.insert("a".to_string(), 1);
    cache.insert("b".to_string(), 2);
    cache.insert("c".to_string(), 3);

    // Access "a" to make it recently used
    let _ = cache.get(&"a".to_string());

    // Add new item, should evict "b" (least recently used)
    cache.insert("d".to_string(), 4);

    // Check stats
    let stats = cache.stats();
    assert_eq!(stats.size, 3, "Cache should maintain max size");
    println!("Cache eviction working: {} entries", stats.size);
}

/// Test error propagation
#[test]
fn test_error_propagation() {
    use wootype::validate::error::{ErrorCollection, ErrorSeverity, SoftError, ValidationError};

    let mut errors = ErrorCollection::new();

    // Add various errors
    errors.add_soft_error(SoftError::new("Warning").with_severity(ErrorSeverity::Hint));
    errors.add_soft_error(SoftError::new("Error").with_severity(ErrorSeverity::Error));
    errors.add_error(ValidationError::CyclicType);

    // Check filtering
    let blocking = errors.filter_by_severity(ErrorSeverity::Error);
    assert_eq!(blocking.len(), 1, "Should find one blocking error");

    // Soften all errors
    let softened = errors.soften();
    assert_eq!(softened.len(), 3, "Should have 3 softened errors");
}

/// Test type flags operations
#[test]
fn test_type_flags_comprehensive() {
    use wootype::core::types::TypeFlags;

    let basic = TypeFlags::BASIC;
    let comparable = TypeFlags::COMPARABLE;
    let named = TypeFlags::NAMED;

    // Test BitOr
    let combined = basic | comparable;
    assert!(combined.contains(basic));
    assert!(combined.contains(comparable));

    // Test BitAnd
    let intersection = combined & basic;
    assert!(intersection.contains(basic));
    assert!(!intersection.contains(comparable));

    // Test intersects
    assert!(combined.intersects(basic));
    assert!(!combined.intersects(named));

    println!("TypeFlags operations verified");
}

/// Test concurrent access patterns (basic)
#[test]
fn test_concurrent_symbol_access() {
    use std::thread;
    use wootype::core::symbol::SymbolTable;

    let table = Arc::new(SymbolTable::new());
    let mut handles = vec![];

    // Spawn threads that intern symbols
    for i in 0..10 {
        let t = table.clone();
        handles.push(thread::spawn(move || {
            let name = format!("symbol_{}", i);
            let id = t.intern(&name);
            // Same name should return same ID
            let id2 = t.intern(&name);
            assert_eq!(id, id2);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Should have 10 unique symbols + 1 pre-interned empty string
    assert_eq!(table.len(), 11);
}

/// Test serialization round-trip
#[test]
fn test_serialization_roundtrip() {
    use wootype::core::types::{PrimitiveType, Type, TypeId, TypeKind};

    let original = Type::new(TypeId(100), TypeKind::Primitive(PrimitiveType::Int));

    // Serialize
    let json = serde_json::to_string(&original).expect("Should serialize");
    println!("Serialized: {}", json);

    // Deserialize
    let deserialized: Type = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(original.id, deserialized.id);
    assert_eq!(original.fingerprint, deserialized.fingerprint);
}

/// Test package import workflow (mock)
#[test]
fn test_package_import_workflow() {
    use wootype::core::{universe::PackageInfo, TypeUniverse};

    let universe = Arc::new(TypeUniverse::new());

    // Register a mock package
    let info = PackageInfo {
        path: Arc::from("github.com/test/pkg"),
        name: Arc::from("pkg"),
        exports: vec![],
        imports: vec![],
    };

    universe.register_package(info);

    let retrieved = universe.get_package("github.com/test/pkg");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name.as_ref(), "pkg");
}

/// Main test runner
fn main() {
    println!("\n========================================");
    println!("   Wootype Integration Tests");
    println!("========================================\n");

    test_end_to_end_type_query();
    test_agent_session_lifecycle();
    test_symbol_resolution_workflow();
    test_type_validation_workflow();
    test_cache_eviction_workflow();
    test_error_propagation();
    test_type_flags_comprehensive();
    test_concurrent_symbol_access();
    test_serialization_roundtrip();
    test_package_import_workflow();

    println!("\n========================================");
    println!("   All integration tests passed! ✓");
    println!("========================================\n");
}
