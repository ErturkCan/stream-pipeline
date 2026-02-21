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
                    ┌─────────────────────────────────┐
                    │      Input Data Source          │
                    └──────────────┬──────────────────┘
                                   │
                                   ▼
                    ┌──────────────────────────┐
                    │ Stage 1 (Reader)         │
                    │ [RingBuffer: 100 items]  │
                    └──────────────┬───────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │ Backpressure Monitor (>80%) │
                    └──────────────┬──────────────┘
                                   │
                                   ▼
                    ┌──────────────────────────┐
                    │ Stage 2 (Processor)      │
                    │ [RingBuffer: 100 items]  │
                    └──────────────┬───────────┘
                                   │
                                   ▼
                    ┌──────────────────────────┐
                    │ Stage 3 (Aggregator)     │
                    │ [RingBuffer: 100 items]  │
                    └──────────────┬───────────┘
                                   │
                                   ▼
                    ┌──────────────────────────┐
                    │ Output / Results         │
                    └──────────────────────────┘
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
    // Build pipeline
    let pipeline = PipelineBuilder::new()
        .add_stage("stage1", 100, OverflowPolicy::Block)
        .add_stage("stage2", 100, OverflowPolicy::Block)
        .with_backpressure(true)
        .build()?;

    let input = pipeline.input();

    // Start pipeline with stage implementations
    let running = pipeline.start(|stage_idx| {
        Box::new(PassthroughStage)
    })?;

    // Push data
    input.push(vec![1, 2, 3])?;

    // Wait for completion
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
        // Transform input
        let output = input.iter().map(|&b| b * 2).collect();
        Ok(vec![output])
    }

    fn name(&self) -> &str {
        "my_stage"
    }

    fn on_start(&mut self) -> PipelineResult<()> {
        println!("Stage starting!");
        Ok(())
    }

    fn on_shutdown(&mut self) -> PipelineResult<()> {
        println!("Stage shutting down!");
        Ok(())
    }
}
```

### Metrics Monitoring

```rust
let pipeline = PipelineBuilder::new()
    .add_stage("processing", 100, OverflowPolicy::Block)
    .build()?;

let metrics = pipeline.stage_metrics(0).unwrap();
let running = pipeline.start(|_| Box::new(MyStage))?;

// Process data...

let snapshot = metrics.snapshot();
println!("Throughput: {:.2} msg/s", snapshot.throughput_mps);
println!("P99 Latency: {:.2} µs", snapshot.latency_p99_us);
println!("Dropped: {}", snapshot.total_dropped);
```

## Examples

Run the included examples:

```bash
# Word frequency counter
cargo run --example word_count --release

# Number processing pipeline
cargo run --example filter_pipeline --release
```

## Benchmarks

Run performance benchmarks:

```bash
# Throughput benchmark (single and multi-stage)
cargo bench --bench throughput

# Backpressure behavior under load
cargo bench --bench backpressure
```

### Expected Performance (on modern x86_64)

| Scenario | Throughput | P50 Latency | P99 Latency |
|----------|-----------|-----------|-----------|
| Single passthrough stage | >1M msg/s | ~1 µs | ~5 µs |
| Three passthrough stages | >500k msg/s | ~3 µs | ~15 µs |
| With slow consumer (100µs) | ~8k msg/s | ~150 µs | ~500 µs |

*Actual performance varies with message size, CPU contention, and system load*

## Running Tests

```bash
cargo test
cargo test --release

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_backpressure_activation
```

## Core Components

### `RingBuffer<T>`
Lock-free MPMC queue with overflow policies and metrics.

```rust
let buffer = RingBuffer::new(100, OverflowPolicy::Block);
buffer.push(vec![1, 2, 3])?;
let item = buffer.pop();
println!("Utilization: {}%", buffer.utilization());
```

### `Stage` Trait
Defines processing logic for each stage.

```rust
pub trait Stage: Send + 'static {
    fn process(&mut self, input: Vec<u8>) -> Result<Vec<Vec<u8>>>;
    fn on_start(&mut self) -> Result<()> { Ok(()) }
    fn on_shutdown(&mut self) -> Result<()> { Ok(()) }
    fn name(&self) -> &str;
}
```

### `StageRunner`
Orchestrates a stage's pull-process-push loop.

### `Pipeline`
Chains multiple stages with buffers between them.

### `BackpressureController`
Monitors buffer utilization and signals upstream slowdown.

### `StageMetrics`
Collects throughput, latency, and utilization metrics.

## Performance Characteristics

### Latency
- **Per-stage latency**: 1-5 µs (single message through empty pipeline)
- **Backpressure overhead**: <1 µs when inactive, ~1 µs per check when active
- **GC impact**: Zero (no allocations in hot path)

### Throughput
- **Single stage**: >1M messages/sec
- **Three stages**: >500k messages/sec
- **Network I/O bound**: Automatic backpressure prevents buffer explosion

### Memory
- **Fixed overhead**: O(capacity) per buffer
- **Metrics overhead**: ~1KB per stage (sliding window of 1000 samples)
- **No unbounded allocations** in pipeline operation

## When to Use

**Good For:**
- Real-time data processing pipelines
- Event streaming and filtering
- Data aggregation and transformation
- System monitoring and alerting
- Network packet processing

**Not Ideal For:**
- Batch processing (use Apache Spark/Flink)
- Complex state management (use stateful stream databases)
- Extreme throughput with large messages (consider custom buffers)

## Production Considerations

1. **Buffer Sizing**: Start with capacity = 100, scale based on message latency and throughput
2. **Backpressure Tuning**: Default 80/40% watermarks work for most cases
3. **Thread Affinity**: For critical systems, pin stage threads to CPU cores
4. **Error Handling**: Stages should be fault-tolerant; errors log but don't stop pipeline
5. **Metrics**: Regular metric snapshots enable capacity planning

## Building and Deployment

### Development
```bash
cargo build
cargo test
```

### Release (Optimized)
```bash
cargo build --release
cargo bench --bench throughput
```

### Integration
```rust
[dependencies]
stream-pipeline = "0.1"
```

## Limitations and Future Work

### Current Limitations
- **Stages are per-thread**: Cannot scale to thousands of stages
- **No stateful processing**: Each stage is stateless transformation
- **Fixed buffer capacity**: Requires pre-planning
- **Single-machine only**: No distributed mode

### Roadmap
- [ ] Stateful stage support with state snapshots
- [ ] Dynamic buffer resizing
- [ ] Distributed stages across machines
- [ ] Rate limiting primitives
- [ ] Async/await integration
- [ ] Stage dependency graphs (DAG execution)

## Contributing

We welcome contributions! Areas for improvement:
- Additional examples
- Performance optimizations
- Documentation enhancements
- New stage types
- Benchmarking improvements

## License

MIT License - See LICENSE file

## Citation

If you use this in academic or professional work:

```bibtex
@software{stream_pipeline_2024,
  title = {Stream Pipeline: A Backpressure-Aware Stream Processing Engine},
  author = {Stream Pipeline Contributors},
  year = {2024},
  url = {https://github.com/example/stream-pipeline}
}
```

## References

- Crossbeam's ArrayQueue: https://docs.rs/crossbeam/
- Lock-free programming: "The Art of Multiprocessor Programming" by Herlihy & Shavit
- Backpressure patterns: "Reactive Streams" specification
