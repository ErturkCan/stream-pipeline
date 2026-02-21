use crate::buffer::RingBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Watermark-based flow control
/// When downstream buffer utilization exceeds the high watermark,
/// upstream is signaled to slow down
pub struct BackpressureController {
    /// High watermark: threshold at which backpressure is triggered (percentage)
    high_watermark: u32,
    /// Low watermark: threshold at which backpressure is released (percentage)
    low_watermark: u32,
    /// Signal for whether backpressure is currently active
    is_active: Arc<AtomicBool>,
}

impl BackpressureController {
    /// Create a new backpressure controller
    /// Default: high=80%, low=40%
    pub fn new() -> Self {
        Self {
            high_watermark: 80,
            low_watermark: 40,
            is_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set custom watermark thresholds
    pub fn with_watermarks(high: u32, low: u32) -> Self {
        Self {
            high_watermark: high.min(100),
            low_watermark: low.min(100),
            is_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check the buffer and update backpressure state
    /// Returns true if backpressure is now active
    pub fn check_and_update(&self, buffer: &RingBuffer<Vec<u8>>) -> bool {
        let utilization = buffer.utilization();
        let was_active = self.is_active.load(Ordering::Relaxed);

        let is_now_active = if was_active {
            // Already under backpressure, check if we should release
            utilization > self.low_watermark
        } else {
            // Not under backpressure, check if we should activate
            utilization >= self.high_watermark
        };

        if is_now_active != was_active {
            self.is_active.store(is_now_active, Ordering::Relaxed);
        }

        is_now_active
    }

    /// Get whether backpressure is currently active
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }

    /// Apply backpressure by spinning with exponential backoff
    pub fn apply(&self, max_iterations: u32) {
        for i in 0..max_iterations {
            if !self.is_active() {
                break;
            }
            // Exponential backoff with cap
            let backoff_micros = (1u32 << i.min(10)).min(1000);
            thread::sleep(Duration::from_micros(backoff_micros as u64));
        }
    }

    /// Get a clone of the active signal for thread spawning
    pub fn signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_active)
    }

    /// Reset the backpressure state
    pub fn reset(&self) {
        self.is_active.store(false, Ordering::Relaxed);
    }
}

impl Default for BackpressureController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::OverflowPolicy;

    #[test]
    fn test_backpressure_activation() {
        let controller = BackpressureController::with_watermarks(80, 40);
        let buffer = RingBuffer::new(10, OverflowPolicy::Block);

        // Fill buffer to 85%
        for i in 0..9 {
            let _ = buffer.push(vec![i]);
        }

        assert!(controller.check_and_update(&buffer));
        assert!(controller.is_active());
    }

    #[test]
    fn test_backpressure_release() {
        let controller = BackpressureController::with_watermarks(80, 40);
        let buffer = RingBuffer::new(10, OverflowPolicy::Block);

        // Activate backpressure
        for i in 0..9 {
            let _ = buffer.push(vec![i]);
        }
        assert!(controller.check_and_update(&buffer));

        // Drain buffer
        while buffer.pop().is_some() {}

        // Should release
        assert!(!controller.check_and_update(&buffer));
        assert!(!controller.is_active());
    }

    #[test]
    fn test_watermark_thresholds() {
        let controller = BackpressureController::with_watermarks(50, 25);
        assert_eq!(controller.high_watermark, 50);
        assert_eq!(controller.low_watermark, 25);
    }
}
