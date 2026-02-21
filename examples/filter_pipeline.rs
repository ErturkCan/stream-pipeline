//! Number processing pipeline demonstrating filtering and transformations
//!
//! Pipeline:
//! 1. Generate: Produces numbers 1-100
//! 2. Filter: Keep only even numbers
//! 3. Transform: Multiply by 10
//! 4. Aggregate: Sum all numbers
//!
//! Usage: cargo run --example filter_pipeline --release

use std::time::{Duration, Instant};
use stream_pipeline::{OverflowPolicy, PipelineBuilder, Stage, Result as PipelineResult};

/// Generator stage that produces numbers
struct NumberGenerator {
    current: u32,
    max: u32,
}

impl NumberGenerator {
    fn new(max: u32) -> Self {
        Self { current: 1, max }
    }
}

impl Stage for NumberGenerator {
    fn process(&mut self, _input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        if self.current <= self.max {
            let num = self.current;
            self.current += 1;
            Ok(vec![num.to_string().into_bytes()])
        } else {
            Ok(vec![])
        }
    }

    fn name(&self) -> &str {
        "generator"
    }
}

/// Filter stage that keeps only even numbers
struct EvenFilter;

impl Stage for EvenFilter {
    fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        let num_str = String::from_utf8_lossy(&input);
        if let Ok(num) = num_str.parse::<u32>() {
            if num % 2 == 0 {
                return Ok(vec![input]);
            }
        }
        Ok(vec![])
    }

    fn name(&self) -> &str {
        "even_filter"
    }
}

/// Transform stage that multiplies numbers by 10
struct MultiplyBy10;

impl Stage for MultiplyBy10 {
    fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        let num_str = String::from_utf8_lossy(&input);
        if let Ok(num) = num_str.parse::<u32>() {
            let result = (num * 10).to_string().into_bytes();
            Ok(vec![result])
        } else {
            Ok(vec![])
        }
    }

    fn name(&self) -> &str {
        "multiply_by_10"
    }
}

/// Aggregator stage that sums all numbers
struct SumAggregator {
    sum: u64,
    count: u64,
}

impl SumAggregator {
    fn new() -> Self {
        Self { sum: 0, count: 0 }
    }
}

impl Stage for SumAggregator {
    fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        let num_str = String::from_utf8_lossy(&input);
        if let Ok(num) = num_str.parse::<u64>() {
            self.sum += num;
            self.count += 1;

            if self.count % 5 == 0 {
                println!("Running sum: {} (count: {})", self.sum, self.count);
            }
        }
        Ok(vec![])
    }

    fn on_shutdown(&mut self) -> PipelineResult<()> {
        println!("\n=== Final Results ===");
        println!("Total numbers processed: {}", self.count);
        println!("Sum of all numbers: {}", self.sum);
        if self.count > 0 {
            println!("Average: {:.2}", self.sum as f64 / self.count as f64);
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "sum_aggregator"
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Number Processing Pipeline");
    println!("==========================");
    println!("Generating numbers 1-100, filtering evens, multiplying by 10, and summing");
    println!();

    let start = Instant::now();

    let pipeline = PipelineBuilder::new()
        .add_stage("generator", 50, OverflowPolicy::Block)
        .add_stage("filter", 50, OverflowPolicy::Block)
        .add_stage("transform", 50, OverflowPolicy::Block)
        .add_stage("aggregate", 50, OverflowPolicy::Block)
        .with_backpressure(true)
        .build()?;

    let input = pipeline.input();

    let running = pipeline.start(|stage_idx| match stage_idx {
        0 => Box::new(NumberGenerator::new(100)),
        1 => Box::new(EvenFilter),
        2 => Box::new(MultiplyBy10),
        3 => Box::new(SumAggregator::new()),
        _ => Box::new(EvenFilter),
    })?;

    // Trigger the generator by pushing an initial trigger message
    // The generator ignores input but uses it as a processing signal
    for _ in 0..100 {
        input.push(vec![0]).ok();
        std::thread::sleep(Duration::from_micros(100));
    }

    // Wait for pipeline to complete
    running.wait()?;

    let elapsed = start.elapsed();
    println!("\nPipeline execution time: {:.3}s", elapsed.as_secs_f64());

    Ok(())
}
