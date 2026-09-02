use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Runtime::new().expect("Failed to create Tokio runtime")
});

pub struct RuntimeManager;

impl RuntimeManager {
    /// The process-wide Tokio runtime, shared with pyo3-asyncio and sqlx.
    pub fn shared() -> &'static Runtime {
        &RUNTIME
    }
}
