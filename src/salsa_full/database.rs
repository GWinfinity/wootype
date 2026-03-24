//! Full Salsa database definition
//!
//! This module defines the main database struct using salsa-rs macros
//! for true incremental computation.

use super::metrics::MetricsCollector;
use std::sync::Arc;

/// The main Salsa database for type checking
#[salsa::db]
#[derive(Clone)]
pub struct TypeDatabase {
    storage: salsa::Storage<Self>,
    metrics: Arc<MetricsCollector>,
}

impl TypeDatabase {
    /// Create a new database instance
    pub fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            metrics: Arc::new(MetricsCollector::new()),
        }
    }

    /// Get the metrics collector
    pub fn metrics(&self) -> &MetricsCollector {
        &self.metrics
    }

    /// Get a snapshot of current metrics
    pub fn metrics_snapshot(&self) -> super::MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Record a query execution (for metrics)
    pub fn record_query(&self, name: &str, duration: std::time::Duration, cached: bool) {
        self.metrics.record_query(name, duration, cached);
    }
}

impl Default for TypeDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[salsa::db]
impl salsa::Database for TypeDatabase {}

/// Re-export salsa Database trait for convenience
pub use salsa::Database;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_creation() {
        let db = TypeDatabase::new();
        let _ = db.metrics();
    }

    #[test]
    fn test_metrics_snapshot() {
        let db = TypeDatabase::new();
        let snapshot = db.metrics_snapshot();
        assert_eq!(snapshot.total_queries, 0);
    }
}
