use crate::backpressure::BackpressureController;
use crate::buffer::{OverflowPolicy, RingBuffer};
use crate::error::{PipelineError, Result};
use crate::metrics::StageMetrics;
use crate::stage::{Stage, StageRunner};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

/// A stage configuration in the pipeline builder
struct PipelineStage {
    name: String,
    buffer_capacity: usize,
    overflow_policy: OverflowPolicy,
}

/// Builder for constructing pipelines
pub struct PipelineBuilder {
    stages: Vec<PipelineStage>,
    enable_backpressure: bool,
}

impl PipelineBuilder {
    /// Create a new pipeline builder
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            enable_backpressure: true,
        }
    }

    /// Add a stage to the pipeline
    pub fn add_stage(
        mut self,
        name: impl Into<String>,
        buffer_capacity: usize,
        overflow_policy: OverflowPolicy,
    ) -> Self {
        self.stages.push(PipelineStage {
            name: name.into(),
            buffer_capacity,
            overflow_policy,
        });
        self
    }

    /// Enable or disable backpressure
    pub fn with_backpressure(mut self, enable: bool) -> Self {
        self.enable_backpressure = enable;
        self
    }

    /// Build the pipeline
    pub fn build(self) -> Result<Pipeline> {
        if self.stages.is_empty() {
            return Err(PipelineError::NoStages);
        }

        let mut buffers = Vec::new();
        let mut metrics = Vec::new();

        // Create buffers and metrics for each stage
        for stage in &self.stages {
            buffers.push(RingBuffer::new(
                stage.buffer_capacity,
                stage.overflow_policy,
            ));
            metrics.push(StageMetrics::new());
        }

        Ok(Pipeline {
            stage_names: self.stages.iter().map(|s| s.name.clone()).collect(),
            buffers,
            metrics,
            handles: Vec::new(),
            backpressure_controller: if self.enable_backpressure {
                Some(BackpressureController::new())
            } else {
                None
            },
            is_running: Arc::new(AtomicBool::new(false)),
            shutdown_signals: Vec::new(),
        })
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A processing pipeline that chains stages together
pub struct Pipeline {
    stage_names: Vec<String>,
    buffers: Vec<RingBuffer<Vec<u8>>>,
    metrics: Vec<StageMetrics>,
    handles: Vec<JoinHandle<Result<()>>>,
    backpressure_controller: Option<BackpressureController>,
    is_running: Arc<AtomicBool>,
    shutdown_signals: Vec<Arc<AtomicBool>>,
}

impl Pipeline {
    /// Get the input buffer for the first stage
    pub fn input(&self) -> RingBuffer<Vec<u8>> {
        self.buffers[0].clone()
    }

    /// Get the output buffer of a specific stage
    pub fn stage_output(&self, index: usize) -> Option<RingBuffer<Vec<u8>>> {
        self.buffers.get(index).cloned()
    }

    /// Get metrics for a specific stage
    pub fn stage_metrics(&self, index: usize) -> Option<&StageMetrics> {
        self.metrics.get(index)
    }

    /// Get the backpressure controller if enabled
    pub fn backpressure(&self) -> Option<&BackpressureController> {
        self.backpressure_controller.as_ref()
    }

    /// Start the pipeline with a closure that creates stages
    pub fn start<F>(mut self, mut stage_factory: F) -> Result<RunningPipeline>
    where
        F: FnMut(usize) -> Box<dyn Stage>,
    {
        if self.is_running.load(Ordering::Relaxed) {
            return Err(PipelineError::AlreadyStarted);
        }

        self.is_running.store(true, Ordering::Relaxed);

        // Spawn a thread for each stage
        for stage_idx in 0..self.stage_names.len() {
            let input_buffer = self.buffers[stage_idx].clone();
            let output_buffer = if stage_idx + 1 < self.buffers.len() {
                self.buffers[stage_idx + 1].clone()
            } else {
                // Last stage: create a dummy buffer that's never used
                RingBuffer::new(10, OverflowPolicy::Block)
            };

            let stage = stage_factory(stage_idx);

            let mut runner = StageRunner::new(input_buffer.clone(), output_buffer.clone());
            let shutdown_signal = runner.shutdown_signal();
            self.shutdown_signals.push(shutdown_signal.clone());

            let metrics_clone = self.metrics[stage_idx].clone();
            let is_running = Arc::clone(&self.is_running);

            let handle = spawn(move || {
                // Run the stage
                let result = runner.run(stage);

                // Copy metrics
                let runner_metrics = runner.metrics();
                for _ in 0..runner_metrics.total_processed() {
                    metrics_clone.record_processed();
                }
                for _ in 0..runner_metrics.total_dropped() {
                    metrics_clone.record_dropped();
                }

                is_running.store(false, Ordering::Relaxed);
                result
            });

            self.handles.push(handle);
        }

        Ok(RunningPipeline {
            pipeline: self,
        })
    }

    /// Stop the pipeline
    pub fn stop(&mut self) -> Result<()> {
        // Signal all stages to shut down
        for signal in &self.shutdown_signals {
            signal.store(true, Ordering::Relaxed);
        }

        // Wait for all threads
        let mut results = Vec::new();
        for handle in self.handles.drain(..) {
            match handle.join() {
                Ok(result) => results.push(result),
                Err(_) => return Err(PipelineError::ThreadError("Join failed".into())),
            }
        }

        // Check for errors
        for result in results {
            result?;
        }

        self.is_running.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Check if pipeline is running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    /// Get all stage metrics
    pub fn all_metrics(&self) -> Vec<&StageMetrics> {
        self.metrics.iter().collect()
    }

    /// Get a summary of all metrics
    pub fn metrics_summary(&self) -> String {
        let mut summary = String::from("Pipeline Metrics Summary:\n");
        for (i, metrics) in self.metrics.iter().enumerate() {
            let snapshot = metrics.snapshot();
            summary.push_str(&format!(
                "  Stage {}: {}\n",
                i,
                snapshot.format()
            ));
        }
        summary
    }
}

/// A running pipeline that can be controlled and monitored
pub struct RunningPipeline {
    pipeline: Pipeline,
}

impl RunningPipeline {
    /// Get the input buffer
    pub fn input(&self) -> RingBuffer<Vec<u8>> {
        self.pipeline.input()
    }

    /// Get the output buffer of the last stage
    pub fn output(&self) -> Option<RingBuffer<Vec<u8>>> {
        let last_idx = self.pipeline.buffers.len() - 1;
        self.pipeline.stage_output(last_idx + 1)
    }

    /// Get metrics for a stage
    pub fn stage_metrics(&self, index: usize) -> Option<&StageMetrics> {
        self.pipeline.stage_metrics(index)
    }

    /// Wait for pipeline to complete
    pub fn wait(mut self) -> Result<()> {
        self.pipeline.stop()
    }

    /// Wait for pipeline to complete with a timeout
    pub fn wait_timeout(mut self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Force shutdown after timeout
        self.pipeline.stop()
    }

    /// Get metrics summary
    pub fn metrics_summary(&self) -> String {
        self.pipeline.metrics_summary()
    }

    /// Gracefully shutdown the pipeline
    pub fn shutdown(mut self) -> Result<()> {
        self.pipeline.stop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_builder() {
        let result = PipelineBuilder::new()
            .add_stage("stage1", 10, OverflowPolicy::Block)
            .add_stage("stage2", 10, OverflowPolicy::Block)
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_stages_error() {
        let result = PipelineBuilder::new().build();
        assert!(matches!(result, Err(PipelineError::NoStages)));
    }

    #[test]
    fn test_pipeline_input_buffer() {
        let pipeline = PipelineBuilder::new()
            .add_stage("stage1", 10, OverflowPolicy::Block)
            .build()
            .unwrap();
        let input = pipeline.input();
        assert!(input.is_empty());
    }
}
