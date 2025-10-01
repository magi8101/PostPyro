use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls, Statement};
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::error::{map_db_error, connection_closed_error, invalid_connection_string_error};
use crate::runtime::RuntimeManager;
use crate::types::py_objects_to_postgres_values;
use crate::row::Row;

/// PostgreSQL database connection
#[pyclass(name = "Connection")]
pub struct PgConnection {
    client: Arc<Mutex<Client>>,
    runtime: RuntimeManager,
    is_closed: Arc<Mutex<bool>>,
    prepared_statements: Arc<Mutex<HashMap<String, Statement>>>,
}

#[pymethods]
impl PgConnection {
    /// Create a new database connection
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
    #[new]
    pub fn new(connection_string: &str) -> PyResult<Self> {
        let runtime = RuntimeManager::new();

        // Parse connection string (basic validation)
        if !connection_string.starts_with("postgresql://") && !connection_string.starts_with("postgres://") {
            return Err(invalid_connection_string_error("Must start with 'postgresql://' or 'postgres://'"));
        }

        // Create connection in async context
        let (client, connection) = runtime.block_on(async {
            tokio_postgres::connect(connection_string, NoTls)
                .await
                .map_err(map_db_error)
        })?;

        let client = Arc::new(Mutex::new(client));
        let is_closed = Arc::new(Mutex::new(false));
        let prepared_statements = Arc::new(Mutex::new(HashMap::new()));

        // Spawn connection handler as background task
        let _client_clone = Arc::clone(&client);
        let is_closed_clone = Arc::clone(&is_closed);
        runtime.spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Connection error: {}", e);
                // Mark connection as closed on error
                if let Ok(mut closed) = is_closed_clone.try_lock() {
                    *closed = true;
                }
            }
        });

        Ok(Self {
            client,
            runtime,
            is_closed,
            prepared_statements,
        })
    }

    /// Execute a query that doesn't return rows (INSERT, UPDATE, DELETE)
    ///
    /// Args:
    ///     query: SQL query string
    ///     params: Query parameters (optional)
    ///
    /// Returns:
    ///     int: Number of rows affected
    ///
    /// Raises:
    ///     InterfaceError: If connection is closed
    ///     ProgrammingError: If query has syntax errors
    ///     DatabaseError: For other database errors
    pub fn execute(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<u64> {
        self.check_connection()?;

        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let client: Arc<Mutex<Client>> = Arc::clone(&self.client);
        self.runtime.block_on(async move {
            let client = client.lock().await;
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.execute(query, &params_refs[..])
                .await
                .map_err(map_db_error)
        })
    }

    /// Execute a query and return all rows
    ///
    /// Args:
    ///     query: SQL query string
    ///     params: Query parameters (optional)
    ///
    /// Returns:
    ///     list: List of Row objects
    ///
    /// Raises:
    ///     InterfaceError: If connection is closed
    ///     ProgrammingError: If query has syntax errors
    ///     DatabaseError: For other database errors
    pub fn query(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<PyObject> {
        self.check_connection()?;

        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let client: Arc<Mutex<Client>> = Arc::clone(&self.client);
        let rows = self.runtime.block_on(async move {
            let client = client.lock().await;
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.query(query, &params_refs[..])
                .await
                .map_err(map_db_error)
        })?;

        // Convert to Python Row objects
        let py_rows = PyList::empty(py);
        for row in rows {
            let py_row = Row::from_tokio_row(py, &row)?;
            py_rows.append(py_row)?;
        }

        Ok(py_rows.to_object(py))
    }

    /// Execute a query and return exactly one row
    ///
    /// Args:
    ///     query: SQL query string
    ///     params: Query parameters (optional)
    ///
    /// Returns:
    ///     Row: Single row result
    ///
    /// Raises:
    ///     InterfaceError: If connection is closed
    ///     ProgrammingError: If query has syntax errors or returns != 1 row
    ///     DatabaseError: For other database errors
    pub fn query_one(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<Py<Row>> {
        self.check_connection()?;

        let postgres_params = if let Some(p) = params {
            let params_slice: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_slice)?
        } else {
            Vec::new()
        };

        let client: Arc<Mutex<Client>> = Arc::clone(&self.client);
        let row = self.runtime.block_on(async move {
            let client = client.lock().await;
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.query_one(query, &params_refs[..])
                .await
                .map_err(map_db_error)
        })?;

        Row::from_tokio_row(py, &row)
    }

    /// Prepare a statement for repeated execution
    ///
    /// Args:
    ///     query: SQL query string
    ///
    /// Returns:
    ///     str: Statement name/handle
    ///
    /// Raises:
    ///     InterfaceError: If connection is closed
    ///     ProgrammingError: If query has syntax errors
    pub fn prepare(&self, query: &str) -> PyResult<String> {
        self.check_connection()?;

        let client: Arc<Mutex<Client>> = Arc::clone(&self.client);
        let prepared_statements: Arc<Mutex<HashMap<String, Statement>>> = Arc::clone(&self.prepared_statements);

        // Use query as the key for simplicity
        let statement_name = query.to_string();

        self.runtime.block_on(async move {
            let client = client.lock().await;
            let statement = client.prepare(query).await.map_err(map_db_error)?;

            let mut statements = prepared_statements.lock().await;
            statements.insert(statement_name.clone(), statement);

            Ok(statement_name)
        })
    }

    /// Close the database connection
    pub fn close(&self) -> PyResult<()> {
        let mut is_closed = self.is_closed.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Connection is busy")
        })?;
        *is_closed = true;
        Ok(())
    }

    /// Check if the connection is closed
    ///
    /// Returns:
    ///     bool: True if connection is closed
    pub fn is_closed(&self) -> PyResult<bool> {
        Ok(*self.is_closed.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Connection state check failed")
        })?)
    }

    /// Test the connection with a simple query
    ///
    /// Returns:
    ///     bool: True if connection is healthy
    pub fn ping(&self, py: Python) -> PyResult<bool> {
        match self.execute(py, "SELECT 1", None) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Get connection information
    ///
    /// Returns:
    ///     dict: Dictionary with connection details
    pub fn info(&self, py: Python) -> PyResult<PyObject> {
        let info = pyo3::types::PyDict::new(py);
        info.set_item("closed", self.is_closed()?)?;
        info.set_item("healthy", self.ping(py)?)?;
        Ok(info.to_object(py))
    }

    /// Context manager entry
    fn __enter__(&self, _py: Python) -> PyResult<Self> {
        Ok(Self {
            client: Arc::clone(&self.client),
            runtime: self.runtime.clone(),
            is_closed: Arc::clone(&self.is_closed),
            prepared_statements: Arc::clone(&self.prepared_statements),
        })
    }

    /// Begin a new transaction (placeholder - use manual BEGIN/COMMIT for now)
    ///
    /// Note: Due to lifetime constraints with tokio-postgres transactions,
    /// use manual transaction management with BEGIN/COMMIT/ROLLBACK statements.
    ///
    /// Returns:
    ///     None: Use manual SQL transaction commands
    ///
    /// Raises:
    ///     NotSupportedError: Feature requires manual transaction management
    pub fn begin(&self, _isolation_level: Option<&str>, _read_only: Option<bool>) -> PyResult<()> {
        Err(crate::error::not_supported_error(
            "Auto-managed transactions - use BEGIN/COMMIT SQL statements or connection context manager"
        ))
    }

    /// Execute multiple statements in a transaction using manual SQL
    ///
    /// Args:
    ///     queries: List of SQL query strings
    ///
    /// Returns:
    ///     list: List of results for each query
    ///
    /// Raises:
    ///     InterfaceError: If connection is closed
    ///     DatabaseError: If any query fails (transaction is rolled back)
    pub fn execute_batch(&self, py: Python, queries: &PyList) -> PyResult<PyObject> {
        self.check_connection()?;

        // Start transaction
        self.execute(py, "BEGIN", None)?;
        let mut results = Vec::new();

        // Execute all queries
        for query_obj in queries {
            let query = query_obj.extract::<String>()?;
            match self.execute(py, &query, None) {
                Ok(result) => results.push(result.to_object(py)),
                Err(e) => {
                    // Rollback on error
                    let _ = self.execute(py, "ROLLBACK", None);
                    return Err(e);
                }
            }
        }

        // Commit transaction
        self.execute(py, "COMMIT", None)?;
        Ok(PyList::new(py, results).to_object(py))
    }

    /// Context manager exit
    fn __exit__(&self, _py: Python, _exc_type: Option<PyObject>, _exc_val: Option<PyObject>, _exc_tb: Option<PyObject>) -> PyResult<()> {
        let _ = self.close();
        Ok(())
    }
}

impl PgConnection {
    /// Check if connection is still active
    fn check_connection(&self) -> PyResult<()> {
        if *self.is_closed.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Connection state check failed")
        })? {
            Err(connection_closed_error())
        } else {
            Ok(())
        }
    }
}