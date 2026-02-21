use crate::buffer::RingBuffer;
use crate::error::Result;
use crate::metrics::StageMetrics;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Trait for a processing stage in the pipeline
pub trait Stage: Send + 'static {
    /// Process an input item and produce output(s)
    /// Can produce 0, 1, or multiple outputs
    fn process(&mut self, input: Vec<u8>) -> Result<Vec<Vec<u8>>>;

    /// Called before the stage starts processing
    fn on_start(&mut self) -> Result<()> {
        Ok(())
    }

    /// Called when the stage is shutting down
    fn on_shutdown(&mut self) -> Result<()> {
        Ok(())
    }

    /// Get a human-readable name for this stage
    fn name(&self) -> &str {
        "stage"
    }
}

/// Runs a stage by pulling from input buffer, processing, and pushing to output buffer
pub struct StageRunner {
    input: RingBuffer<Vec<u8>>,
    output: RingBuffer<Vec<u8>>,
    metrics: StageMetrics,
    shutdown: Arc<AtomicBool>,
}

impl StageRunner {
    /// Create a new stage runner
    pub fn new(
        input: RingBuffer<Vec<u8>>,
        output: RingBuffer<Vec<u8>>,
    ) -> Self {
        Self {
            input,
            output,
            metrics: StageMetrics::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a reference to the metrics
    pub fn metrics(&self) -> &StageMetrics {
        &self.metrics
    }

    /// Get the shutdown signal
    pub fn shutdown_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Run the stage with the provided implementation
    /// This method blocks until shutdown is signaled
    pub fn run(&mut self, mut stage: Box<dyn Stage>) -> Result<()> {
        stage.on_start()?;

        while !self.shutdown.load(Ordering::Relaxed) {
            // Try to get an item from input
            if let Some(item) = self.input.pop() {
                let start = Instant::now();

                // Process the item
                match stage.process(item) {
                    Ok(outputs) => {
                        let elapsed = start.elapsed().as_nanos() as u64;
                        self.metrics.record_latency(elapsed);

                        // Push outputs to next stage
                        for output in outputs {
                            match self.output.push(output) {
                                Ok(()) => {
                                    self.metrics.record_processed();
                                }
                                Err(_) => {
                                    self.metrics.record_dropped();
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Stage {} error: {}", stage.name(), e);
                    }
                }
            } else {
                // No item available, sleep briefly to reduce CPU usage
                std::thread::sleep(std::time::Duration::from_micros(10));
            }
        }

        stage.on_shutdown()?;
        Ok(())
    }

    /// Signal the stage to shut down
    pub fn signal_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// A simple pass-through stage for testing
#[derive(Debug)]
pub struct PassthroughStage;

impl Stage for PassthroughStage {
    fn process(&mut self, input: Vec<u8>) -> Result<Vec<Vec<u8>>> {
        Ok(vec![input])
    }

    fn name(&self) -> &str {
        "passthrough"
    }
}

/// A filtering stage that passes through items matching a predicate
#[derive(Debug)]
pub struct FilterStage<F>
where
    F: Fn(&[u8]) -> bool + Send + 'static,
{
    name: String,
    predicate: F,
}

impl<F> FilterStage<F>
where
    F: Fn(&[u8]) -> bool + Send + 'static,
{
    /// Create a new filter stage
    pub fn new(name: impl Into<String>, predicate: F) -> Self {
        Self {
            name: name.into(),
            predicate,
        }
    }
}

impl<F> Stage for FilterStage<F>
where
    F: Fn(&[u8]) -> bool + Send + 'static,
{
    fn process(&mut self, input: Vec<u8>) -> Result<Vec<Vec<u8>>> {
        if (self.predicate)(&input) {
            Ok(vec![input])
        } else {
            Ok(vec![])
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A mapping stage that transforms each item
#[derive(Debug)]
pub struct MapStage<F>
where
    F: Fn(Vec<u8>) -> Result<Vec<u8>> + Send + 'static,
{
    name: String,
    mapper: F,
}

impl<F> MapStage<F>
where
    F: Fn(Vec<u8>) -> Result<Vec<u8>> + Send + 'static,
{
    /// Create a new map stage
    pub fn new(name: impl Into<String>, mapper: F) -> Self {
        Self {
            name: name.into(),
            mapper,
        }
    }
}

impl<F> Stage for MapStage<F>
where
    F: Fn(Vec<u8>) -> Result<Vec<u8>> + Send + 'static,
{
    fn process(&mut self, input: Vec<u8>) -> Result<Vec<Vec<u8>>> {
        Ok(vec![(self.mapper)(input)?])
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::OverflowPolicy;

    #[test]
    fn test_passthrough_stage() {
        let mut stage = PassthroughStage;
        let input = vec![1, 2, 3];
        let output = stage.process(input.clone()).unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], input);
    }

    #[test]
    fn test_filter_stage() {
        let mut stage = FilterStage::new("test_filter", |data| data[0] > 5);
        assert_eq!(stage.process(vec![3]).unwrap().len(), 0);
        assert_eq!(stage.process(vec![7]).unwrap().len(), 1);
    }

    #[test]
    fn test_map_stage() {
        let mut stage = MapStage::new("test_map", |mut data| {
            data[0] *= 2;
            Ok(data)
        });
        let output = stage.process(vec![5]).unwrap();
        assert_eq!(output[0][0], 10);
    }

    #[test]
    fn test_stage_runner_metrics() {
        let input = RingBuffer::new(10, OverflowPolicy::Block);
        let output = RingBuffer::new(10, OverflowPolicy::Block);
        let runner = StageRunner::new(input.clone(), output.clone());

        assert_eq!(runner.metrics().total_processed(), 0);
    }
}
