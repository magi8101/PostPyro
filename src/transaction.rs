use pyo3::prelude::*;
use pyo3::types::PyList;
use sqlx::{Postgres, Transaction as SqlxTransaction};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{map_db_error, transaction_completed_error};
use crate::row::Row;
use crate::types::bind_params;

/// A running transaction. Obtain via `tx = await pool.transaction()`, then
/// either `async with tx:` for auto-commit on success and auto-rollback on
/// exception, or explicit `commit()`/`rollback()`.
#[pyclass]
pub struct Transaction {
    inner: Arc<Mutex<Option<SqlxTransaction<'static, Postgres>>>>,
}

impl Transaction {
    pub fn new(txn: SqlxTransaction<'static, Postgres>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(txn))),
        }
    }
}

#[pymethods]
impl Transaction {
    #[pyo3(signature = (query, params=None))]
    fn execute<'p>(&self, py: Python<'p>, query: String, params: Option<Vec<PyObject>>) -> PyResult<&'p PyAny> {
        let inner = Arc::clone(&self.inner);
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut guard = inner.lock().await;
            let txn = guard.as_mut().ok_or_else(transaction_completed_error)?;
            let q = Python::with_gil(|py| {
                bind_params(sqlx::query(sqlx::AssertSqlSafe(query)), py, &params.unwrap_or_default())
            })?;
            let result = q.execute(&mut **txn).await.map_err(map_db_error)?;
            Ok(result.rows_affected())
        })
    }

    #[pyo3(signature = (query, params=None))]
    fn query<'p>(&self, py: Python<'p>, query: String, params: Option<Vec<PyObject>>) -> PyResult<&'p PyAny> {
        let inner = Arc::clone(&self.inner);
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut guard = inner.lock().await;
            let txn = guard.as_mut().ok_or_else(transaction_completed_error)?;
            let q = Python::with_gil(|py| {
                bind_params(sqlx::query(sqlx::AssertSqlSafe(query)), py, &params.unwrap_or_default())
            })?;
            let rows = q.fetch_all(&mut **txn).await.map_err(map_db_error)?;
            Python::with_gil(|py| {
                let py_rows: PyResult<Vec<Py<Row>>> = rows
                    .iter()
                    .map(|r| Py::new(py, Row::from_pg_row(py, r)?))
                    .collect();
                Ok(PyList::new(py, py_rows?).to_object(py))
            })
        })
    }

    #[pyo3(signature = (query, params=None))]
    fn query_one<'p>(&self, py: Python<'p>, query: String, params: Option<Vec<PyObject>>) -> PyResult<&'p PyAny> {
        let inner = Arc::clone(&self.inner);
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut guard = inner.lock().await;
            let txn = guard.as_mut().ok_or_else(transaction_completed_error)?;
            let q = Python::with_gil(|py| {
                bind_params(sqlx::query(sqlx::AssertSqlSafe(query)), py, &params.unwrap_or_default())
            })?;
            let row = q.fetch_one(&mut **txn).await.map_err(map_db_error)?;
            Python::with_gil(|py| Row::from_pg_row(py, &row))
        })
    }

    fn commit<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let inner = Arc::clone(&self.inner);
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut guard = inner.lock().await;
            let txn = guard.take().ok_or_else(transaction_completed_error)?;
            txn.commit().await.map_err(map_db_error)
        })
    }

    fn rollback<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let inner = Arc::clone(&self.inner);
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut guard = inner.lock().await;
            let txn = guard.take().ok_or_else(transaction_completed_error)?;
            txn.rollback().await.map_err(map_db_error)
        })
    }

    fn is_active(&self) -> bool {
        self.inner.try_lock().map(|g| g.is_some()).unwrap_or(true)
    }

    fn __aenter__<'p>(slf: PyRef<'_, Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        let same = Py::new(py, Transaction { inner: Arc::clone(&slf.inner) })?;
        pyo3_asyncio::tokio::future_into_py(py, async move { Ok(same) })
    }

    #[pyo3(signature = (exc_type, exc_val, exc_tb))]
    fn __aexit__<'p>(
        &self,
        py: Python<'p>,
        exc_type: Option<PyObject>,
        exc_val: Option<PyObject>,
        exc_tb: Option<PyObject>,
    ) -> PyResult<&'p PyAny> {
        let _ = (exc_val, exc_tb);
        let inner = Arc::clone(&self.inner);
        let had_exception = exc_type.is_some();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut guard = inner.lock().await;
            if let Some(txn) = guard.take() {
                if had_exception {
                    let _ = txn.rollback().await;
                } else {
                    txn.commit().await.map_err(map_db_error)?;
                }
            }
            Ok(false)
        })
    }
}
