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

#[pymodule]
fn PostPyro(_py: Python, m: &PyModule) -> PyResult<()> {
    pyo3_asyncio::tokio::init_with_runtime(RuntimeManager::shared()).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "pyo3-asyncio: failed to init shared Tokio runtime (already initialized?)",
        )
    })?;

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

    m.add("__version__", "2.0.0-dev")?;
    m.add("apilevel", "2.0")?;
    m.add("threadsafety", 2)?;
    m.add("paramstyle", "format")?;

    Ok(())
}
