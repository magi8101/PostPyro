use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Runtime::new().expect("Failed to create Tokio runtime")
});

#[derive(Clone, Copy)]
pub struct RuntimeManager {
    runtime: &'static Runtime,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self { runtime: &RUNTIME }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        self.runtime.block_on(future)
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }

    /// The process-wide Tokio runtime, shared with pyo3-asyncio and sqlx.
    pub fn shared() -> &'static Runtime {
        &RUNTIME
    }
}