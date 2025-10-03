use pyo3::prelude::*;

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
use transaction::Transaction;

#[pymodule]
fn PostPyro(_py: Python, m: &PyModule) -> PyResult<()> {
    // Classes
    m.add_class::<PgConnection>()?;
    m.add_class::<ConnectionPool>()?;
    m.add_class::<Row>()?;
    m.add_class::<Transaction>()?;

    // Exceptions (DB-API 2.0 compliant)
    m.add("DatabaseError", _py.get_type::<DatabaseError>())?;
    m.add("InterfaceError", _py.get_type::<InterfaceError>())?;
    m.add("DataError", _py.get_type::<DataError>())?;
    m.add("OperationalError", _py.get_type::<OperationalError>())?;
    m.add("IntegrityError", _py.get_type::<IntegrityError>())?;
    m.add("InternalError", _py.get_type::<InternalError>())?;
    m.add("ProgrammingError", _py.get_type::<ProgrammingError>())?;
    m.add("NotSupportedError", _py.get_type::<NotSupportedError>())?;

    // Constants (DB-API 2.0)
    m.add("__version__", "0.2.0")?;
    m.add("apilevel", "2.0")?;
    m.add("threadsafety", 2)?;
    m.add("paramstyle", "format")?;

    Ok(())
}