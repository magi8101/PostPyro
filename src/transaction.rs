use pyo3::prelude::*;
use pyo3::types::PyList;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Client;

use crate::error::{map_db_error, transaction_completed_error};
use crate::row::Row;
use crate::runtime::RuntimeManager;
use crate::types::py_objects_to_postgres_values;

/// Represents a database transaction using manual SQL commands
/// This avoids lifetime issues with tokio_postgres::Transaction
#[pyclass]
pub struct Transaction {
    client: Arc<Mutex<Client>>,
    runtime: RuntimeManager,
    is_completed: Arc<Mutex<bool>>,
}

#[derive(Debug, Clone, Copy)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

impl IsolationLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "READ UNCOMMITTED" => Some(Self::ReadUncommitted),
            "READ COMMITTED" => Some(Self::ReadCommitted),
            "REPEATABLE READ" => Some(Self::RepeatableRead),
            "SERIALIZABLE" => Some(Self::Serializable),
            _ => None,
        }
    }

    pub fn to_sql(&self) -> &'static str {
        match self {
            Self::ReadUncommitted => "READ UNCOMMITTED",
            Self::ReadCommitted => "READ COMMITTED",
            Self::RepeatableRead => "REPEATABLE READ",
            Self::Serializable => "SERIALIZABLE",
        }
    }
}

#[pymethods]
impl Transaction {
    /// Execute a query within the transaction that doesn't return rows
    pub fn execute(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<u64> {
        self.check_active()?;

        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let client = Arc::clone(&self.client);
        self.runtime.block_on(async move {
            let client = client.lock().await;
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p.as_ref() as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.execute(query, &params_refs[..]).await.map_err(map_db_error)
        })
    }

    /// Execute a query within the transaction and return all rows
    pub fn query(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<PyObject> {
        self.check_active()?;

        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let client = Arc::clone(&self.client);
        let rows = self.runtime.block_on(async move {
            let client = client.lock().await;
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p.as_ref() as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.query(query, &params_refs[..]).await.map_err(map_db_error)
        })?;

        let py_rows = PyList::empty(py);
        for row in rows {
            let py_row = Row::from_tokio_row(py, &row)?;
            let py_cell = Py::new(py, py_row)?;
            py_rows.append(py_cell)?;
        }

        Ok(py_rows.to_object(py))
    }

    /// Execute a query within the transaction and return exactly one row
    pub fn query_one(&self, py: Python, query: &str, params: Option<&PyList>) -> PyResult<Py<Row>> {
        self.check_active()?;

        let postgres_params = if let Some(p) = params {
            let params_vec: Vec<PyObject> = p.iter().map(|item| item.into()).collect();
            py_objects_to_postgres_values(py, &params_vec)?
        } else {
            Vec::new()
        };

        let client = Arc::clone(&self.client);
        let row = self.runtime.block_on(async move {
            let client = client.lock().await;
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p.as_ref() as &(dyn postgres_types::ToSql + Sync))
                .collect();

            client.query_one(query, &params_refs[..])
                .await
                .map_err(map_db_error)
        })?;

        let row_obj = Row::from_tokio_row(py, &row)?;
        Ok(Py::new(py, row_obj)?)
    }

    /// Commit the transaction
    pub fn commit(&self) -> PyResult<()> {
        self.check_active()?;

        let client = Arc::clone(&self.client);
        let is_completed = Arc::clone(&self.is_completed);

        self.runtime.block_on(async move {
            let client = client.lock().await;
            client.batch_execute("COMMIT").await.map_err(map_db_error)?;

            let mut completed = is_completed.lock().await;
            *completed = true;

            Ok(())
        })
    }

    /// Roll back the transaction
    pub fn rollback(&self) -> PyResult<()> {
        self.check_active()?;

        let client = Arc::clone(&self.client);
        let is_completed = Arc::clone(&self.is_completed);

        self.runtime.block_on(async move {
            let client = client.lock().await;
            client.batch_execute("ROLLBACK").await.map_err(map_db_error)?;

            let mut completed = is_completed.lock().await;
            *completed = true;

            Ok(())
        })
    }

    /// Create a savepoint within the transaction
    pub fn savepoint(&self, name: &str) -> PyResult<()> {
        self.check_active()?;

        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') || name.is_empty() {
            return Err(crate::error::type_conversion_error(
                "valid SQL identifier",
                name,
            ));
        }

        let client = Arc::clone(&self.client);
        let sql = format!("SAVEPOINT {}", name);
        
        self.runtime.block_on(async move {
            let client = client.lock().await;
            client.batch_execute(&sql).await.map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Roll back to a savepoint
    pub fn rollback_to(&self, name: &str) -> PyResult<()> {
        self.check_active()?;

        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') || name.is_empty() {
            return Err(crate::error::type_conversion_error(
                "valid SQL identifier",
                name,
            ));
        }

        let client = Arc::clone(&self.client);
        let sql = format!("ROLLBACK TO SAVEPOINT {}", name);

        self.runtime.block_on(async move {
            let client = client.lock().await;
            client.batch_execute(&sql).await.map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Release a savepoint
    pub fn release_savepoint(&self, name: &str) -> PyResult<()> {
        self.check_active()?;

        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') || name.is_empty() {
            return Err(crate::error::type_conversion_error(
                "valid SQL identifier",
                name,
            ));
        }

        let client = Arc::clone(&self.client);
        let sql = format!("RELEASE SAVEPOINT {}", name);

        self.runtime.block_on(async move {
            let client = client.lock().await;
            client.batch_execute(&sql).await.map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Set the transaction isolation level
    pub fn set_isolation_level(&self, level: &str) -> PyResult<()> {
        self.check_active()?;

        let isolation_level = IsolationLevel::from_str(level)
            .ok_or_else(|| crate::error::type_conversion_error("valid isolation level", level))?;

        let client = Arc::clone(&self.client);
        let sql = format!(
            "SET TRANSACTION ISOLATION LEVEL {}",
            isolation_level.to_sql()
        );

        self.runtime.block_on(async move {
            let client = client.lock().await;
            client.batch_execute(&sql).await.map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Set whether the transaction is read-only
    pub fn set_read_only(&self, read_only: bool) -> PyResult<()> {
        self.check_active()?;

        let client = Arc::clone(&self.client);
        let sql = if read_only {
            "SET TRANSACTION READ ONLY"
        } else {
            "SET TRANSACTION READ WRITE"
        };

        self.runtime.block_on(async move {
            let client = client.lock().await;
            client.batch_execute(sql).await.map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Check if the transaction is still active
    pub fn is_active(&self) -> PyResult<bool> {
        let is_completed = self.is_completed.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Transaction state check failed")
        })?;
        Ok(!*is_completed)
    }

    /// Context manager entry
    fn __enter__(&self, _py: Python) -> PyResult<Self> {
        Ok(Self {
            client: Arc::clone(&self.client),
            runtime: self.runtime.clone(),
            is_completed: Arc::clone(&self.is_completed),
        })
    }

    /// Context manager exit
    fn __exit__(
        &self,
        _py: Python,
        exc_type: Option<PyObject>,
        _exc_val: Option<PyObject>,
        _exc_tb: Option<PyObject>,
    ) -> PyResult<bool> {
        if exc_type.is_some() {
            if let Ok(guard) = self.is_completed.try_lock() {
                if !*guard {
                    drop(guard);
                    let _ = self.rollback();
                }
            }
        }
        Ok(false)
    }
}

impl Transaction {
    /// Create a new transaction using manual BEGIN command
    pub fn new(client: Arc<Mutex<Client>>, runtime: RuntimeManager) -> PyResult<Self> {
        let txn = Self {
            client,
            runtime: runtime.clone(),
            is_completed: Arc::new(Mutex::new(false)),
        };
        
        // Execute BEGIN to start transaction
        runtime.block_on(async {
            let client = txn.client.lock().await;
            client.batch_execute("BEGIN").await.map_err(map_db_error)
        })?;
        
        Ok(txn)
    }

    /// Check if transaction is still active
    fn check_active(&self) -> PyResult<()> {
        if *self.is_completed.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Transaction state check failed")
        })? {
            Err(transaction_completed_error())
        } else {
            Ok(())
        }
    }
}