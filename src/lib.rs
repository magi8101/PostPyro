use pyo3::prelude::*;

// Import all our modules
mod runtime;
mod error;
mod types;
mod connection;
mod row;
mod transaction;

// Re-export for use in lib.rs
use connection::PgConnection;
use error::*;
use row::Row;
use transaction::Transaction;

/// High-performance PostgreSQL driver for Python using PyO3 and tokio-postgres
///
/// This module provides a complete PostgreSQL database driver that wraps
/// tokio-postgres with PyO3 bindings for high performance and full async support.
///
/// ## Basic Usage
///
/// ```python
/// import PostPyro as pg
///
/// # Connect to database
/// conn = pg.connect("postgresql://user:pass@localhost/dbname")
///
/// # Execute queries
/// conn.execute("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT)")
/// conn.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
///
/// # Query data
/// rows = conn.query("SELECT * FROM users WHERE name = $1", ["Alice"])
/// for row in rows:
///     print(f"ID: {row['id']}, Name: {row['name']}")
///
/// # Use transactions
/// with conn.begin() as txn:
///     txn.execute("INSERT INTO users (name) VALUES ($1)", ["Bob"])
///     txn.commit()  # or automatic rollback on exception
///
/// conn.close()
/// ```
///
/// ## Features
///
/// - Full DB-API 2.0 compliance
/// - High performance with Rust backend
/// - Async I/O with Tokio runtime
/// - Type-safe parameter binding
/// - Comprehensive error handling
/// - Connection pooling support
/// - Transaction management with savepoints
/// - Support for all PostgreSQL data types
#[pymodule]
fn pypg_driver(_py: Python, m: &PyModule) -> PyResult<()> {
    // Add version information
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // DB-API 2.0 compliance attributes
    m.add("apilevel", "2.0")?;
    m.add("threadsafety", 2)?;  // Threads may share the module and connections
    m.add("paramstyle", "numeric")?;  // PostgreSQL uses $1, $2, $3 placeholders

    // Add classes
    m.add_class::<PgConnection>()?;
    m.add_class::<Row>()?;
    m.add_class::<Transaction>()?;

    // Add exception types
    m.add("DatabaseError", _py.get_type::<DatabaseError>())?;
    m.add("InterfaceError", _py.get_type::<InterfaceError>())?;
    m.add("DataError", _py.get_type::<DataError>())?;
    m.add("OperationalError", _py.get_type::<OperationalError>())?;
    m.add("IntegrityError", _py.get_type::<IntegrityError>())?;
    m.add("InternalError", _py.get_type::<InternalError>())?;
    m.add("ProgrammingError", _py.get_type::<ProgrammingError>())?;
    m.add("NotSupportedError", _py.get_type::<NotSupportedError>())?;

    // Add module-level functions
    m.add_function(wrap_pyfunction!(connect, m)?)?;
    m.add_function(wrap_pyfunction!(get_version, m)?)?;

    Ok(())
}

/// Connect to a PostgreSQL database
///
/// This is a convenience function that creates a new Connection instance.
///
/// Args:
///     connection_string: PostgreSQL connection string
///         Format: postgresql://user:password@host:port/database?options
///
/// Returns:
///     Connection: New database connection
///
/// Raises:
///     InterfaceError: If connection fails
///
/// Example:
///     >>> conn = PostPyro.connect("postgresql://user:pass@localhost/dbname")
#[pyfunction]
fn connect(connection_string: &str) -> PyResult<PgConnection> {
    PgConnection::new(connection_string)
}

/// Get the driver version
///
/// Returns:
///     str: Version string
///
/// Example:
///     >>> version = PostPyro.get_version()
///     >>> print(version)
///     '0.1.0'
#[pyfunction]
fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}