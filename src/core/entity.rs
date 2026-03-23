//! Entity Component System (ECS) core
//! 
//! Uses archetype-based storage for cache-friendly, SIMD-friendly data layout.

use std::sync::atomic::{AtomicU64, Ordering};
use std::num::NonZeroU64;

/// Unique entity identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EntityId(NonZeroU64);

impl EntityId {
    pub fn new(id: u64) -> Option<Self> {
        NonZeroU64::new(id).map(Self)
    }
    
    pub fn as_u64(&self) -> u64 {
        self.0.get()
    }
}

impl Default for EntityId {
    fn default() -> Self {
        // This should never be used in production, but helps with tests
        Self(NonZeroU64::new(1).unwrap())
    }
}

/// Generation counter for entity liveness tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct Generation(u32);

impl Generation {
    pub fn new(gen: u32) -> Self {
        Self(gen)
    }
    
    pub fn next(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
    
    pub fn is_alive(&self, other: Generation) -> bool {
        self.0 == other.0
    }
}

/// A live entity reference in the ECS world
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    pub id: EntityId,
    pub generation: Generation,
}

impl Entity {
    pub fn new(id: u64, generation: u32) -> Option<Self> {
        EntityId::new(id).map(|id| Self {
            id,
            generation: Generation(generation),
        })
    }
    
    pub fn id(&self) -> EntityId {
        self.id
    }
}

/// Thread-safe entity ID generator
pub struct EntityGenerator {
    counter: AtomicU64,
}

impl Default for EntityGenerator {
    fn default() -> Self {
        Self {
            counter: AtomicU64::new(1),
        }
    }
}

impl EntityGenerator {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Generate a new unique entity ID
    pub fn generate(&self) -> EntityId {
        let id = self.counter.fetch_add(1, Ordering::SeqCst);
        EntityId::new(id).expect("Entity ID overflow")
    }
    
    /// Get current counter value (for debugging/snapshot purposes)
    pub fn current(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_generation() {
        let generator = EntityGenerator::new();
        let id1 = generator.generate();
        let id2 = generator.generate();
        
        assert_ne!(id1, id2);
        assert_eq!(id2.as_u64(), id1.as_u64() + 1);
    }
    
    #[test]
    fn test_entity_creation() {
        let entity = Entity::new(42, 1).unwrap();
        assert_eq!(entity.id.as_u64(), 42);
        assert_eq!(entity.generation.0, 1);
    }
}
