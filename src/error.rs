use thiserror::Error;

/// Result type for stream pipeline operations
pub type Result<T> = std::result::Result<T, PipelineError>;

/// Errors that can occur during pipeline execution
#[derive(Error, Debug)]
pub enum PipelineError {
    /// Pipeline has already been started
    #[error("Pipeline has already been started")]
    AlreadyStarted,

    /// No stages in pipeline
    #[error("Cannot start pipeline with no stages")]
    NoStages,

    /// Stage execution error
    #[error("Stage execution failed: {0}")]
    StageError(String),

    /// Thread join error
    #[error("Thread join error: {0}")]
    ThreadError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Shutdown error
    #[error("Pipeline shutdown error: {0}")]
    ShutdownError(String),
}

