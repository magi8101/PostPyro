use pyo3::prelude::*;
use std::time::Duration;

mod connection;
mod error;
mod pool;
mod row;
mod runtime;
mod transaction;
mod types;

use connection::PgConnection;
use error::{
    DataError, DatabaseError, IntegrityError, InterfaceError, InternalError, NotSupportedError,
    OperationalError, ProgrammingError,
};
use pool::ConnectionPool;
use row::Row;
use runtime::RuntimeManager;
use transaction::Transaction;

/// Temporary smoke test for the pyo3-asyncio <-> shared-runtime wiring.
/// Removed in Task 3 once real async methods exist to prove it instead.
#[pyfunction]
fn _test_async_bridge(py: Python) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(true)
    })
}

#[pymodule]
fn PostPyro(_py: Python, m: &PyModule) -> PyResult<()> {
    pyo3_asyncio::tokio::init_with_runtime(RuntimeManager::shared())
        .expect("pyo3-asyncio: failed to init shared Tokio runtime");

    m.add_class::<PgConnection>()?;
    m.add_class::<ConnectionPool>()?;
    m.add_class::<Row>()?;
    m.add_class::<Transaction>()?;
    m.add_function(wrap_pyfunction!(_test_async_bridge, m)?)?;

    m.add("DatabaseError", _py.get_type::<DatabaseError>())?;
    m.add("InterfaceError", _py.get_type::<InterfaceError>())?;
    m.add("DataError", _py.get_type::<DataError>())?;
    m.add("OperationalError", _py.get_type::<OperationalError>())?;
    m.add("IntegrityError", _py.get_type::<IntegrityError>())?;
    m.add("InternalError", _py.get_type::<InternalError>())?;
    m.add("ProgrammingError", _py.get_type::<ProgrammingError>())?;
    m.add("NotSupportedError", _py.get_type::<NotSupportedError>())?;

    m.add("__version__", "0.2.0")?;
    m.add("apilevel", "2.0")?;
    m.add("threadsafety", 2)?;
    m.add("paramstyle", "format")?;

    Ok(())
}