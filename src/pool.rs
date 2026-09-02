use pyo3::prelude::*;
use pyo3::types::PyList;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::error::map_db_error;
use crate::row::Row;
use crate::types::bind_params;

/// An async PostgreSQL connection pool. Construct via the module-level
/// `connect()` factory - there is no synchronous constructor, since
/// establishing the first connection is itself async.
#[pyclass]
pub struct Pool {
    pool: PgPool,
}

#[pymethods]
impl Pool {
    #[pyo3(signature = (query, params=None))]
    fn execute<'p>(&self, py: Python<'p>, query: String, params: Option<Vec<PyObject>>) -> PyResult<&'p PyAny> {
        let pool = self.pool.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let q = Python::with_gil(|py| {
                bind_params(sqlx::query(sqlx::AssertSqlSafe(query)), py, &params.unwrap_or_default())
            })?;
            let result = q.execute(&pool).await.map_err(map_db_error)?;
            Ok(result.rows_affected())
        })
    }

    #[pyo3(signature = (query, params=None))]
    fn query<'p>(&self, py: Python<'p>, query: String, params: Option<Vec<PyObject>>) -> PyResult<&'p PyAny> {
        let pool = self.pool.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let q = Python::with_gil(|py| {
                bind_params(sqlx::query(sqlx::AssertSqlSafe(query)), py, &params.unwrap_or_default())
            })?;
            let rows = q.fetch_all(&pool).await.map_err(map_db_error)?;
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
        let pool = self.pool.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let q = Python::with_gil(|py| {
                bind_params(sqlx::query(sqlx::AssertSqlSafe(query)), py, &params.unwrap_or_default())
            })?;
            let row = q.fetch_one(&pool).await.map_err(map_db_error)?;
            Python::with_gil(|py| Row::from_pg_row(py, &row))
        })
    }

    fn close<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let pool = self.pool.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            pool.close().await;
            // pool.close() waits for connections checked out through the
            // normal path, but a Transaction abandoned without commit()/
            // rollback()/`async with` returns its connection via a detached
            // background task spawned from Rust's Drop (see
            // Transaction::drop) - nothing awaits that task's completion.
            // A short grace period here gives it a scheduling window to
            // actually finish before the caller (often the whole process)
            // proceeds, rather than racing runtime/process teardown against
            // still-in-flight cleanup work.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            Ok(())
        })
    }

    fn is_closed(&self) -> bool {
        self.pool.is_closed()
    }

    /// Begin a transaction. `tx = await pool.transaction()`, then either
    /// explicit `await tx.commit()` / `await tx.rollback()`, or
    /// `async with tx:` for auto-commit on clean exit / auto-rollback on
    /// exception. Starting the transaction is itself async (it's a real
    /// `BEGIN` round-trip), so `pool.transaction()` can't be used directly
    /// as `async with pool.transaction() as tx:` - it must be awaited
    /// first to get the `Transaction` object.
    fn transaction<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let pool = self.pool.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let txn = pool.begin().await.map_err(map_db_error)?;
            Ok(crate::transaction::Transaction::new(txn))
        })
    }
}

impl Pool {
    pub(crate) fn from_pg_pool(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Create a new connection pool. This is the only way to obtain a `Pool`.
#[pyfunction]
#[pyo3(signature = (dsn, max_size=10, min_size=0))]
pub fn connect(py: Python<'_>, dsn: String, max_size: u32, min_size: u32) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        let pool = PgPoolOptions::new()
            .max_connections(max_size)
            .min_connections(min_size)
            .connect(&dsn)
            .await
            .map_err(map_db_error)?;
        Ok(Pool::from_pg_pool(pool))
    })
}
