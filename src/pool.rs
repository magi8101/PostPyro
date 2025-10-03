use std::sync::Arc;
use pyo3::prelude::*;
use pyo3::types::PyList;
use tokio_postgres::{NoTls, Config};
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};

use crate::error::map_db_error;
use crate::row::Row;
use crate::runtime::RuntimeManager;
use crate::types::py_objects_to_postgres_values;

/// High-performance connection pool for managing database connections
#[pyclass(name = "ConnectionPool")]
pub struct ConnectionPool {
    pool: Arc<Pool>,
    runtime: RuntimeManager,
}

#[pymethods]
impl ConnectionPool {
    /// Create a new connection pool
    ///
    /// Args:
    ///     connection_string: PostgreSQL connection string
    ///     max_size: Maximum number of connections in pool (default: 10)
    ///     min_size: Minimum number of connections in pool (default: 0)
    ///
    /// Returns:
    ///     ConnectionPool: New connection pool
    ///
    /// Raises:
    ///     InterfaceError: If pool creation fails
    #[new]
    #[pyo3(signature = (connection_string, max_size=10, min_size=0))]
    pub fn new(connection_string: &str, max_size: usize, min_size: usize) -> PyResult<Self> {
        let runtime = RuntimeManager::new();

        // Parse connection string
        let config: Config = connection_string.parse().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid connection string: {}", e))
        })?;

        // Create pool
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(config, NoTls, mgr_config);
        
        let pool = runtime.block_on(async {
            Pool::builder(mgr)
                .max_size(max_size)
                .build()
                .map_err(|e| {
                    pyo3::exceptions::PyConnectionError::new_err(format!("Pool creation error: {}", e))
                })
        })?;

        Ok(Self {
            pool: Arc::new(pool),
            runtime,
        })
    }

    /// Execute a query that doesn't return rows
    ///
    /// Args:
    ///     query: SQL query string
    ///     params: Query parameters (optional)
    ///
    /// Returns:
    ///     int: Number of rows affected
    pub fn execute(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<u64> {
        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let pool = Arc::clone(&self.pool);
        let query = query.to_string();

        self.runtime.block_on(async move {
            let client = pool.get().await.map_err(|e| {
                pyo3::exceptions::PyConnectionError::new_err(format!("Failed to get connection: {}", e))
            })?;

            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p.as_ref() as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.execute(&query, &params_refs[..])
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
    pub fn query(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<PyObject> {
        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let pool = Arc::clone(&self.pool);
        let query = query.to_string();

        let rows = self.runtime.block_on(async move {
            let client = pool.get().await.map_err(|e| {
                pyo3::exceptions::PyConnectionError::new_err(format!("Failed to get connection: {}", e))
            })?;

            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p.as_ref() as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.query(&query, &params_refs[..])
                .await
                .map_err(map_db_error)
        })?;

        let py_rows = if rows.len() < 100 {
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                result.push(Row::from_tokio_row(py, &row)?);
            }
            result
        } else {
            Row::from_tokio_rows(py, &rows)?
        };

        Ok(py_rows.into_py(py))
    }

    /// Execute a query and return exactly one row
    ///
    /// Args:
    ///     query: SQL query string
    ///     params: Query parameters (optional)
    ///
    /// Returns:
    ///     Row: Single row result
    pub fn query_one(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<Py<Row>> {
        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let pool = Arc::clone(&self.pool);
        let query = query.to_string();

        let row = self.runtime.block_on(async move {
            let client = pool.get().await.map_err(|e| {
                pyo3::exceptions::PyConnectionError::new_err(format!("Failed to get connection: {}", e))
            })?;

            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p.as_ref() as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.query_one(&query, &params_refs[..])
                .await
                .map_err(map_db_error)
        })?;

        let row_obj = Row::from_tokio_row(py, &row)?;
        Ok(Py::new(py, row_obj)?)
    }

    /// Get pool status information
    ///
    /// Returns:
    ///     dict: Dictionary with pool statistics
    pub fn status(&self, py: Python) -> PyResult<PyObject> {
        let status = self.pool.status();
        let info = pyo3::types::PyDict::new(py);
        info.set_item("size", status.size)?;
        info.set_item("available", status.available)?;
        info.set_item("max_size", status.max_size)?;
        Ok(info.to_object(py))
    }
}