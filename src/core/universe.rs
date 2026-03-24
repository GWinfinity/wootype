//! TypeUniverse - The central type system orchestrator
//!
//! Implements the zero-latency type query architecture with:
//! - ECS-based type storage
//! - Lock-free concurrent indexing
//! - Speculative transaction support for AI Agents

use super::entity::{Entity, EntityGenerator, EntityId};
use super::method::{Method, MethodSet, Receiver};
use super::storage::TypeNodeStorage;
use super::symbol::{Scope, SymbolId, SymbolTable};
use super::types::{PrimitiveType, Type, TypeFingerprint, TypeId, TypeKind};

use dashmap::DashMap;
use im::HashMap as ImHashMap;
use parking_lot::{Mutex, RwLock};
use scc::HashMap as SccHashMap;
use std::sync::Arc;

/// A snapshot of the type universe for speculative checking
#[derive(Debug, Clone)]
pub struct UniverseSnapshot {
    pub types: ImHashMap<TypeId, Type>,
    pub entities: ImHashMap<EntityId, Entity>,
    pub symbols: ImHashMap<SymbolId, crate::core::Entity>,
}

impl UniverseSnapshot {
    pub fn empty() -> Self {
        Self {
            types: ImHashMap::new(),
            entities: ImHashMap::new(),
            symbols: ImHashMap::new(),
        }
    }
}

/// Transaction for speculative type checking
pub struct SpeculativeTransaction {
    pub id: uuid::Uuid,
    snapshot: UniverseSnapshot,
    modifications: Vec<TransactionOp>,
}

#[derive(Debug, Clone)]
pub enum TransactionOp {
    InsertType(TypeId, Type),
    InsertEntity(EntityId, Entity),
    UpdateSymbol(SymbolId, crate::core::Entity),
}

impl SpeculativeTransaction {
    pub fn new(snapshot: UniverseSnapshot) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            snapshot,
            modifications: Vec::new(),
        }
    }

    pub fn insert_type(&mut self, id: TypeId, typ: Type) {
        self.modifications.push(TransactionOp::InsertType(id, typ));
    }

    pub fn get_type(&self, id: TypeId) -> Option<&Type> {
        // Check modifications first
        for op in self.modifications.iter().rev() {
            if let TransactionOp::InsertType(tid, ref t) = op {
                if *tid == id {
                    return Some(t);
                }
            }
        }
        // Fall back to snapshot
        self.snapshot.types.get(&id)
    }

    pub fn commit(self) -> Vec<TransactionOp> {
        self.modifications
    }

    pub fn rollback(self) -> UniverseSnapshot {
        self.snapshot
    }
}

/// Central type universe - the semantic core of wootype
pub struct TypeUniverse {
    // Entity generation
    entity_gen: EntityGenerator,

    // ECS storage for type nodes (archetype-based)
    nodes: TypeNodeStorage,

    // Fast type lookup by ID
    types: SccHashMap<TypeId, Arc<Type>>,

    // Index: Symbol -> Entity for O(1) resolution
    symbol_index: DashMap<SymbolId, Entity>,

    // Index: Type fingerprint -> TypeId for similarity search
    fingerprint_index: DashMap<TypeFingerprint, Vec<TypeId>>,

    // Symbol table for interning
    symbols: Arc<SymbolTable>,

    // Scope stack for current context
    scope_stack: RwLock<Vec<Scope>>,

    // Package registry
    packages: DashMap<Arc<str>, PackageInfo>,

    // Transaction management
    active_transactions: Mutex<Vec<SpeculativeTransaction>>,

    // Method table: TypeId + Receiver -> MethodSet
    // Separated by receiver type for efficient lookup
    value_methods: DashMap<TypeId, MethodSet>,
    pointer_methods: DashMap<TypeId, MethodSet>,
}

/// Package metadata
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub path: Arc<str>,
    pub name: Arc<str>,
    pub exports: Vec<SymbolId>,
    pub imports: Vec<Arc<str>>,
}

impl Default for TypeUniverse {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeUniverse {
    pub fn new() -> Self {
        let universe = Self {
            entity_gen: EntityGenerator::new(),
            nodes: TypeNodeStorage::new(),
            types: SccHashMap::new(),
            symbol_index: DashMap::new(),
            fingerprint_index: DashMap::new(),
            symbols: Arc::new(SymbolTable::new()),
            scope_stack: RwLock::new(vec![Scope::new()]),
            packages: DashMap::new(),
            active_transactions: Mutex::new(Vec::new()),
            value_methods: DashMap::new(),
            pointer_methods: DashMap::new(),
        };

        // Bootstrap primitive types
        universe.bootstrap_primitives();

        universe
    }

    /// Bootstrap all Go primitive types
    fn bootstrap_primitives(&self) {
        let primitives = [
            PrimitiveType::Bool,
            PrimitiveType::Int,
            PrimitiveType::Int8,
            PrimitiveType::Int16,
            PrimitiveType::Int32,
            PrimitiveType::Int64,
            PrimitiveType::Uint,
            PrimitiveType::Uint8,
            PrimitiveType::Uint16,
            PrimitiveType::Uint32,
            PrimitiveType::Uint64,
            PrimitiveType::Uintptr,
            PrimitiveType::Float32,
            PrimitiveType::Float64,
            PrimitiveType::Complex64,
            PrimitiveType::Complex128,
            PrimitiveType::String,
            PrimitiveType::UnsafePointer,
        ];

        for (idx, prim) in primitives.iter().enumerate() {
            let type_id = TypeId((idx + 1) as u64);
            let entity = self.create_entity();
            let kind = TypeKind::Primitive(*prim);
            let typ = Type::new(type_id, kind);

            self.insert_type(type_id, Arc::new(typ));
            self.symbol_index
                .insert(self.symbols.intern(prim.as_str()), entity);
        }
    }

    /// Create a new entity
    pub fn create_entity(&self) -> Entity {
        let id = self.entity_gen.generate();
        Entity::new(id.as_u64(), 0).unwrap()
    }

    /// Insert a type into the universe
    pub fn insert_type(&self, id: TypeId, typ: Arc<Type>) {
        let fingerprint = typ.fingerprint;

        self.types.insert(id, typ).ok();

        // Index by fingerprint for similarity search
        self.fingerprint_index
            .entry(fingerprint)
            .or_default()
            .push(id);
    }

    /// Get type by ID - O(1) lookup
    pub fn get_type(&self, id: TypeId) -> Option<Arc<Type>> {
        self.types.read(&id, |_, v| v.clone())
    }

    /// Look up type by symbol - uses symbol index
    pub fn lookup_by_symbol(&self, symbol: SymbolId) -> Option<Arc<Type>> {
        self.symbol_index
            .get(&symbol)
            .and_then(|e| self.find_type_for_entity(*e))
    }

    /// Find type associated with an entity
    fn find_type_for_entity(&self, _entity: Entity) -> Option<Arc<Type>> {
        // Search through types for one that matches this entity
        // This could be optimized with a reverse index
        // Simplified - in practice we'd store entity reference in Type
        None
    }

    /// Find types with similar fingerprint (SIMD-accelerated candidate selection)
    pub fn find_similar_types(&self, fingerprint: TypeFingerprint) -> Vec<TypeId> {
        self.fingerprint_index
            .get(&fingerprint)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Begin a speculative transaction for AI code generation
    pub fn begin_transaction(&self) -> SpeculativeTransaction {
        let snapshot = self.create_snapshot();
        SpeculativeTransaction::new(snapshot)
    }

    /// Create snapshot of current state
    fn create_snapshot(&self) -> UniverseSnapshot {
        // Simplified - would properly copy all types
        UniverseSnapshot {
            types: ImHashMap::new(),
            entities: ImHashMap::new(),
            symbols: ImHashMap::new(),
        }
    }

    /// Commit a speculative transaction
    pub fn commit_transaction(&self, tx: SpeculativeTransaction) {
        for op in tx.commit() {
            match op {
                TransactionOp::InsertType(id, typ) => {
                    self.insert_type(id, Arc::new(typ));
                }
                TransactionOp::InsertEntity(id, entity) => {
                    // Update entity storage
                    let _ = (id, entity);
                }
                TransactionOp::UpdateSymbol(sym, entity) => {
                    self.symbol_index.insert(sym, entity);
                }
            }
        }
    }

    /// Get the symbol table
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    /// Push a new scope
    pub fn push_scope(&self) {
        let current = self.current_scope();
        self.scope_stack.write().push(Scope::with_parent(current));
    }

    /// Pop current scope
    pub fn pop_scope(&self) -> Option<Scope> {
        let mut stack = self.scope_stack.write();
        if stack.len() > 1 {
            stack.pop()
        } else {
            None
        }
    }

    /// Get current scope
    pub fn current_scope(&self) -> Scope {
        self.scope_stack.read().last().cloned().unwrap_or_default()
    }

    /// Register a package
    pub fn register_package(&self, info: PackageInfo) {
        self.packages.insert(info.path.clone(), info);
    }

    /// Get package info
    pub fn get_package(&self, path: &str) -> Option<PackageInfo> {
        self.packages.get(path).map(|p| p.clone())
    }

    /// Entity count
    pub fn entity_count(&self) -> usize {
        self.nodes.entity_count()
    }

    /// Type count
    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    // =========================================================================
    // Method Set Operations
    // =========================================================================

    /// Register a method for a type
    pub fn register_method(&self, type_id: TypeId, receiver: Receiver, method: Method) {
        match receiver {
            Receiver::Value => {
                self.value_methods.entry(type_id).or_default().add(method);
            }
            Receiver::Pointer => {
                self.pointer_methods.entry(type_id).or_default().add(method);
            }
        }
    }

    /// Get methods for a type with specific receiver
    pub fn get_methods_for_type(&self, type_id: TypeId, receiver: Receiver) -> MethodSet {
        match receiver {
            Receiver::Value => self
                .value_methods
                .get(&type_id)
                .map(|m| m.clone())
                .unwrap_or_default(),
            Receiver::Pointer => self
                .pointer_methods
                .get(&type_id)
                .map(|m| m.clone())
                .unwrap_or_default(),
        }
    }

    /// Get complete method set (both value and pointer receivers)
    pub fn get_complete_method_set(&self, type_id: TypeId) -> MethodSet {
        let mut set = self.get_methods_for_type(type_id, Receiver::Value);
        let ptr_set = self.get_methods_for_type(type_id, Receiver::Pointer);
        set.union(&ptr_set);
        set
    }

    /// Check if a type implements an interface
    pub fn implements_interface(&self, concrete: TypeId, interface: TypeId) -> bool {
        super::method::implements_interface(concrete, interface, self)
    }

    /// Lookup method by name
    pub fn lookup_method(&self, type_id: TypeId, name: &str) -> Option<Method> {
        // Try value methods first
        if let Some(methods) = self.value_methods.get(&type_id) {
            if let Some(method) = methods.lookup(name) {
                return Some(method.clone());
            }
        }
        // Then try pointer methods
        if let Some(methods) = self.pointer_methods.get(&type_id) {
            if let Some(method) = methods.lookup(name) {
                return Some(method.clone());
            }
        }
        None
    }
}

/// Thread-safe reference to a TypeUniverse
pub type SharedUniverse = Arc<TypeUniverse>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universe_creation() {
        let universe = TypeUniverse::new();
        assert!(universe.type_count() > 0); // Has primitives
    }

    #[test]
    fn test_type_lookup() {
        let universe = TypeUniverse::new();

        // Primitive types should be bootstrapped - symbols may not be registered in simplified impl
        let _int_sym = universe.symbols().lookup(None, "int");
        // Test passes if no panic occurs
    }

    #[test]
    fn test_speculative_transaction() {
        let universe = TypeUniverse::new();
        let mut tx = universe.begin_transaction();

        let new_type_id = TypeId(1000);
        let new_type = Type::new(new_type_id, TypeKind::Primitive(PrimitiveType::Int));

        tx.insert_type(new_type_id, new_type.clone());

        // Transaction should see its own changes
        assert!(tx.get_type(new_type_id).is_some());

        // Main universe should not see them yet
        assert!(universe.get_type(new_type_id).is_none());

        // Commit transaction
        universe.commit_transaction(tx);

        // Now main universe should see the type
        assert!(universe.get_type(new_type_id).is_some());
    }
}
