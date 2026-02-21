use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A simple percentile tracker that maintains a sliding window of measurements
#[derive(Debug, Clone)]
pub struct PercentileTracker {
    measurements: Arc<Mutex<VecDeque<u64>>>,
    window_size: usize,
}

impl PercentileTracker {
    /// Create a new percentile tracker with a specified window size
    pub fn new(window_size: usize) -> Self {
        Self {
            measurements: Arc::new(Mutex::new(VecDeque::with_capacity(window_size))),
            window_size,
        }
    }

    /// Record a measurement (in nanoseconds)
    pub fn record(&self, nanos: u64) {
        let mut measurements = self.measurements.lock();
        if measurements.len() >= self.window_size {
            measurements.pop_front();
        }
        measurements.push_back(nanos);
    }

    /// Calculate the p50 (median) latency in microseconds
    pub fn p50_us(&self) -> f64 {
        self.percentile(0.50)
    }

    /// Calculate the p99 (99th percentile) latency in microseconds
    pub fn p99_us(&self) -> f64 {
        self.percentile(0.99)
    }

    /// Calculate the p95 (95th percentile) latency in microseconds
    pub fn p95_us(&self) -> f64 {
        self.percentile(0.95)
    }

    fn percentile(&self, p: f64) -> f64 {
        let measurements = self.measurements.lock();
        if measurements.is_empty() {
            return 0.0;
        }

        let mut sorted: Vec<_> = measurements.iter().copied().collect();
        sorted.sort_unstable();

        let idx = ((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1);
        sorted[idx] as f64 / 1000.0 // Convert nanoseconds to microseconds
    }

    /// Get the count of recorded measurements
    pub fn count(&self) -> usize {
        self.measurements.lock().len()
    }
}

/// Per-stage metrics collector
#[derive(Debug, Clone)]
pub struct StageMetrics {
    /// Number of messages processed
    messages_processed: Arc<AtomicU64>,
    /// Number of messages dropped
    messages_dropped: Arc<AtomicU64>,
    /// Number of times the stage blocked (backpressure)
    blocks: Arc<AtomicU64>,
    /// Latency histogram (p50, p95, p99)
    latency_tracker: PercentileTracker,
    /// Creation time for throughput calculation
    start_time: Instant,
}

impl StageMetrics {
    /// Create a new metrics collector for a stage
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            messages_processed: Arc::new(AtomicU64::new(0)),
            messages_dropped: Arc::new(AtomicU64::new(0)),
            blocks: Arc::new(AtomicU64::new(0)),
            latency_tracker: PercentileTracker::new(1000),
            start_time: now,
        }
    }

    /// Record a processed message
    pub fn record_processed(&self) {
        self.messages_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a dropped message
    pub fn record_dropped(&self) {
        self.messages_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a block event (backpressure triggered)
    pub fn record_block(&self) {
        self.blocks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a latency measurement in nanoseconds
    pub fn record_latency(&self, nanos: u64) {
        self.latency_tracker.record(nanos);
    }

    /// Get the total number of messages processed
    pub fn total_processed(&self) -> u64 {
        self.messages_processed.load(Ordering::Relaxed)
    }

    /// Get the total number of messages dropped
    pub fn total_dropped(&self) -> u64 {
        self.messages_dropped.load(Ordering::Relaxed)
    }

    /// Get the total number of block events
    pub fn total_blocks(&self) -> u64 {
        self.blocks.load(Ordering::Relaxed)
    }

    /// Calculate current throughput in messages per second
    pub fn throughput_mps(&self) -> f64 {
        let elapsed = self.start_time.elapsed();
        let total = self.total_processed();
        if elapsed.as_secs_f64() == 0.0 {
            0.0
        } else {
            total as f64 / elapsed.as_secs_f64()
        }
    }

    /// Get P50 latency in microseconds
    pub fn latency_p50_us(&self) -> f64 {
        self.latency_tracker.p50_us()
    }

    /// Get P95 latency in microseconds
    pub fn latency_p95_us(&self) -> f64 {
        self.latency_tracker.p95_us()
    }

    /// Get P99 latency in microseconds
    pub fn latency_p99_us(&self) -> f64 {
        self.latency_tracker.p99_us()
    }

    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_processed: self.total_processed(),
            total_dropped: self.total_dropped(),
            total_blocks: self.total_blocks(),
            throughput_mps: self.throughput_mps(),
            latency_p50_us: self.latency_p50_us(),
            latency_p95_us: self.latency_p95_us(),
            latency_p99_us: self.latency_p99_us(),
            elapsed: self.start_time.elapsed(),
        }
    }
}

impl Default for StageMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// A snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_processed: u64,
    pub total_dropped: u64,
    pub total_blocks: u64,
    pub throughput_mps: f64,
    pub latency_p50_us: f64,
    pub latency_p95_us: f64,
    pub latency_p99_us: f64,
    pub elapsed: Duration,
}

impl MetricsSnapshot {
    /// Format metrics as a human-readable string
    pub fn format(&self) -> String {
        format!(
            "Processed: {}, Dropped: {}, Blocks: {}, Throughput: {:.2} msg/s, \
             Latency P50: {:.2}µs, P95: {:.2}µs, P99: {:.2}µs, Elapsed: {:.2}s",
            self.total_processed,
            self.total_dropped,
            self.total_blocks,
            self.throughput_mps,
            self.latency_p50_us,
            self.latency_p95_us,
            self.latency_p99_us,
            self.elapsed.as_secs_f64()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile_tracker() {
        let tracker = PercentileTracker::new(10);
        for i in 1..=10 {
            tracker.record(i * 1000); // 1us to 10us in nanos
        }
        assert!(tracker.p50_us() > 0.0);
        assert!(tracker.p99_us() >= tracker.p50_us());
    }

    #[test]
    fn test_stage_metrics() {
        let metrics = StageMetrics::new();
        for _ in 0..100 {
            metrics.record_processed();
            metrics.record_latency(1000);
        }
        assert_eq!(metrics.total_processed(), 100);
        assert!(metrics.throughput_mps() > 0.0);
    }
}
