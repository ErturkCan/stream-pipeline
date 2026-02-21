//! A backpressure-aware stream processing engine for real-time data pipelines.
//!
//! This crate provides a lock-free, multi-stage pipeline that processes data streams
//! with built-in backpressure support to prevent buffer overflow and maintain system stability.
//!
//! # Features
//!
//! - Lock-free ring buffers using crossbeam's ArrayQueue
//! - Configurable overflow policies (Block or Drop)
//! - Watermark-based backpressure control
//! - Per-stage metrics: throughput, latency percentiles, buffer utilization
//! - Builder pattern for easy pipeline construction
//! - Thread-safe, lock-free operations
//!
//! # Example
//!
//! ```ignore
//! use stream_pipeline::{PipelineBuilder, OverflowPolicy, Stage};
//!
//! let pipeline = PipelineBuilder::new()
//!     .add_stage("stage1", 100, OverflowPolicy::Block)
//!     .add_stage("stage2", 100, OverflowPolicy::Block)
//!     .build()?;
//!
//! let running = pipeline.start(|idx| {
//!     Box::new(PassthroughStage)
//! })?;
//!
//! // Process data...
//! running.wait()?;
//! ```

pub mod backpressure;
pub mod buffer;
pub mod error;
pub mod metrics;
pub mod pipeline;
pub mod stage;

// Re-exports for convenience
pub use backpressure::BackpressureController;
pub use buffer::{OverflowPolicy, RingBuffer};
pub use error::{PipelineError, Result};
pub use metrics::{MetricsSnapshot, StageMetrics};
pub use pipeline::{Pipeline, PipelineBuilder, RunningPipeline};
pub use stage::{FilterStage, MapStage, PassthroughStage, Stage, StageRunner};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
