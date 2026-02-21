# Stream Pipeline: A Backpressure-Aware Stream Processing Engine

A production-ready, lock-free stream processing engine for real-time data pipelines with built-in backpressure support, designed for low-latency, high-throughput applications.

## Features

- **Lock-Free Design**: Uses crossbeam's `ArrayQueue` for zero-copy, contention-free data passing between stages
- **Configurable Backpressure**: Watermark-based flow control with automatic pressure propagation
- **Flexible Overflow Policies**: Choose between blocking (wait for space) or dropping old messages
- **Per-Stage Metrics**: Track throughput, latency percentiles (P50, P95, P99), buffer utilization, and drop counts
- **Builder Pattern**: Intuitive, fluent API for pipeline construction
- **Thread-Safe**: Safe concurrent access with atomic operations and arc-managed state
- **Minimal Dependencies**: Only crossbeam, thiserror, and parking_lot

## Architecture

```
                    Input Data Source
                          |
                          v
                    Stage 1 (Reader)
                    [RingBuffer: 100 items]
                          |
                    Backpressure Monitor (>80%)
                          |
                          v
                    Stage 2 (Processor)
                    [RingBuffer: 100 items]
                          |
                          v
                    Stage 3 (Aggregator)
                    [RingBuffer: 100 items]
                          |
                          v
                    Output / Results
```

## Design Decisions

### 1. Lock-Free Ring Buffers
- **Why**: Eliminates lock contention between stages, enabling sub-microsecond latencies
- **Implementation**: Built on crossbeam's `ArrayQueue` (MPMC queue)
- **Trade-off**: Fixed capacity (must be pre-configured), but provides guaranteed performance

### 2. Watermark-Based Backpressure
- **How it works**:
  - When a stage's output buffer reaches 80% utilization, backpressure activates
  - Upstream stages are signaled to slow down (spin-wait with exponential backoff)
  - Released when buffer drops below 40% utilization (hysteresis prevents oscillation)
- **Benefits**: Prevents cascading buffer overflows while minimizing false positives

### 3. Configurable Overflow Policies
- **Block**: Producer spins with backoff waiting for buffer space (default for critical stages)
- **DropOldest**: Discards the oldest item to make space for new one (for non-critical aggregation)
- **Use cases**: Block = transactional pipelines, DropOldest = real-time monitoring

### 4. Per-Stage Metrics Collection
- **Zero-overhead atomic counters** for message counts and events
- **Sliding-window latency tracker** (P50, P95, P99) with fixed memory overhead
- **Throughput calculation** from start time (no periodic snapshots needed)
- **Buffer utilization** tracked continuously

### 5. Thread-per-Stage Model
- **Simplicity**: Each stage runs in its own thread, enabling CPU-level parallelism
- **Scalability**: O(n) threads for n stages (reasonable for typical pipelines)
- **Performance**: Avoids work-stealing overhead for predictable, throughput-critical workloads

## Usage

### Basic Pipeline

```rust
use stream_pipeline::{PipelineBuilder, OverflowPolicy, PassthroughStage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pipeline = PipelineBuilder::new()
        .add_stage("stage1", 100, OverflowPolicy::Block)
        .add_stage("stage2", 100, OverflowPolicy::Block)
        .with_backpressure(true)
        .build()?;

    let input = pipeline.input();
    let running = pipeline.start(|stage_idx| {
        Box::new(PassthroughStage)
    })?;

    input.push(vec![1, 2, 3])?;
    running.wait()?;
    Ok(())
}
```

### Custom Stages

```rust
use stream_pipeline::{Stage, Result as PipelineResult};

struct MyStage;

impl Stage for MyStage {
    fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        let output = input.iter().map(|&b| b * 2).collect();
        Ok(vec![output])
    }

    fn name(&self) -> &str { "my_stage" }
}
```

## Benchmarks

| Scenario | Throughput | P50 Latency | P99 Latency |
|----------|-----------|-----------|-----------|
| Single passthrough stage | >1M msg/s | ~1 us | ~5 us |
| Three passthrough stages | >500k msg/s | ~3 us | ~15 us |
| With slow consumer (100us) | ~8k msg/s | ~150 us | ~500 us |

## License

MIT License - See LICENSE file
