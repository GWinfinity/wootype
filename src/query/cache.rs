//! Query result cache with LRU eviction
//!
//! Provides microsecond-level repeated query results.

use dashmap::DashMap;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Cache entry with metadata
struct CacheEntry<V> {
    value: V,
    access_count: AtomicU64,
    last_accessed: Mutex<Instant>,
    created_at: Instant,
}

impl<V> CacheEntry<V> {
    fn new(value: V) -> Self {
        let now = Instant::now();
        Self {
            value,
            access_count: AtomicU64::new(1),
            last_accessed: Mutex::new(now),
            created_at: now,
        }
    }

    fn touch(&self) {
        self.access_count.fetch_add(1, Ordering::Relaxed);
        *self.last_accessed.lock() = Instant::now();
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// LRU cache for query results
pub struct QueryCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    store: DashMap<K, CacheEntry<V>>,
    max_size: usize,
    default_ttl: Duration,
    hit_count: AtomicU64,
    miss_count: AtomicU64,
}

impl<K, V> QueryCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    pub fn new(max_size: usize) -> Self {
        Self {
            store: DashMap::new(),
            max_size,
            default_ttl: Duration::from_secs(300), // 5 minutes
            hit_count: AtomicU64::new(0),
            miss_count: AtomicU64::new(0),
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Get value from cache
    pub fn get(&self, key: &K) -> Option<V> {
        if let Some(entry) = self.store.get(key) {
            if entry.is_expired(self.default_ttl) {
                drop(entry);
                self.store.remove(key);
                self.miss_count.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            entry.touch();
            self.hit_count.fetch_add(1, Ordering::Relaxed);
            Some(entry.value.clone())
        } else {
            self.miss_count.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Insert value into cache
    pub fn insert(&self, key: K, value: V) {
        // Evict if at capacity
        if self.store.len() >= self.max_size {
            self.evict_lru();
        }

        let entry = CacheEntry::new(value);
        self.store.insert(key, entry);
    }

    /// Get or compute value
    pub fn get_or_insert<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> V,
    {
        if let Some(value) = self.get(&key) {
            return value;
        }

        let value = f();
        self.insert(key, value.clone());
        value
    }

    /// Invalidate a specific key
    pub fn invalidate(&self, key: &K) {
        self.store.remove(key);
    }

    /// Invalidate all keys matching a predicate
    pub fn invalidate_where<F>(&self, predicate: F)
    where
        F: Fn(&K) -> bool,
    {
        let keys_to_remove: Vec<K> = self
            .store
            .iter()
            .filter(|e| predicate(e.key()))
            .map(|e| e.key().clone())
            .collect();

        for key in keys_to_remove {
            self.store.remove(&key);
        }
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.store.clear();
        self.hit_count.store(0, Ordering::Relaxed);
        self.miss_count.store(0, Ordering::Relaxed);
    }

    /// Evict least recently used entry
    fn evict_lru(&self) {
        let lru_key = self
            .store
            .iter()
            .min_by_key(|e| *e.last_accessed.lock())
            .map(|e| e.key().clone());

        if let Some(key) = lru_key {
            self.store.remove(&key);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let hits = self.hit_count.load(Ordering::Relaxed);
        let misses = self.miss_count.load(Ordering::Relaxed);
        let total = hits + misses;

        CacheStats {
            size: self.store.len(),
            max_size: self.max_size,
            hits,
            misses,
            hit_rate: if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            },
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub size: usize,
    pub max_size: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache: {}/{} entries, {} hits, {} misses, {:.2}% hit rate",
            self.size,
            self.max_size,
            self.hits,
            self.misses,
            self.hit_rate * 100.0
        )
    }
}

/// Type-safe query cache wrapper
pub struct TypedQueryCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    inner: QueryCache<K, V>,
}

impl<K, V> TypedQueryCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    pub fn new(max_size: usize) -> Self {
        Self {
            inner: QueryCache::new(max_size),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.inner.get(key)
    }

    pub fn insert(&self, key: K, value: V) {
        self.inner.insert(key, value);
    }

    pub fn stats(&self) -> CacheStats {
        self.inner.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let cache = QueryCache::<String, i32>::new(100);

        cache.insert("key".to_string(), 42);
        assert_eq!(cache.get(&"key".to_string()), Some(42));
        assert_eq!(cache.get(&"missing".to_string()), None);
    }

    #[test]
    fn test_cache_get_or_insert() {
        let cache = QueryCache::<String, i32>::new(100);

        let value = cache.get_or_insert("key".to_string(), || 42);
        assert_eq!(value, 42);

        // Second call should use cached value
        let value2 = cache.get_or_insert("key".to_string(), || 999);
        assert_eq!(value2, 42);
    }

    #[test]
    fn test_cache_stats() {
        let cache = QueryCache::<String, i32>::new(100);

        cache.insert("key".to_string(), 42);
        cache.get(&"key".to_string());
        cache.get(&"key".to_string());
        cache.get(&"missing".to_string());

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
    }
}
