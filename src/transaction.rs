use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Transaction as TokioTransaction;
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::error::{map_db_error, transaction_completed_error};
use crate::runtime::RuntimeManager;
use crate::types::py_objects_to_postgres_values;
use crate::row::Row;

/// Represents a database transaction
#[pyclass]
pub struct Transaction {
    transaction: Arc<Mutex<Option<TokioTransaction<'static>>>>,
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

        let transaction = Arc::clone(&self.transaction);
        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p as &(dyn postgres_types::ToSql + Sync))
                .collect();

            txn.execute(query, &params_refs)
                .await
                .map_err(map_db_error)
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

        let transaction = Arc::clone(&self.transaction);
        let rows = self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p as &(dyn postgres_types::ToSql + Sync))
                .collect();

            txn.query(query, &params_refs)
                .await
                .map_err(map_db_error)
        })?;

        let py_rows = PyList::empty(py);
        for row in rows {
            let py_row = Row::from_tokio_row(py, &row)?;
            py_rows.append(py_row)?;
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

        let transaction = Arc::clone(&self.transaction);
        let row = self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            let params_refs: Vec<&(dyn postgres_types::ToSql + Sync)> = postgres_params
                .iter()
                .map(|p| p as &(dyn postgres_types::ToSql + Sync))
                .collect();

            txn.query_one(query, &params_refs)
                .await
                .map_err(map_db_error)
        })?;

        Row::from_tokio_row(py, &row)
    }

    /// Commit the transaction
    pub fn commit(&self) -> PyResult<()> {
        self.check_active()?;

        let transaction = Arc::clone(&self.transaction);
        let is_completed = Arc::clone(&self.is_completed);

        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.take().unwrap();
            txn.commit().await.map_err(map_db_error)?;

            let mut completed = is_completed.lock().await;
            *completed = true;

            Ok(())
        })
    }

    /// Roll back the transaction
    pub fn rollback(&self) -> PyResult<()> {
        self.check_active()?;

        let transaction = Arc::clone(&self.transaction);
        let is_completed = Arc::clone(&self.is_completed);

        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.take().unwrap();
            txn.rollback().await.map_err(map_db_error)?;

            let mut completed = is_completed.lock().await;
            *completed = true;

            Ok(())
        })
    }

    /// Create a savepoint within the transaction
    pub fn savepoint(&self, name: &str) -> PyResult<()> {
        self.check_active()?;
        
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') || name.is_empty() {
            return Err(crate::error::type_conversion_error("valid SQL identifier", name));
        }

        let transaction = Arc::clone(&self.transaction);
        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            txn.savepoint(name).await.map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Roll back to a savepoint
    pub fn rollback_to(&self, name: &str) -> PyResult<()> {
        self.check_active()?;
        
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') || name.is_empty() {
            return Err(crate::error::type_conversion_error("valid SQL identifier", name));
        }

        let transaction = Arc::clone(&self.transaction);
        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            
            let rollback_sql = format!("ROLLBACK TO SAVEPOINT {}", name);
            txn.batch_execute(&rollback_sql)
                .await
                .map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Release a savepoint
    pub fn release_savepoint(&self, name: &str) -> PyResult<()> {
        self.check_active()?;
        
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') || name.is_empty() {
            return Err(crate::error::type_conversion_error("valid SQL identifier", name));
        }

        let transaction = Arc::clone(&self.transaction);
        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            
            let release_sql = format!("RELEASE SAVEPOINT {}", name);
            txn.batch_execute(&release_sql)
                .await
                .map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Set the transaction isolation level
    pub fn set_isolation_level(&self, level: &str) -> PyResult<()> {
        self.check_active()?;
        
        let isolation_level = IsolationLevel::from_str(level)
            .ok_or_else(|| crate::error::type_conversion_error(
                "valid isolation level", 
                level
            ))?;
            
        let transaction = Arc::clone(&self.transaction);
        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            
            let sql = format!("SET TRANSACTION ISOLATION LEVEL {}", isolation_level.to_sql());
            txn.batch_execute(&sql)
                .await
                .map_err(map_db_error)?;
            Ok(())
        })
    }

    /// Set whether the transaction is read-only
    pub fn set_read_only(&self, read_only: bool) -> PyResult<()> {
        self.check_active()?;

        let transaction = Arc::clone(&self.transaction);
        self.runtime.block_on(async move {
            let mut txn_guard = transaction.lock().await;
            let txn = txn_guard.as_mut().unwrap();
            
            let sql = if read_only {
                "SET TRANSACTION READ ONLY"
            } else {
                "SET TRANSACTION READ WRITE"
            };
            
            txn.batch_execute(sql)
                .await
                .map_err(map_db_error)?;
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
            transaction: Arc::clone(&self.transaction),
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
        _exc_tb: Option<PyObject>
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
    /// Create a new transaction from a tokio_postgres transaction
    pub fn new(txn: TokioTransaction<'static>, runtime: RuntimeManager) -> Self {
        Self {
            transaction: Arc::new(Mutex::new(Some(txn))),
            runtime,
            is_completed: Arc::new(Mutex::new(false)),
        }
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