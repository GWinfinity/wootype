//! Symbol table for identifier management
//!
//! Provides fast symbol lookup using string interning

use dashmap::DashMap;
use std::sync::Arc;

/// Interned symbol identifier
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct SymbolId(u32);

impl SymbolId {
    pub const INVALID: SymbolId = SymbolId(0);

    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn index(&self) -> usize {
        self.0 as usize
    }
}

impl Default for SymbolId {
    fn default() -> Self {
        Self::INVALID
    }
}

/// Symbol information
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    #[serde(with = "arc_str")]
    pub name: Arc<str>,
    #[serde(with = "arc_str_opt")]
    pub pkg_path: Option<Arc<str>>,
}

mod arc_str {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(arc: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(arc)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Ok(Arc::from(s))
    }
}

mod arc_str_opt {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(opt: &Option<Arc<str>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt {
            Some(arc) => serializer.serialize_some(arc.as_ref()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Arc<str>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Deserialize::deserialize(deserializer)?;
        Ok(opt.map(|s| Arc::from(s)))
    }
}

/// Thread-safe symbol table with string interning
pub struct SymbolTable {
    symbols: DashMap<SymbolId, Symbol>,
    name_to_id: DashMap<(Option<Arc<str>>, Arc<str>), SymbolId>,
    counter: std::sync::atomic::AtomicU32,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        let table = Self {
            symbols: DashMap::new(),
            name_to_id: DashMap::new(),
            counter: std::sync::atomic::AtomicU32::new(1),
        };

        // Pre-intern common symbols
        table.intern("");

        table
    }

    /// Intern a symbol name, returning its ID
    pub fn intern(&self, name: &str) -> SymbolId {
        self.intern_in_package(None, name)
    }

    /// Intern a symbol in a specific package
    pub fn intern_in_package(&self, pkg_path: Option<Arc<str>>, name: &str) -> SymbolId {
        let name: Arc<str> = name.into();
        let key = (pkg_path.clone(), name.clone());

        // Fast path: already interned
        if let Some(id) = self.name_to_id.get(&key) {
            return *id;
        }

        // Slow path: intern new symbol
        let id = SymbolId(
            self.counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        let symbol = Symbol { id, name, pkg_path };

        self.symbols.insert(id, symbol.clone());
        self.name_to_id.insert(key, id);

        id
    }

    /// Get symbol by ID
    pub fn get(&self, id: SymbolId) -> Option<Symbol> {
        self.symbols.get(&id).map(|s| s.clone())
    }

    /// Look up symbol by name
    pub fn lookup(&self, pkg_path: Option<&str>, name: &str) -> Option<SymbolId> {
        let key: (Option<Arc<str>>, Arc<str>) = (pkg_path.map(|s| s.into()), name.into());
        self.name_to_id.get(&key).map(|id| *id)
    }

    /// Get symbol name by ID
    pub fn name(&self, id: SymbolId) -> Option<Arc<str>> {
        self.symbols.get(&id).map(|s| s.name.clone())
    }

    /// Number of interned symbols
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Scope-aware symbol resolution
#[derive(Debug, Clone, Default)]
pub struct Scope {
    parent: Option<Box<Scope>>,
    symbols: im::HashMap<SymbolId, crate::core::Entity>,
}

impl Scope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_parent(parent: Scope) -> Self {
        Self {
            parent: Some(Box::new(parent)),
            symbols: im::HashMap::new(),
        }
    }

    pub fn insert(&mut self, symbol: SymbolId, entity: crate::core::Entity) {
        self.symbols.insert(symbol, entity);
    }

    pub fn lookup(&self, symbol: SymbolId) -> Option<crate::core::Entity> {
        self.symbols
            .get(&symbol)
            .copied()
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup(symbol)))
    }

    pub fn contains(&self, symbol: SymbolId) -> bool {
        self.symbols.contains_key(&symbol)
    }

    pub fn into_parent(self) -> Option<Scope> {
        self.parent.map(|p| *p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_interning() {
        let table = SymbolTable::new();

        let id1 = table.intern("test");
        let id2 = table.intern("test");

        assert_eq!(id1, id2);
        assert_eq!(table.name(id1).unwrap().as_ref(), "test");
    }

    #[test]
    fn test_symbol_scoped() {
        let table = SymbolTable::new();

        let id1 = table.intern_in_package(Some("pkg1".into()), "Foo");
        let id2 = table.intern_in_package(Some("pkg2".into()), "Foo");

        assert_ne!(id1, id2);

        assert_eq!(table.lookup(Some("pkg1"), "Foo"), Some(id1));
        assert_eq!(table.lookup(Some("pkg2"), "Foo"), Some(id2));
    }

    #[test]
    fn test_scope_chain() {
        let mut outer = Scope::new();
        let sym = SymbolId::new(1);
        let entity = crate::core::Entity::new(1, 1).unwrap();

        outer.insert(sym, entity);

        let inner = Scope::with_parent(outer.clone());

        assert_eq!(inner.lookup(sym), Some(entity));
    }

    #[test]
    fn test_symbol_table_creation() {
        let table = SymbolTable::new();
        // Table pre-interns empty string, so not empty
        assert!(!table.is_empty());
        assert!(table.len() >= 1);
    }

    #[test]
    fn test_symbol_lookup_missing() {
        let table = SymbolTable::new();
        assert!(table.lookup(None, "missing").is_none());
        assert!(table.get(SymbolId::new(999)).is_none());
    }

    #[test]
    fn test_scope_contains() {
        let mut scope = Scope::new();
        let sym = SymbolId::new(1);
        let entity = crate::core::Entity::new(1, 1).unwrap();

        assert!(!scope.contains(sym));
        scope.insert(sym, entity);
        assert!(scope.contains(sym));
    }

    #[test]
    fn test_deep_scope_chain() {
        let mut scope1 = Scope::new();
        let sym1 = SymbolId::new(1);
        let entity1 = crate::core::Entity::new(1, 1).unwrap();
        scope1.insert(sym1, entity1);

        let mut scope2 = Scope::with_parent(scope1);
        let sym2 = SymbolId::new(2);
        let entity2 = crate::core::Entity::new(2, 1).unwrap();
        scope2.insert(sym2, entity2);

        let scope3 = Scope::with_parent(scope2);

        // Should find both symbols through chain
        assert_eq!(scope3.lookup(sym1), Some(entity1));
        assert_eq!(scope3.lookup(sym2), Some(entity2));
    }

    #[test]
    fn test_symbol_info() {
        let table = SymbolTable::new();
        let id = table.intern("test_symbol");

        let symbol = table.get(id).unwrap();
        assert_eq!(symbol.name.as_ref(), "test_symbol");
        assert_eq!(symbol.id, id);
    }
}
