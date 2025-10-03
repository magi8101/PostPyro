use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::runtime::Runtime;

static RUNTIME: Lazy<Arc<Runtime>> = Lazy::new(|| {
    Arc::new(
        tokio::runtime::Runtime::new()
            .expect("Failed to create Tokio runtime")
    )
});

#[derive(Clone)]
pub struct RuntimeManager {
    runtime: Arc<Runtime>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            runtime: RUNTIME.clone(),
        }
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
}