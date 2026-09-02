// The crate/module/function name must be exactly `PostPyro` - it's the
// Python import name (`import PostPyro`), which PyO3's #[pymodule] requires
// to match the function name verbatim.
#![allow(non_snake_case)]

use pyo3::prelude::*;

mod error;
mod pool;
mod row;
mod runtime;
mod transaction;
mod types;

use error::{
    DataError, DatabaseError, IntegrityError, InterfaceError, InternalError, NotSupportedError,
    OperationalError, ProgrammingError,
};
use pool::{connect, Pool};
use row::Row;
use runtime::RuntimeManager;
use transaction::Transaction;

/// Best-effort safety net for process exit without an explicit `Pool.close()`:
/// gives any `Transaction::drop`-spawned background cleanup task (rollback +
/// return connection to the pool - see that Drop impl) a scheduling window to
/// finish before Python proceeds further into interpreter shutdown, where
/// the Tokio runtime being torn down mid-task could otherwise abort the
/// process. `Pool.close()` does the same drain directly; this covers the
/// case where `close()` is never called at all.
#[pyfunction]
fn _drain_background_cleanup_on_exit() {
    RuntimeManager::shared().block_on(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    });
}

#[pymodule]
fn PostPyro(_py: Python, m: &PyModule) -> PyResult<()> {
    pyo3_asyncio::tokio::init_with_runtime(RuntimeManager::shared()).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "pyo3-asyncio: failed to init shared Tokio runtime (already initialized?)",
        )
    })?;

    _py.import("atexit")?.call_method1(
        "register",
        (wrap_pyfunction!(_drain_background_cleanup_on_exit, m)?,),
    )?;

    m.add_class::<Pool>()?;
    m.add_class::<Row>()?;
    m.add_class::<Transaction>()?;
    m.add_function(wrap_pyfunction!(connect, m)?)?;

    m.add("DatabaseError", _py.get_type::<DatabaseError>())?;
    m.add("InterfaceError", _py.get_type::<InterfaceError>())?;
    m.add("DataError", _py.get_type::<DataError>())?;
    m.add("OperationalError", _py.get_type::<OperationalError>())?;
    m.add("IntegrityError", _py.get_type::<IntegrityError>())?;
    m.add("InternalError", _py.get_type::<InternalError>())?;
    m.add("ProgrammingError", _py.get_type::<ProgrammingError>())?;
    m.add("NotSupportedError", _py.get_type::<NotSupportedError>())?;

    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("apilevel", "2.0")?;
    m.add("threadsafety", 2)?;
    // Placeholders are $1, $2, ... (PEP 249 "numeric"), not "format" (%s).
    m.add("paramstyle", "numeric")?;

    Ok(())
}
