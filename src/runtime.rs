use std::sync::Arc;
use once_cell::sync::Lazy;
use tokio::runtime::{Builder, Runtime};

/// Global Tokio runtime instance shared across all connections
static RUNTIME: Lazy<Arc<Runtime>> = Lazy::new(|| {
    Arc::new(
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("pypg-driver")
            .build()
            .expect("Failed to create Tokio runtime"),
    )
});

/// Runtime manager for handling async operations in a synchronous context
pub struct RuntimeManager {
    runtime: Arc<Runtime>,
}

impl RuntimeManager {
    /// Create a new runtime manager with the global runtime
    pub fn new() -> Self {
        Self {
            runtime: Arc::clone(&RUNTIME),
        }
    }

    /// Execute an async function synchronously, blocking the current thread
    /// This is the primary method for bridging async PostgreSQL operations to Python
    pub fn block_on<F, T>(&self, future: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        self.runtime.block_on(future)
    }

    /// Spawn a background task on the runtime
    /// Useful for connection maintenance or background operations
    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }

    /// Get a reference to the underlying runtime
    /// For advanced use cases that need direct runtime access
    pub fn runtime(&self) -> &Arc<Runtime> {
        &self.runtime
    }
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RuntimeManager {
    fn clone(&self) -> Self {
        Self {
            runtime: Arc::clone(&self.runtime),
        }
    }
}