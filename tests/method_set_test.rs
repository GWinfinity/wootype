//! Tests for method set computation and interface satisfaction

use std::sync::Arc;
use wootype::core::{
    method::{compute_method_set, Method, MethodSet, Receiver},
    Type, TypeId, TypeKind, TypeUniverse,
};

#[test]
fn test_method_set_basic() {
    let universe = Arc::new(TypeUniverse::new());

    // Create a simple type
    let type_id = TypeId(1000);
    let typ = Type::new(type_id, TypeKind::Struct { fields: vec![] });
    universe.insert_type(type_id, Arc::new(typ));

    // Register some methods
    let method1 = Method {
        name: Arc::from("Method1"),
        sig: TypeId(100),
        recv: Some(Receiver::Value),
    };
    let method2 = Method {
        name: Arc::from("Method2"),
        sig: TypeId(101),
        recv: Some(Receiver::Pointer),
    };

    universe.register_method(type_id, Receiver::Value, method1.clone());
    universe.register_method(type_id, Receiver::Pointer, method2.clone());

    // Test value method set
    let value_set = universe.get_methods_for_type(type_id, Receiver::Value);
    assert!(value_set.contains("Method1"));
    assert!(!value_set.contains("Method2")); // Pointer methods not in value set

    // Test pointer method set
    let ptr_set = universe.get_methods_for_type(type_id, Receiver::Pointer);
    assert!(!ptr_set.contains("Method1"));
    assert!(ptr_set.contains("Method2"));

    // Test complete method set
    let complete = universe.get_complete_method_set(type_id);
    assert!(complete.contains("Method1"));
    assert!(complete.contains("Method2"));
    assert_eq!(complete.len(), 2);
}

#[test]
fn test_receiver_can_call() {
    // Value receiver can only call value methods
    assert!(Receiver::Value.can_call(Receiver::Value));
    assert!(!Receiver::Value.can_call(Receiver::Pointer));

    // Pointer receiver can call both
    assert!(Receiver::Pointer.can_call(Receiver::Value));
    assert!(Receiver::Pointer.can_call(Receiver::Pointer));
}

#[test]
fn test_method_set_union() {
    let mut set1 = MethodSet::new();
    let mut set2 = MethodSet::new();

    set1.add(Method {
        name: Arc::from("A"),
        sig: TypeId(1),
        recv: None,
    });
    set2.add(Method {
        name: Arc::from("B"),
        sig: TypeId(2),
        recv: None,
    });

    set1.union(&set2);

    assert!(set1.contains("A"));
    assert!(set1.contains("B"));
    assert_eq!(set1.len(), 2);
}

#[test]
fn test_method_set_implements() {
    let mut concrete = MethodSet::new();
    let mut interface = MethodSet::new();

    // Concrete type has methods A and B
    concrete.add(Method {
        name: Arc::from("A"),
        sig: TypeId(1),
        recv: None,
    });
    concrete.add(Method {
        name: Arc::from("B"),
        sig: TypeId(2),
        recv: None,
    });

    // Interface only requires method A
    interface.add(Method {
        name: Arc::from("A"),
        sig: TypeId(1),
        recv: None,
    });

    // Concrete should implement interface
    assert!(concrete.implements(&interface));

    // Interface should not implement concrete (missing B)
    assert!(!interface.implements(&concrete));
}

#[test]
fn test_interface_method_set() {
    let universe = Arc::new(TypeUniverse::new());

    // Create interface type with methods
    let interface_id = TypeId(2000);
    let interface_kind = TypeKind::Interface {
        methods: vec![
            wootype::core::types::InterfaceMethod {
                name: Arc::from("Read"),
                sig: TypeId(200),
            },
            wootype::core::types::InterfaceMethod {
                name: Arc::from("Write"),
                sig: TypeId(201),
            },
        ],
        embedded: vec![],
        implicit: false,
    };
    let interface = Type::new(interface_id, interface_kind);
    universe.insert_type(interface_id, Arc::new(interface));

    // Compute method set
    let method_set = compute_method_set(interface_id, Receiver::Value, &universe);

    assert!(method_set.contains("Read"));
    assert!(method_set.contains("Write"));
    assert_eq!(method_set.len(), 2);
}

#[test]
fn test_type_constraint_any() {
    use wootype::core::types::TypeConstraint;

    let universe = TypeUniverse::new();
    let constraint = TypeConstraint::Any;

    // Any constraint should be satisfied by any type
    // int is TypeId 2 (second primitive after bool)
    let int_type = universe.get_type(TypeId(2)).unwrap();
    assert!(constraint.satisfied_by(&int_type, &universe));
}

#[test]
fn test_type_constraint_comparable() {
    use wootype::core::types::TypeConstraint;

    let universe = TypeUniverse::new();
    let constraint = TypeConstraint::Comparable;

    // int is comparable (TypeId 2 - second primitive after bool)
    let int_type = universe.get_type(TypeId(2)).unwrap();
    assert!(constraint.satisfied_by(&int_type, &universe));

    // float is ordered but not comparable
    // Float32 is at index 13 in bootstrap_primitives (TypeId 14)
    let float_type = universe.get_type(TypeId(14)).unwrap();
    assert!(!constraint.satisfied_by(&float_type, &universe));
}

#[test]
fn test_type_constraint_ordered() {
    use wootype::core::types::TypeConstraint;

    let universe = TypeUniverse::new();
    let constraint = TypeConstraint::Ordered;

    // int is ordered (TypeId 2 - second primitive after bool)
    let int_type = universe.get_type(TypeId(2)).unwrap();
    assert!(constraint.satisfied_by(&int_type, &universe));
}

#[test]
fn test_embedded_field_name_extraction() {
    use wootype::core::types::StructField;

    let universe = Arc::new(TypeUniverse::new());

    // Create a named type
    let inner_id = TypeId(3000);
    let inner = Type::new(
        inner_id,
        TypeKind::Named {
            pkg_path: Arc::from(""),
            name: Arc::from("Inner"),
            underlying: inner_id,
        },
    );
    universe.insert_type(inner_id, Arc::new(inner));

    // Create struct with embedded field
    let struct_id = TypeId(3001);
    let struct_kind = TypeKind::Struct {
        fields: vec![StructField {
            name: Arc::from("Inner"), // Implicit name from type
            typ: inner_id,
            embedded: true,
            tag: None,
        }],
    };
    let struct_type = Type::new(struct_id, struct_kind);
    universe.insert_type(struct_id, Arc::new(struct_type));

    // Verify the field name was correctly set
    if let Some(typ) = universe.get_type(struct_id) {
        if let TypeKind::Struct { fields } = &typ.kind {
            assert_eq!(fields[0].name.as_ref(), "Inner");
            assert!(fields[0].embedded);
        } else {
            panic!("Expected struct kind");
        }
    } else {
        panic!("Type not found");
    }
}
