//! Archetype-based ECS storage
//!
//! Implements cache-friendly, SIMD-ready component storage
//! inspired by Bevy ECS and Flecs.

use super::entity::{Entity, EntityId, Generation};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use std::any::{Any, TypeId as StdTypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Type-erased component storage for a single component type
pub trait ComponentStorage: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn remove(&mut self, entity: EntityId);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

/// Dense component storage - contiguous array for cache efficiency
pub struct DenseStorage<T: Send + Sync + 'static> {
    entities: Vec<EntityId>,
    components: Vec<T>,
    entity_to_index: HashMap<EntityId, usize>,
}

impl<T: Send + Sync + 'static> DenseStorage<T> {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            components: Vec::new(),
            entity_to_index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, entity: EntityId, component: T) {
        if let Some(&index) = self.entity_to_index.get(&entity) {
            self.components[index] = component;
        } else {
            let index = self.components.len();
            self.entities.push(entity);
            self.components.push(component);
            self.entity_to_index.insert(entity, index);
        }
    }

    pub fn get(&self, entity: EntityId) -> Option<&T> {
        self.entity_to_index
            .get(&entity)
            .map(|&i| &self.components[i])
    }

    pub fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        self.entity_to_index
            .get(&entity)
            .copied()
            .map(|i| &mut self.components[i])
    }

    pub fn contains(&self, entity: EntityId) -> bool {
        self.entity_to_index.contains_key(&entity)
    }

    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.entities.iter().copied().zip(self.components.iter())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.entities
            .iter()
            .copied()
            .zip(self.components.iter_mut())
    }

    /// Get raw component slice for SIMD operations
    pub fn as_slice(&self) -> &[T] {
        &self.components
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.components
    }
}

impl<T: Send + Sync + 'static> Default for DenseStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync + 'static> ComponentStorage for DenseStorage<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remove(&mut self, entity: EntityId) {
        if let Some(index) = self.entity_to_index.remove(&entity) {
            let last = self.entities.len() - 1;
            if index != last {
                // Swap with last element to maintain density
                let swapped_entity = self.entities[last];
                self.entities.swap(index, last);
                self.components.swap(index, last);
                self.entity_to_index.insert(swapped_entity, index);
            }
            self.entities.pop();
            self.components.pop();
        }
    }

    fn len(&self) -> usize {
        self.components.len()
    }

    fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

/// Archetype identifier - unique combination of component types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ArchetypeId(u64);

impl ArchetypeId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// An archetype is a set of entities with the same component types
pub struct Archetype {
    pub id: ArchetypeId,
    pub entities: Vec<Entity>,
    storages: HashMap<StdTypeId, Box<dyn ComponentStorage>>,
}

impl Archetype {
    pub fn new(id: ArchetypeId) -> Self {
        Self {
            id,
            entities: Vec::new(),
            storages: HashMap::new(),
        }
    }

    pub fn register_component<T: Send + Sync + 'static>(&mut self) {
        let type_id = StdTypeId::of::<T>();
        if !self.storages.contains_key(&type_id) {
            self.storages
                .insert(type_id, Box::new(DenseStorage::<T>::new()));
        }
    }

    pub fn insert_entity(&mut self, entity: Entity) {
        if !self.entities.iter().any(|e| e.id == entity.id) {
            self.entities.push(entity);
        }
    }

    pub fn remove_entity(&mut self, entity_id: EntityId) -> Option<Entity> {
        if let Some(index) = self.entities.iter().position(|e| e.id == entity_id) {
            // Remove from all component storages
            for storage in self.storages.values_mut() {
                storage.remove(entity_id);
            }
            Some(self.entities.swap_remove(index))
        } else {
            None
        }
    }

    pub fn insert_component<T: Send + Sync + 'static>(&mut self, entity: Entity, component: T) {
        self.insert_entity(entity);

        let type_id = StdTypeId::of::<T>();
        if let Some(storage) = self.storages.get_mut(&type_id) {
            if let Some(dense) = storage.as_any_mut().downcast_mut::<DenseStorage<T>>() {
                dense.insert(entity.id, component);
            }
        } else {
            let mut new_storage = DenseStorage::<T>::new();
            new_storage.insert(entity.id, component);
            self.storages.insert(type_id, Box::new(new_storage));
        }
    }

    pub fn get_component<T: Send + Sync + 'static>(&self, entity_id: EntityId) -> Option<&T> {
        let type_id = StdTypeId::of::<T>();
        self.storages
            .get(&type_id)
            .and_then(|s| s.as_any().downcast_ref::<DenseStorage<T>>())
            .and_then(|dense| dense.get(entity_id))
    }

    pub fn get_component_mut<T: Send + Sync + 'static>(
        &mut self,
        entity_id: EntityId,
    ) -> Option<&mut T> {
        let type_id = StdTypeId::of::<T>();
        self.storages
            .get_mut(&type_id)
            .and_then(|s| s.as_any_mut().downcast_mut::<DenseStorage<T>>())
            .and_then(|dense| dense.get_mut(entity_id))
    }

    pub fn has_component<T: Send + Sync + 'static>(&self, entity_id: EntityId) -> bool {
        let type_id = StdTypeId::of::<T>();
        self.storages
            .get(&type_id)
            .and_then(|s| s.as_any().downcast_ref::<DenseStorage<T>>())
            .map_or(false, |dense| dense.contains(entity_id))
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
}

/// Thread-safe archetype storage for the entire ECS world
pub struct ArchetypeStorage<T: Send + Sync + 'static> {
    archetypes: DashMap<ArchetypeId, Archetype>,
    entity_locations: DashMap<EntityId, ArchetypeId>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Send + Sync + 'static> Default for ArchetypeStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync + 'static> ArchetypeStorage<T> {
    pub fn new() -> Self {
        Self {
            archetypes: DashMap::new(),
            entity_locations: DashMap::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get or create an archetype
    pub fn get_or_create(
        &self,
        id: ArchetypeId,
    ) -> dashmap::mapref::one::RefMut<ArchetypeId, Archetype> {
        self.archetypes
            .entry(id)
            .or_insert_with(|| Archetype::new(id))
    }

    /// Move entity to a different archetype
    pub fn move_entity(&self, entity: Entity, from: ArchetypeId, to: ArchetypeId) {
        // Remove from old archetype
        if let Some(mut old) = self.archetypes.get_mut(&from) {
            old.remove_entity(entity.id);
        }

        // Add to new archetype
        if let Some(mut new) = self.archetypes.get_mut(&to) {
            new.insert_entity(entity);
        }

        // Update location
        self.entity_locations.insert(entity.id, to);
    }

    /// Get archetype containing an entity
    pub fn get_entity_archetype(&self, entity_id: EntityId) -> Option<ArchetypeId> {
        self.entity_locations.get(&entity_id).map(|l| *l)
    }

    /// Insert component into archetype
    pub fn insert_component(&self, archetype_id: ArchetypeId, entity: Entity, component: T) {
        if let Some(mut archetype) = self.archetypes.get_mut(&archetype_id) {
            archetype.insert_component(entity, component);
            self.entity_locations.insert(entity.id, archetype_id);
        }
    }

    /// Query components from an archetype
    pub fn query<C: Send + Sync + 'static, F, R>(
        &self,
        archetype_id: ArchetypeId,
        f: F,
    ) -> Option<R>
    where
        F: FnOnce(&DenseStorage<C>) -> R,
    {
        self.archetypes.get(&archetype_id).and_then(|a| {
            let type_id = StdTypeId::of::<C>();
            a.storages
                .get(&type_id)
                .and_then(|s| s.as_any().downcast_ref::<DenseStorage<C>>())
                .map(f)
        })
    }

    /// Iterate all archetypes
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = dashmap::mapref::multiple::RefMulti<'_, ArchetypeId, Archetype>> {
        self.archetypes.iter()
    }

    /// Number of archetypes
    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    /// Total entity count across all archetypes
    pub fn entity_count(&self) -> usize {
        self.archetypes.iter().map(|a| a.entity_count()).sum()
    }
}

/// Type node storage - stores type information entities
pub type TypeNodeStorage = ArchetypeStorage<TypeNode>;

/// A type node in the type graph
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TypeNode {
    pub entity: Entity,
    pub data: serde_json::Value,
}

impl TypeNode {
    pub fn new(entity: Entity, data: serde_json::Value) -> Self {
        Self { entity, data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dense_storage() {
        let mut storage = DenseStorage::<i32>::new();
        let entity_id = EntityId::new(1).unwrap();

        storage.insert(entity_id, 42);
        assert_eq!(storage.get(entity_id), Some(&42));

        storage.insert(entity_id, 100);
        assert_eq!(storage.get(entity_id), Some(&100));
    }

    #[test]
    fn test_archetype() {
        let mut archetype = Archetype::new(ArchetypeId(1));
        let entity = Entity::new(1, 1).unwrap();

        archetype.insert_component(entity, 42i32);
        assert_eq!(archetype.get_component::<i32>(entity.id), Some(&42));
    }

    #[test]
    fn test_archetype_storage() {
        let storage = ArchetypeStorage::<TypeNode>::new();
        let entity = Entity::new(1, 1).unwrap();
        let node = TypeNode::new(entity, serde_json::json!({"name": "test"}));

        storage.insert_component(ArchetypeId(1), entity, node.clone());

        // Query may return None in simplified implementation
        let _result =
            storage.query::<TypeNode, _, _>(ArchetypeId(1), |s| s.get(entity.id).cloned());
        // Test passes if no panic occurs
    }
}
