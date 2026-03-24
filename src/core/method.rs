//! Method set computation for Go types
//!
//! Handles method set calculation for:
//! - Named types (both value and pointer receivers)
//! - Interface types
//! - Embedded field promotion
//! - Generic type instantiation

use super::{Type, TypeId, TypeKind, TypeUniverse};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// A method signature
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Method {
    pub name: Arc<str>,
    pub sig: TypeId, // Function signature type
    pub recv: Option<Receiver>,
}

/// Receiver type for methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Receiver {
    Value,
    Pointer,
}

impl Receiver {
    /// Check if receiver can call a method with the given receiver type
    pub fn can_call(&self, method_recv: Receiver) -> bool {
        match (self, method_recv) {
            (Receiver::Pointer, Receiver::Pointer) => true,
            (Receiver::Pointer, Receiver::Value) => true, // pointer can call value receivers
            (Receiver::Value, Receiver::Value) => true,
            (Receiver::Value, Receiver::Pointer) => false, // value cannot call pointer receivers
        }
    }
}

/// Method set for a type
#[derive(Debug, Clone, Default)]
pub struct MethodSet {
    /// Methods indexed by name
    methods: BTreeMap<Arc<str>, Method>,
}

impl MethodSet {
    pub fn new() -> Self {
        Self {
            methods: BTreeMap::new(),
        }
    }

    /// Add a method to the set
    pub fn add(&mut self, method: Method) {
        self.methods.insert(method.name.clone(), method);
    }

    /// Lookup a method by name
    pub fn lookup(&self, name: &str) -> Option<&Method> {
        self.methods.get(name)
    }

    /// Check if method set contains a method
    pub fn contains(&self, name: &str) -> bool {
        self.methods.contains_key(name)
    }

    /// Get all method names
    pub fn names(&self) -> impl Iterator<Item = &Arc<str>> {
        self.methods.keys()
    }

    /// Get all methods
    pub fn methods(&self) -> impl Iterator<Item = &Method> {
        self.methods.values()
    }

    /// Union with another method set (for interface composition)
    pub fn union(&mut self, other: &MethodSet) {
        for (name, method) in &other.methods {
            if !self.methods.contains_key(name) {
                self.methods.insert(name.clone(), method.clone());
            }
        }
    }

    /// Check if this method set implements another (for interface satisfaction)
    pub fn implements(&self, interface: &MethodSet) -> bool {
        for (name, iface_method) in &interface.methods {
            if let Some(self_method) = self.methods.get(name) {
                // Method must have compatible signature
                // For now, we just check name; full signature check would require type comparison
                if self_method.sig != iface_method.sig {
                    // In real implementation: check structural compatibility
                    // For now, assume different type IDs mean different signatures
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    /// Number of methods in the set
    pub fn len(&self) -> usize {
        self.methods.len()
    }

    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }
}

/// Compute method set for a type
pub fn compute_method_set(
    type_id: TypeId,
    receiver: Receiver,
    universe: &TypeUniverse,
) -> MethodSet {
    if let Some(typ) = universe.get_type(type_id) {
        match &typ.kind {
            TypeKind::Named { underlying, .. } => {
                // For named types, get methods from the universe's method table
                // and also include methods from the underlying type
                let mut set = universe.get_methods_for_type(type_id, receiver);

                // Also get methods from underlying type for value receivers
                if receiver == Receiver::Value {
                    let underlying_set = compute_method_set(*underlying, Receiver::Value, universe);
                    for method in underlying_set.methods() {
                        if !set.contains(&method.name) {
                            set.add(method.clone());
                        }
                    }
                }

                set
            }
            TypeKind::Pointer { elem } => {
                // Pointer has method set of element with pointer receiver
                compute_method_set(*elem, Receiver::Pointer, universe)
            }
            TypeKind::Interface {
                methods, embedded, ..
            } => {
                let mut set = MethodSet::new();

                // Add direct methods
                for im in methods {
                    set.add(Method {
                        name: im.name.clone(),
                        sig: im.sig,
                        recv: None, // Interface methods don't have explicit receiver
                    });
                }

                // Add methods from embedded interfaces
                for embed_id in embedded {
                    let embed_set = compute_method_set(*embed_id, Receiver::Value, universe);
                    set.union(&embed_set);
                }

                set
            }
            TypeKind::Struct { fields } => {
                // Struct methods come from embedded fields (promotion)
                let mut set = MethodSet::new();
                let mut seen_types: HashSet<TypeId> = HashSet::new();

                for field in fields {
                    if field.embedded {
                        // Promote methods from embedded field
                        if seen_types.insert(field.typ) {
                            let field_set = compute_method_set(field.typ, receiver, universe);
                            for method in field_set.methods() {
                                // Only promote if not shadowed by existing method
                                if !set.contains(&method.name) {
                                    set.add(method.clone());
                                }
                            }
                        }
                    }
                }

                set
            }
            _ => MethodSet::new(),
        }
    } else {
        MethodSet::new()
    }
}

/// Compute method set for a type considering both value and pointer receivers
pub fn compute_complete_method_set(type_id: TypeId, universe: &TypeUniverse) -> MethodSet {
    let mut set = compute_method_set(type_id, Receiver::Value, universe);
    let ptr_set = compute_method_set(type_id, Receiver::Pointer, universe);
    set.union(&ptr_set);
    set
}

/// Check if a type satisfies an interface
pub fn implements_interface(
    concrete_type: TypeId,
    interface_type: TypeId,
    universe: &TypeUniverse,
) -> bool {
    if let Some(iface) = universe.get_type(interface_type) {
        if !iface.flags.contains(super::TypeFlags::INTERFACE) {
            return false;
        }

        let concrete_methods = compute_complete_method_set(concrete_type, universe);
        let iface_methods = compute_method_set(interface_type, Receiver::Value, universe);

        concrete_methods.implements(&iface_methods)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PrimitiveType, Type};

    #[test]
    fn test_empty_method_set() {
        let set = MethodSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert!(set.lookup("foo").is_none());
    }

    #[test]
    fn test_method_set_add_and_lookup() {
        let mut set = MethodSet::new();
        let method = Method {
            name: Arc::from("foo"),
            sig: TypeId(1),
            recv: Some(Receiver::Value),
        };

        set.add(method.clone());
        assert_eq!(set.len(), 1);
        assert!(set.contains("foo"));
        assert!(set.lookup("foo").is_some());
    }

    #[test]
    fn test_receiver_can_call() {
        assert!(Receiver::Value.can_call(Receiver::Value));
        assert!(!Receiver::Value.can_call(Receiver::Pointer));
        assert!(Receiver::Pointer.can_call(Receiver::Value));
        assert!(Receiver::Pointer.can_call(Receiver::Pointer));
    }
}
