//! Performance monitoring and metrics
//!
//! Tracks query execution times, cache hit rates, and memory usage
//! for optimizing the type checker.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Global metrics collector
#[derive(Clone)]
pub struct MetricsCollector {
    /// Query execution statistics
    query_stats: Arc<Mutex<HashMap<String, QueryMetrics>>>,
    /// Cache statistics
    cache_hits: Arc<AtomicU64>,
    cache_misses: Arc<AtomicU64>,
    /// Current memory usage estimate (in bytes)
    memory_usage: Arc<AtomicU64>,
    /// Active operations
    active_operations: Arc<Mutex<Vec<ActiveOperation>>>,
}

/// Metrics for a single query type
#[derive(Clone, Debug, Default)]
pub struct QueryMetrics {
    pub name: String,
    pub total_calls: u64,
    pub total_time: Duration,
    pub min_time: Duration,
    pub max_time: Duration,
    pub cached_calls: u64,
}

/// Currently running operation
#[derive(Clone, Debug)]
pub struct ActiveOperation {
    pub name: String,
    pub started_at: Instant,
    pub span_id: u64,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            query_stats: Arc::new(Mutex::new(HashMap::new())),
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
            memory_usage: Arc::new(AtomicU64::new(0)),
            active_operations: Arc::new(Mutex::new(vec![])),
        }
    }
    
    /// Record a query execution
    pub fn record_query(&self, name: &str, duration: Duration, cached: bool) {
        let mut stats = self.query_stats.lock();
        let entry = stats.entry(name.to_string()).or_insert_with(|| QueryMetrics {
            name: name.to_string(),
            min_time: duration,
            max_time: duration,
            ..Default::default()
        });
        
        entry.total_calls += 1;
        entry.total_time += duration;
        entry.min_time = entry.min_time.min(duration);
        entry.max_time = entry.max_time.max(duration);
        
        if cached {
            entry.cached_calls += 1;
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Record cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Update memory usage
    pub fn update_memory_usage(&self, bytes: u64) {
        self.memory_usage.store(bytes, Ordering::Relaxed);
    }
    
    /// Add to memory usage
    pub fn add_memory(&self, bytes: u64) {
        self.memory_usage.fetch_add(bytes, Ordering::Relaxed);
    }
    
    /// Subtract from memory usage
    pub fn subtract_memory(&self, bytes: u64) {
        let _ = self.memory_usage.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_sub(bytes))
        });
    }
    
    /// Start tracking an operation
    pub fn start_operation(&self, name: &str) -> OperationGuard {
        let span_id = rand::random();
        let operation = ActiveOperation {
            name: name.to_string(),
            started_at: Instant::now(),
            span_id,
        };
        
        self.active_operations.lock().push(operation);
        
        OperationGuard {
            collector: self.clone(),
            span_id,
        }
    }
    
    /// Get all metrics as a snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        let query_stats = self.query_stats.lock().clone();
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        let memory_usage = self.memory_usage.load(Ordering::Relaxed);
        let active_ops = self.active_operations.lock().clone();
        
        let total_queries: u64 = query_stats.values().map(|m| m.total_calls).sum();
        let total_time: Duration = query_stats.values().map(|m| m.total_time).sum();
        
        MetricsSnapshot {
            query_stats,
            cache_hits,
            cache_misses,
            cache_hit_rate: if cache_hits + cache_misses > 0 {
                cache_hits as f64 / (cache_hits + cache_misses) as f64
            } else {
                0.0
            },
            memory_usage_bytes: memory_usage,
            memory_usage_mb: memory_usage as f64 / (1024.0 * 1024.0),
            total_queries,
            total_query_time: total_time,
            active_operations: active_ops,
        }
    }
    
    /// Reset all metrics
    pub fn reset(&self) {
        self.query_stats.lock().clear();
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.memory_usage.store(0, Ordering::Relaxed);
        self.active_operations.lock().clear();
    }
    
    /// Print formatted report
    pub fn print_report(&self) {
        let snapshot = self.snapshot();
        
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║                   WOOTYPE METRICS REPORT                     ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Memory Usage:     {:>10.2} MB                              ║", snapshot.memory_usage_mb);
        println!("║ Cache Hit Rate:   {:>10.1}%                                ║", snapshot.cache_hit_rate * 100.0);
        println!("║ Total Queries:    {:>10}                                  ║", snapshot.total_queries);
        println!("║ Active Operations: {:>10}                                  ║", snapshot.active_operations.len());
        println!("╚══════════════════════════════════════════════════════════════╝");
        
        println!("\nQuery Performance:");
        println!("{:<30} {:>10} {:>12} {:>12} {:>12}", "Query", "Calls", "Avg(ms)", "Min(ms)", "Max(ms)");
        println!("{}", "─".repeat(80));
        
        let mut sorted_stats: Vec<_> = snapshot.query_stats.values().collect();
        sorted_stats.sort_by(|a, b| b.total_time.cmp(&a.total_time));
        
        for stat in sorted_stats {
            let avg = stat.total_time.as_millis() as f64 / stat.total_calls.max(1) as f64;
            let min = stat.min_time.as_millis();
            let max = stat.max_time.as_millis();
            println!(
                "{:<30} {:>10} {:>12.2} {:>12} {:>12}",
                stat.name, stat.total_calls, avg, min, max
            );
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for operation tracking
pub struct OperationGuard {
    collector: MetricsCollector,
    span_id: u64,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let mut ops = self.collector.active_operations.lock();
        if let Some(pos) = ops.iter().position(|op| op.span_id == self.span_id) {
            ops.remove(pos);
        }
    }
}

/// Snapshot of all metrics at a point in time
#[derive(Clone, Debug)]
pub struct MetricsSnapshot {
    pub query_stats: HashMap<String, QueryMetrics>,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub memory_usage_bytes: u64,
    pub memory_usage_mb: f64,
    pub total_queries: u64,
    pub total_query_time: Duration,
    pub active_operations: Vec<ActiveOperation>,
}

impl MetricsSnapshot {
    /// Export as JSON
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        
        let query_stats: Vec<_> = self.query_stats.values().map(|s| {
            json!({
                "name": s.name,
                "total_calls": s.total_calls,
                "avg_time_ms": s.total_time.as_millis() as f64 / s.total_calls.max(1) as f64,
                "min_time_ms": s.min_time.as_millis(),
                "max_time_ms": s.max_time.as_millis(),
                "cached_calls": s.cached_calls,
            })
        }).collect();
        
        json!({
            "cache_hit_rate": self.cache_hit_rate,
            "cache_hits": self.cache_hits,
            "cache_misses": self.cache_misses,
            "memory_usage_mb": self.memory_usage_mb,
            "total_queries": self.total_queries,
            "total_query_time_ms": self.total_query_time.as_millis(),
            "query_stats": query_stats,
            "active_operations": self.active_operations.len(),
        })
    }
}

/// Timer for measuring operation duration
pub struct Timer {
    start: Instant,
    name: String,
    collector: Option<MetricsCollector>,
}

impl Timer {
    pub fn new(name: &str) -> Self {
        Self {
            start: Instant::now(),
            name: name.to_string(),
            collector: None,
        }
    }
    
    pub fn with_collector(name: &str, collector: &MetricsCollector) -> Self {
        Self {
            start: Instant::now(),
            name: name.to_string(),
            collector: Some(collector.clone()),
        }
    }
    
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
    
    pub fn stop(self) -> Duration {
        let duration = self.start.elapsed();
        
        if let Some(ref collector) = self.collector {
            collector.record_query(&self.name, duration, false);
        }
        
        duration
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Don't record if stop() was called
    }
}

/// Performance budget checker
pub struct PerformanceBudget {
    /// Maximum allowed query time
    max_query_time: Duration,
    /// Maximum allowed memory usage (bytes)
    max_memory: u64,
    /// Warning threshold (percentage)
    warning_threshold: f64,
}

impl PerformanceBudget {
    pub fn new(max_query_time_ms: u64, max_memory_mb: u64) -> Self {
        Self {
            max_query_time: Duration::from_millis(max_query_time_ms),
            max_memory: max_memory_mb * 1024 * 1024,
            warning_threshold: 0.8,
        }
    }
    
    /// Check if metrics are within budget
    pub fn check(&self, metrics: &MetricsSnapshot) -> BudgetStatus {
        let mut warnings = vec![];
        let mut violations = vec![];
        
        // Check query times
        for (name, stat) in &metrics.query_stats {
            let avg_time = stat.total_time / stat.total_calls.max(1) as u32;
            
            if avg_time > self.max_query_time {
                violations.push(BudgetViolation::QueryTooSlow {
                    query: name.clone(),
                    avg_time,
                    max_time: self.max_query_time,
                });
            } else if avg_time.as_secs_f64() > self.max_query_time.as_secs_f64() * self.warning_threshold {
                warnings.push(BudgetWarning::QuerySlow {
                    query: name.clone(),
                    avg_time,
                    threshold: self.max_query_time,
                });
            }
        }
        
        // Check memory
        if metrics.memory_usage_bytes > self.max_memory {
            violations.push(BudgetViolation::MemoryExceeded {
                used: metrics.memory_usage_bytes,
                limit: self.max_memory,
            });
        } else if (metrics.memory_usage_bytes as f64) > (self.max_memory as f64 * self.warning_threshold) {
            warnings.push(BudgetWarning::MemoryHigh {
                used: metrics.memory_usage_bytes,
                threshold: self.max_memory,
            });
        }
        
        // Check cache hit rate
        if metrics.cache_hit_rate < 0.5 && metrics.total_queries > 100 {
            warnings.push(BudgetWarning::LowCacheHitRate {
                rate: metrics.cache_hit_rate,
            });
        }
        
        BudgetStatus { warnings, violations }
    }
}

/// Budget check result
#[derive(Clone, Debug)]
pub struct BudgetStatus {
    pub warnings: Vec<BudgetWarning>,
    pub violations: Vec<BudgetViolation>,
}

impl BudgetStatus {
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }
    
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

#[derive(Clone, Debug)]
pub enum BudgetWarning {
    QuerySlow { query: String, avg_time: Duration, threshold: Duration },
    MemoryHigh { used: u64, threshold: u64 },
    LowCacheHitRate { rate: f64 },
}

#[derive(Clone, Debug)]
pub enum BudgetViolation {
    QueryTooSlow { query: String, avg_time: Duration, max_time: Duration },
    MemoryExceeded { used: u64, limit: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_record_query() {
        let metrics = MetricsCollector::new();
        
        metrics.record_query("test_query", Duration::from_millis(10), false);
        metrics.record_query("test_query", Duration::from_millis(20), false);
        metrics.record_query("test_query", Duration::from_millis(5), true);
        
        let snapshot = metrics.snapshot();
        let stat = snapshot.query_stats.get("test_query").unwrap();
        
        assert_eq!(stat.total_calls, 3);
        assert_eq!(stat.cached_calls, 1);
    }
    
    #[test]
    fn test_cache_hit_rate() {
        let metrics = MetricsCollector::new();
        
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();
        
        let snapshot = metrics.snapshot();
        assert!((snapshot.cache_hit_rate - 0.666).abs() < 0.01);
    }
    
    #[test]
    fn test_budget_check() {
        let budget = PerformanceBudget::new(100, 100); // 100ms, 100MB
        
        let metrics = MetricsCollector::new();
        metrics.record_query("slow_query", Duration::from_millis(150), false);
        metrics.add_memory(150 * 1024 * 1024); // 150MB
        
        let snapshot = metrics.snapshot();
        let status = budget.check(&snapshot);
        
        assert!(!status.is_ok());
        assert_eq!(status.violations.len(), 2);
    }
}