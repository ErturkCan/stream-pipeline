use crossbeam::queue::ArrayQueue;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Determines how the buffer should handle overflow conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Block (spin-wait) until space is available
    Block,
    /// Drop the oldest message to make space
    DropOldest,
}

/// A lock-free ring buffer using crossbeam's ArrayQueue
#[derive(Debug)]
pub struct RingBuffer<T: Send> {
    queue: Arc<ArrayQueue<T>>,
    policy: OverflowPolicy,
    dropped_count: Arc<AtomicU64>,
    block_count: Arc<AtomicU64>,
}

impl<T: Send> Clone for RingBuffer<T> {
    fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
            policy: self.policy,
            dropped_count: Arc::clone(&self.dropped_count),
            block_count: Arc::clone(&self.block_count),
        }
    }
}

impl<T: Send> RingBuffer<T> {
    /// Create a new ring buffer with the specified capacity and overflow policy
    pub fn new(capacity: usize, policy: OverflowPolicy) -> Self {
        Self {
            queue: Arc::new(ArrayQueue::new(capacity)),
            policy,
            dropped_count: Arc::new(AtomicU64::new(0)),
            block_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Attempt to push an item into the buffer
    pub fn push(&self, item: T) -> Result<(), T> {
        match self.queue.push(item) {
            Ok(()) => Ok(()),
            Err(item) => {
                match self.policy {
                    OverflowPolicy::Block => {
                        self.block_count.fetch_add(1, Ordering::Relaxed);
                        self.push_blocking(item)
                    }
                    OverflowPolicy::DropOldest => {
                        self.dropped_count.fetch_add(1, Ordering::Relaxed);
                        // Try to pop an old item and push the new one
                        let _ = self.queue.pop();
                        let _ = self.queue.push(item);
                        Ok(())
                    }
                }
            }
        }
    }

    /// Push with blocking until space is available
    fn push_blocking(&self, mut item: T) -> Result<(), T> {
        loop {
            match self.queue.push(item) {
                Ok(()) => return Ok(()),
                Err(i) => {
                    item = i;
                    // Spin with a small backoff to reduce CPU usage
                    thread::sleep(Duration::from_micros(1));
                }
            }
        }
    }

    /// Attempt to pop an item from the buffer
    pub fn pop(&self) -> Option<T> {
        self.queue.pop()
    }

    /// Get the current size of the buffer
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get the capacity of the buffer
    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    /// Get the utilization of the buffer as a percentage (0-100)
    pub fn utilization(&self) -> u32 {
        ((self.len() as u32 * 100) / self.capacity() as u32).min(100)
    }

    /// Get the number of dropped messages
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }

    /// Get the number of block events
    pub fn block_count(&self) -> u64 {
        self.block_count.load(Ordering::Relaxed)
    }

    /// Reset counters (useful for testing)
    pub fn reset_counters(&self) {
        self.dropped_count.store(0, Ordering::Relaxed);
        self.block_count.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_push_pop() {
        let buffer = RingBuffer::new(10, OverflowPolicy::Block);
        assert!(buffer.push(42).is_ok());
        assert_eq!(buffer.pop(), Some(42));
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_buffer_utilization() {
        let buffer = RingBuffer::new(10, OverflowPolicy::Block);
        for i in 0..5 {
            let _ = buffer.push(i);
        }
        assert_eq!(buffer.utilization(), 50);
    }

    #[test]
    fn test_drop_policy() {
        let buffer = RingBuffer::new(3, OverflowPolicy::DropOldest);
        let _ = buffer.push(1);
        let _ = buffer.push(2);
        let _ = buffer.push(3);
        let _ = buffer.push(4); // Should drop 1
        assert_eq!(buffer.dropped_count(), 1);
    }

    #[test]
    fn test_block_policy() {
        let buffer = RingBuffer::new(2, OverflowPolicy::Block);
        assert!(buffer.push(1).is_ok());
        assert!(buffer.push(2).is_ok());
        // Next push would block, let's just verify it succeeds after pop
        let _ = buffer.pop();
        assert!(buffer.push(3).is_ok());
    }

    #[test]
    fn test_capacity() {
        let buffer: RingBuffer<i32> = RingBuffer::new(42, OverflowPolicy::Block);
        assert_eq!(buffer.capacity(), 42);
    }
}
