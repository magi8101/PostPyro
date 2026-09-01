# PostPyro Async Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace PostPyro's `tokio-postgres`/`deadpool-postgres`/hand-rolled type layer with `sqlx` + `pyo3-asyncio`, producing a fully async, GIL-releasing driver with a real column-name-addressable `Row` and no float-precision loss on parameter binding.

**Architecture:** One shared `tokio::Runtime` backs both `sqlx`'s pool and `pyo3-asyncio`'s bridge. Every DB operation is exposed to Python as `async def`/`await` via `pyo3_asyncio::tokio::future_into_py` — no `block_on`, no GIL held during I/O. `Connection`/`ConnectionPool` unify into a single `Pool` class, constructed only via an async `connect()` factory (PyO3 constructors can't be async).

**Tech Stack:** Rust, PyO3 0.20 (`extension-module`, `abi3-py38` — unchanged), `sqlx` 0.9 (`postgres`, `runtime-tokio`, `tls-rustls`, `chrono`, `uuid`, `json`), `pyo3-asyncio` 0.20 (`tokio-runtime`), Python 3.8+, `maturin`.

**Spec:** `/home/user/Documents/coding/posty/PostPyro/ARCHITECTURE.md`

## Global Constraints

- Rust edition stays 2021 (do not change `Cargo.toml`'s `edition`).
- `sqlx = { version = "0.9", features = ["runtime-tokio", "tls-rustls", "postgres", "chrono", "uuid", "json"] }` — no other sqlx features.
- `pyo3-asyncio = { version = "0.20", features = ["tokio-runtime"] }` — this is the version that matches the pinned `pyo3 = "0.20"`; do not bump pyo3 to chase a newer pyo3-asyncio fork as part of this plan.
- `sqlx`'s `SqlSafeStr` trait requires wrapping any non-`'static` query string in `sqlx::AssertSqlSafe(..)` before passing it to `sqlx::query(..)`. This is safe here because query text is always developer-authored SQL with `$1`-style placeholders — user data flows only through `.bind()`, never string-concatenated into the query text.
- `Pool` has **no** `#[new]`/constructor. The only way to obtain one is `await PostPyro.connect(dsn, max_size=10, min_size=0)`, because establishing the first connection is itself async.
- Anything that touches `PyObject`/`Python::with_gil` **cannot** be tested via bare `cargo test` — the crate builds with the `extension-module` PyO3 feature, which does not link `libpython` into a standalone test binary. Only pure-Rust logic with no PyO3 types gets a `cargo test` unit test; everything else is verified by importing the built extension from a real Python process (`maturin develop` then `python3 -c "..."`), against a live Postgres.
- DB-API 2.0 exception class names are unchanged: `DatabaseError`, `InterfaceError`, `DataError`, `OperationalError`, `IntegrityError`, `InternalError`, `ProgrammingError`, `NotSupportedError`.
- Every task below is implemented on its own branch cut from `master` (`rewrite/NN-slug`), pushed, and opened as its own PR. Do not stack task branches on each other — merge each PR before branching for the next task. The current empty `rewrite/sqlx-async` branch (no commits ahead of `master`) should be renamed to `rewrite/01-deps-and-runtime` to start Task 1.
- Every commit's trailer is `Co-Authored-By: mj7841@srmist.edu.in` — never the default Claude address.
- Live-Postgres test steps use a throwaway container: `docker run -d --rm --name postpyro-test-pg -e POSTGRES_PASSWORD=postgres -p 5433:5432 postgres:16`, target `postgresql://postgres:postgres@localhost:5433/postgres`, torn down with `docker stop postpyro-test-pg` at the end of the task.

---

## Task 1: Add `sqlx` + `pyo3-asyncio`; wire the shared Tokio runtime into pyo3-asyncio

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/runtime.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Produces: `runtime::RuntimeManager::shared() -> &'static tokio::runtime::Runtime` — used by every later task that needs the runtime pyo3-asyncio is bridging into.
- Produces (temporary, removed in Task 3): `_test_async_bridge()` pyfunction proving the bridge works.

- [ ] **Step 1: Rename the current branch and add the new dependencies**

```bash
git branch -m rewrite/sqlx-async rewrite/01-deps-and-runtime
```

Edit `Cargo.toml`, adding to `[dependencies]` (leave every existing dependency untouched — they're still used by the not-yet-replaced `Connection`/`ConnectionPool`/`Transaction`):

```toml
sqlx = { version = "0.9", features = ["runtime-tokio", "tls-rustls", "postgres", "chrono", "uuid", "json"] }
pyo3-asyncio = { version = "0.20", features = ["tokio-runtime"] }
```

- [ ] **Step 2: Verify it still compiles unmodified**

Run: `cargo check`
Expected: succeeds (same warnings as before; two new crates now resolve in `Cargo.lock`).

- [ ] **Step 3: Rewrite `src/runtime.rs` to expose a `'static` runtime reference**

`pyo3_asyncio::tokio::init_with_runtime` requires `&'static Runtime`. Drop the `Arc<Runtime>` wrapper (the `once_cell::Lazy<Runtime>` static already yields `'static` references) and add a `shared()` accessor. `RuntimeManager`'s public surface (`new`, `block_on`, `spawn`, `Clone`) stays identical, so `connection.rs`/`pool.rs`/`transaction.rs` need no changes yet:

```rust
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Runtime::new().expect("Failed to create Tokio runtime")
});

#[derive(Clone, Copy)]
pub struct RuntimeManager {
    runtime: &'static Runtime,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self { runtime: &RUNTIME }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        self.runtime.block_on(future)
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }

    /// The process-wide Tokio runtime, shared with pyo3-asyncio and sqlx.
    pub fn shared() -> &'static Runtime {
        &RUNTIME
    }
}
```

- [ ] **Step 4: Verify it still compiles**

Run: `cargo check`
Expected: succeeds, no new warnings beyond the pre-existing ones.

- [ ] **Step 5: Wire `pyo3-asyncio` into module init and add a smoke-test function**

Edit `src/lib.rs` — add the import, the `_test_async_bridge` pyfunction, the `init_with_runtime` call, and register the function:

```rust
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
```

- [ ] **Step 6: Build the extension and run the Python smoke test**

Run:
```bash
pip install --quiet maturin
maturin develop
python3 -c "
import asyncio, PostPyro
result = asyncio.run(PostPyro._test_async_bridge())
assert result is True
print('OK: pyo3-asyncio bridge works on the shared runtime')
"
```
Expected: prints `OK: pyo3-asyncio bridge works on the shared runtime`, exit code 0.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/runtime.rs src/lib.rs
git commit -m "$(cat <<'EOF'
Add sqlx + pyo3-asyncio deps, wire shared Tokio runtime

pyo3-asyncio bridges into the same runtime sqlx will use, proven with
a throwaway async smoke-test function. Existing sync Connection/
ConnectionPool/Transaction are untouched and still work.

Co-Authored-By: mj7841@srmist.edu.in
EOF
)"
```

- [ ] **Step 8: Push and open the PR**

```bash
git push -u origin rewrite/01-deps-and-runtime
gh pr create --title "Wire pyo3-asyncio onto the shared Tokio runtime" --body "$(cat <<'EOF'
## Summary
- Adds sqlx and pyo3-asyncio dependencies (unused by production code yet)
- Reworks the shared runtime static to hand out `&'static Runtime`
- Initializes pyo3-asyncio against that same runtime at module load
- Proves the wiring with a temporary `_test_async_bridge()` smoke test (removed in a later PR)

## Test plan
- [x] `cargo check` passes
- [x] `maturin develop` + Python smoke test confirms the async bridge resolves on the shared runtime
EOF
)"
```

---

## Task 2: Rewrite `error.rs` for `sqlx::Error`

**Files:**
- Modify: `src/error.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `pub fn map_db_error(error: sqlx::Error) -> PyErr` — used by every DB-facing method from Task 3 onward. Exception types (`DatabaseError`, `InterfaceError`, `DataError`, `OperationalError`, `IntegrityError`, `InternalError`, `ProgrammingError`, `NotSupportedError`) are unchanged. `pub fn type_conversion_error(expected: &str, actual: &str) -> PyErr` and `pub fn transaction_completed_error() -> PyErr` are unchanged in signature, kept for later tasks.

- [ ] **Step 1: Start the branch**

```bash
git checkout master && git pull
git checkout -b rewrite/02-error-mapping
```

- [ ] **Step 2: Write the failing unit tests**

Add to the bottom of `src/error.rs` (the module doesn't exist yet at this point — this step writes the tests first, referencing the `classify_sqlstate` function Step 4 will add):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_connection_sqlstate() {
        assert_eq!(classify_sqlstate("08006"), PostgreSQLErrorClass::ConnectionIssue);
    }

    #[test]
    fn classifies_unique_violation_sqlstate() {
        assert_eq!(classify_sqlstate("23505"), PostgreSQLErrorClass::ConstraintViolation);
    }

    #[test]
    fn classifies_syntax_error_sqlstate() {
        assert_eq!(classify_sqlstate("42601"), PostgreSQLErrorClass::SyntaxError);
    }

    #[test]
    fn classifies_unknown_sqlstate_as_generic() {
        assert_eq!(classify_sqlstate("99999"), PostgreSQLErrorClass::GenericDatabase);
    }

    #[test]
    fn classifies_insufficient_resources() {
        assert_eq!(classify_sqlstate("53200"), PostgreSQLErrorClass::InsufficientResources);
        assert_eq!(classify_sqlstate("54000"), PostgreSQLErrorClass::InsufficientResources);
    }
}
```

(No GIL-touching tests here — per Global Constraints, `PyErr`/`Python::with_gil` code isn't reachable from a bare `cargo test` binary under the `extension-module` feature. `map_db_error`'s exception-mapping behavior is verified end-to-end from Python starting in Task 3, once there's a live method that can trigger a real Postgres error.)

- [ ] **Step 3: Run the tests to confirm they fail to compile**

Run: `cargo test --lib error::tests`
Expected: FAIL — `classify_sqlstate` and `PostgreSQLErrorClass` not found.

- [ ] **Step 4: Replace the rest of `src/error.rs`**

```rust
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::PyErr;

// Base Database Error - follows DB-API 2.0 specification (PEP 249)
create_exception!(PostPyro, DatabaseError, PyException);
create_exception!(PostPyro, InterfaceError, DatabaseError);
create_exception!(PostPyro, DataError, DatabaseError);
create_exception!(PostPyro, OperationalError, DatabaseError);
create_exception!(PostPyro, IntegrityError, DatabaseError);
create_exception!(PostPyro, InternalError, DatabaseError);
create_exception!(PostPyro, ProgrammingError, DatabaseError);
create_exception!(PostPyro, NotSupportedError, DatabaseError);

pub fn type_conversion_error(expected: &str, actual: &str) -> PyErr {
    DataError::new_err(format!(
        "Type conversion error: expected {}, got {}",
        expected, actual
    ))
}

pub fn invalid_connection_string_error(details: &str) -> PyErr {
    InterfaceError::new_err(format!("Invalid connection string: {}", details))
}

pub fn transaction_completed_error() -> PyErr {
    ProgrammingError::new_err("Transaction is already committed or rolled back")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostgreSQLErrorClass {
    ConnectionIssue,
    SyntaxError,
    ConstraintViolation,
    DataTypeIssue,
    InsufficientResources,
    SystemError,
    UnsupportedFeature,
    GenericDatabase,
}

/// Classify a PostgreSQL SQLSTATE code into an error category. Pure
/// function so it's unit-testable without a live connection or the GIL.
fn classify_sqlstate(code: &str) -> PostgreSQLErrorClass {
    match code {
        c if c.starts_with("08") => PostgreSQLErrorClass::ConnectionIssue,
        c if c.starts_with("42") => PostgreSQLErrorClass::SyntaxError,
        c if c.starts_with("23") => PostgreSQLErrorClass::ConstraintViolation,
        c if c.starts_with("22") => PostgreSQLErrorClass::DataTypeIssue,
        c if c.starts_with("53") || c.starts_with("54") => {
            PostgreSQLErrorClass::InsufficientResources
        }
        c if c.starts_with("58") || c == "XX000" => PostgreSQLErrorClass::SystemError,
        c if c.starts_with("0A") => PostgreSQLErrorClass::UnsupportedFeature,
        _ => PostgreSQLErrorClass::GenericDatabase,
    }
}

fn suggestion_for(class: PostgreSQLErrorClass, sqlstate: &str) -> &'static str {
    match class {
        PostgreSQLErrorClass::ConnectionIssue => {
            "Check network connectivity, server status, and connection parameters"
        }
        PostgreSQLErrorClass::SyntaxError => {
            "Verify SQL syntax, table/column names, and parameter placeholders"
        }
        PostgreSQLErrorClass::ConstraintViolation => match sqlstate {
            "23505" => "Duplicate key violation - ensure unique values",
            "23503" => "Foreign key constraint violation - check referenced values",
            "23502" => "NOT NULL constraint violation - provide required values",
            "23514" => "CHECK constraint violation - verify data meets constraints",
            _ => "Check data integrity constraints",
        },
        PostgreSQLErrorClass::DataTypeIssue => {
            "Verify data types and format - check parameter types and values"
        }
        PostgreSQLErrorClass::InsufficientResources => {
            "Database server resources exhausted - contact administrator"
        }
        PostgreSQLErrorClass::SystemError => {
            "Internal database error - check server logs and contact administrator"
        }
        PostgreSQLErrorClass::UnsupportedFeature => "Feature not available in this PostgreSQL version",
        PostgreSQLErrorClass::GenericDatabase => "Check query and database configuration",
    }
}

fn map_database_error(db_err: &dyn sqlx::error::DatabaseError) -> PyErr {
    let message = db_err.message();
    let (class, enhanced) = match db_err.code() {
        Some(code) => {
            let class = classify_sqlstate(&code);
            let suggestion = suggestion_for(class, &code);
            (
                class,
                format!("{} (SQLSTATE: {})\nSuggestion: {}", message, code, suggestion),
            )
        }
        None => (PostgreSQLErrorClass::GenericDatabase, message.to_string()),
    };

    match class {
        PostgreSQLErrorClass::ConnectionIssue | PostgreSQLErrorClass::InsufficientResources => {
            OperationalError::new_err(enhanced)
        }
        PostgreSQLErrorClass::SyntaxError => ProgrammingError::new_err(enhanced),
        PostgreSQLErrorClass::ConstraintViolation => IntegrityError::new_err(enhanced),
        PostgreSQLErrorClass::DataTypeIssue => DataError::new_err(enhanced),
        PostgreSQLErrorClass::SystemError => InternalError::new_err(enhanced),
        PostgreSQLErrorClass::UnsupportedFeature => NotSupportedError::new_err(enhanced),
        PostgreSQLErrorClass::GenericDatabase => DatabaseError::new_err(enhanced),
    }
}

/// Map a sqlx error to the DB-API 2.0 exception hierarchy.
pub fn map_db_error(error: sqlx::Error) -> PyErr {
    match error {
        sqlx::Error::Database(db_err) => map_database_error(db_err.as_ref()),
        sqlx::Error::RowNotFound => {
            ProgrammingError::new_err("Query returned no rows, expected exactly one")
        }
        sqlx::Error::PoolTimedOut => {
            OperationalError::new_err("Timed out waiting for a connection from the pool")
        }
        sqlx::Error::PoolClosed => OperationalError::new_err("Connection pool is closed"),
        sqlx::Error::WorkerCrashed => InternalError::new_err("Database worker task crashed"),
        sqlx::Error::Io(e) => OperationalError::new_err(format!("I/O error: {}", e)),
        sqlx::Error::Tls(e) => OperationalError::new_err(format!("TLS error: {}", e)),
        sqlx::Error::Protocol(msg) => InternalError::new_err(format!("Protocol error: {}", msg)),
        sqlx::Error::Configuration(e) => {
            InterfaceError::new_err(format!("Invalid configuration: {}", e))
        }
        sqlx::Error::ColumnNotFound(name) => {
            ProgrammingError::new_err(format!("Column '{}' not found", name))
        }
        sqlx::Error::ColumnIndexOutOfBounds { index, len } => ProgrammingError::new_err(format!(
            "Column index {} out of bounds (row has {} columns)",
            index, len
        )),
        sqlx::Error::ColumnDecode { index, source } => {
            DataError::new_err(format!("Failed to decode column {}: {}", index, source))
        }
        sqlx::Error::Decode(e) => DataError::new_err(format!("Decode error: {}", e)),
        sqlx::Error::Encode(e) => DataError::new_err(format!("Encode error: {}", e)),
        sqlx::Error::TypeNotFound { type_name } => {
            NotSupportedError::new_err(format!("Type '{}' not found", type_name))
        }
        other => DatabaseError::new_err(format!("Database error: {}", other)),
    }
}
```

Note: `sqlx::Error` is `#[non_exhaustive]`, hence the `other =>` catch-all arm.

- [ ] **Step 5: Run the tests to confirm they pass**

Run: `cargo test --lib error::tests`
Expected: 5 tests pass.

- [ ] **Step 6: Confirm the whole crate still builds**

Run: `cargo check`
Expected: succeeds. `map_db_error` and `invalid_connection_string_error` are unused warnings at this point (nothing calls them yet, until Task 3) — that's expected and resolves once Task 3 lands.

- [ ] **Step 7: Commit**

```bash
git add src/error.rs
git commit -m "$(cat <<'EOF'
Retarget error mapping from tokio_postgres::Error to sqlx::Error

Same DB-API 2.0 exception hierarchy and SQLSTATE-based classification
as before, now covering sqlx's non-Database error variants (RowNotFound,
PoolTimedOut, PoolClosed, etc). Also drops the old dead
map_db_error_simple/not_supported_error/connection_closed_error/
performance-timing wrapper - sqlx's pool reports closed-pool state as
a PoolClosed error itself, so there's no need to hand-track a
connection-closed flag any more.

Co-Authored-By: mj7841@srmist.edu.in
EOF
)"
```

- [ ] **Step 8: Push and open the PR**

```bash
git push -u origin rewrite/02-error-mapping
gh pr create --title "Retarget DB-API error mapping to sqlx::Error" --body "$(cat <<'EOF'
## Summary
- `map_db_error` now takes `sqlx::Error` instead of `tokio_postgres::Error`
- Same public exception hierarchy; SQLSTATE classification logic unchanged
- Handles sqlx's additional non-Database error variants
- Drops dead code from the old implementation (unused `map_db_error_simple`, timing instrumentation)

## Test plan
- [x] `cargo test --lib error::tests` - 5 SQLSTATE classification tests pass
- [x] `cargo check` passes
EOF
)"
```

---

## Task 3: `types.rs` + `Row` + async `Pool` (execute/query/query_one/close)

This is the first task that actually replaces the old sync driver core. It's sized as one task/PR because `types.rs`, `row.rs`, and `pool.rs` have no meaningful independent test boundary — none of them can be exercised from Python without the other two.

**Files:**
- Delete: `src/connection.rs`
- Rewrite: `src/row.rs`
- Rewrite: `src/types.rs`
- Rewrite: `src/pool.rs`
- Modify: `src/lib.rs`
- Test: `tests/pool_and_row.py` (new — a Python integration script, not a `cargo test`; see Global Constraints)

**Interfaces:**
- Consumes: `error::map_db_error`, `runtime::RuntimeManager::shared()` (Tasks 1-2).
- Produces: `types::bind_params(query, py, params) -> PyResult<Query<Postgres, PgArguments>>`, `types::pg_value_to_py(py, row, idx) -> PyResult<PyObject>`, `Row::from_pg_row(py, &PgRow) -> PyResult<Row>` — all consumed by Task 4's `Transaction`. `pool::connect(py, dsn, max_size, min_size) -> PyResult<&PyAny>` (async pyfunction) and the `Pool` pyclass (`execute`, `query`, `query_one`, `close`, `is_closed`) — `Pool.transaction()` is added in Task 4, not here.

- [ ] **Step 1: Start the branch**

```bash
git checkout master && git pull
git checkout -b rewrite/03-row-types-pool
```

- [ ] **Step 2: Delete `src/connection.rs`**

```bash
git rm src/connection.rs
```

- [ ] **Step 3: Rewrite `src/row.rs`**

```rust
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use sqlx::postgres::PgRow;
use sqlx::{Column, Row as SqlxRow};

use crate::types::pg_value_to_py;

/// A single result row with real name- and index-based column access.
#[pyclass(frozen)]
pub struct Row {
    names: Vec<String>,
    values: Vec<PyObject>,
}

#[pymethods]
impl Row {
    fn __len__(&self) -> usize {
        self.values.len()
    }

    fn __getitem__(&self, py: Python, key: &PyAny) -> PyResult<PyObject> {
        if let Ok(idx) = key.extract::<usize>() {
            self.values
                .get(idx)
                .map(|v| v.clone_ref(py))
                .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("Row index out of range"))
        } else if let Ok(name) = key.extract::<&str>() {
            self.get_by_name(py, name)
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!("Column '{}' not found", name)))
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "Row indices must be an int or str",
            ))
        }
    }

    fn __iter__(&self, py: Python) -> PyResult<PyObject> {
        let list = PyList::new(py, self.values.iter().map(|v| v.clone_ref(py)));
        list.call_method0("__iter__").map(|it| it.to_object(py))
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let mut parts = Vec::with_capacity(self.names.len());
        for (name, value) in self.names.iter().zip(self.values.iter()) {
            parts.push(format!("{}={}", name, value.as_ref(py).repr()?));
        }
        Ok(format!("Row({})", parts.join(", ")))
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, py: Python, key: &PyAny, default: Option<PyObject>) -> PyResult<PyObject> {
        let found = if let Ok(idx) = key.extract::<usize>() {
            self.values.get(idx).map(|v| v.clone_ref(py))
        } else if let Ok(name) = key.extract::<&str>() {
            self.get_by_name(py, name)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err("key must be an int or str"));
        };
        Ok(found.unwrap_or_else(|| default.unwrap_or_else(|| py.None())))
    }

    fn keys(&self) -> Vec<String> {
        self.names.clone()
    }

    fn values(&self, py: Python) -> Vec<PyObject> {
        self.values.iter().map(|v| v.clone_ref(py)).collect()
    }

    fn items(&self, py: Python) -> Vec<(String, PyObject)> {
        self.names
            .iter()
            .cloned()
            .zip(self.values.iter().map(|v| v.clone_ref(py)))
            .collect()
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (name, value) in self.names.iter().zip(self.values.iter()) {
            dict.set_item(name, value.clone_ref(py))?;
        }
        Ok(dict.to_object(py))
    }
}

impl Row {
    fn get_by_name(&self, py: Python, name: &str) -> Option<PyObject> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|idx| self.values[idx].clone_ref(py))
    }

    pub fn from_pg_row(py: Python, row: &PgRow) -> PyResult<Self> {
        let columns = row.columns();
        let mut names = Vec::with_capacity(columns.len());
        let mut values = Vec::with_capacity(columns.len());
        for (idx, column) in columns.iter().enumerate() {
            names.push(column.name().to_string());
            values.push(pg_value_to_py(py, row, idx)?);
        }
        Ok(Row { names, values })
    }
}
```

- [ ] **Step 4: Rewrite `src/types.rs`**

```rust
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString};
use sqlx::postgres::PgRow;
use sqlx::{Column, Postgres, Row as SqlxRow, TypeInfo};

use crate::error::type_conversion_error;

/// Bind a Python parameter list onto a query, one `.bind()` call per
/// parameter. sqlx only exposes `.bind()` (not a standalone Arguments
/// builder) for the default `Query<DB, DB::Arguments>` returned by
/// `sqlx::query()`, so this takes and returns that concrete type.
///
/// ponytail: Python ints always bind as Postgres BIGINT (i8/int8). This is
/// simple and deterministic, unlike guessing i16/i32/i64 from the value's
/// magnitude (what the old driver did). If an INSERT/UPDATE against a
/// narrower INT2/INT4 column needs an exact type match, cast in the SQL
/// text (`$1::int4`). Revisit only if this becomes a real friction point.
pub fn bind_params<'q>(
    mut query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    py: Python,
    params: &[PyObject],
) -> PyResult<sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>> {
    for obj in params {
        let obj_ref = obj.as_ref(py);
        query = if obj.is_none(py) {
            query.bind(None::<String>)
        } else if let Ok(b) = obj_ref.downcast::<PyBool>() {
            query.bind(b.extract::<bool>()?)
        } else if let Ok(i) = obj_ref.downcast::<PyInt>() {
            query.bind(i.extract::<i64>()?)
        } else if let Ok(f) = obj_ref.downcast::<PyFloat>() {
            query.bind(f.extract::<f64>()?)
        } else if let Ok(s) = obj_ref.downcast::<PyString>() {
            query.bind(s.extract::<String>()?)
        } else {
            let s = obj_ref.str()?.extract::<String>()?;
            query.bind(s)
        };
    }
    Ok(query)
}

/// Convert one PgRow column to a Python object, type-specialized for the
/// common scalar types. Anything else falls back to a string
/// representation rather than failing the whole row.
pub fn pg_value_to_py(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let type_name = row.column(idx).type_info().name();
    let value = match type_name {
        "BOOL" => row.try_get::<Option<bool>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "INT2" => row.try_get::<Option<i16>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "INT4" => row.try_get::<Option<i32>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "INT8" => row.try_get::<Option<i64>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "FLOAT4" => row.try_get::<Option<f32>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "FLOAT8" => row.try_get::<Option<f64>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" => {
            row.try_get::<Option<String>, _>(idx).ok().flatten().map(|v| v.into_py(py))
        }
        _ => row.try_get::<Option<String>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
    };
    Ok(value.unwrap_or_else(|| py.None()))
}

#[allow(dead_code)]
fn unused_import_anchor() -> PyErr {
    // Keeps `type_conversion_error` imported/used until Task 4 also calls it
    // from transaction.rs bind failures; remove this if unused warnings persist.
    type_conversion_error("", "")
}
```

Note on the last function: if `cargo check` in Step 5 shows `type_conversion_error` as genuinely unused (rather than a false positive), delete `unused_import_anchor` and the `use` line instead of keeping a placeholder — don't leave dead code just to silence a warning.

- [ ] **Step 5: Rewrite `src/pool.rs`**

```rust
use pyo3::prelude::*;
use pyo3::types::PyList;
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::error::map_db_error;
use crate::row::Row;
use crate::types::{bind_params, pg_value_to_py as _pg_value_to_py};

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
                let py_rows: PyResult<Vec<Row>> =
                    rows.iter().map(|r| Row::from_pg_row(py, r)).collect();
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
            Ok(())
        })
    }

    fn is_closed(&self) -> bool {
        self.pool.is_closed()
    }
}

impl Pool {
    pub(crate) fn from_pg_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) fn inner(&self) -> &PgPool {
        &self.pool
    }
}

/// Create a new connection pool. This is the only way to obtain a `Pool`.
#[pyfunction]
#[pyo3(signature = (dsn, max_size=10, min_size=0))]
pub fn connect(py: Python, dsn: String, max_size: u32, min_size: u32) -> PyResult<&PyAny> {
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
```

(`_pg_value_to_py` import and `Pool::inner()` are unused until Task 4 wires `Pool.transaction()` through them — if `cargo check` in Step 6 flags them, that's expected; leave them, Task 4 removes the underscore/`#[allow]` as it starts using them for real.)

- [ ] **Step 6: Update `src/lib.rs`** — remove the old `Connection`/`ConnectionPool` wiring and the old (currently unreachable) `Transaction` registration, remove the Task 1 smoke-test function, register `Pool` and `connect`:

```rust
use pyo3::prelude::*;

mod error;
mod pool;
mod row;
mod runtime;
mod types;

use error::{
    DataError, DatabaseError, IntegrityError, InterfaceError, InternalError, NotSupportedError,
    OperationalError, ProgrammingError,
};
use pool::{connect, Pool};
use row::Row;
use runtime::RuntimeManager;

#[pymodule]
fn PostPyro(_py: Python, m: &PyModule) -> PyResult<()> {
    pyo3_asyncio::tokio::init_with_runtime(RuntimeManager::shared())
        .expect("pyo3-asyncio: failed to init shared Tokio runtime");

    m.add_class::<Pool>()?;
    m.add_class::<Row>()?;
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
```

Note `mod transaction;` and `mod connection;` are gone — `src/connection.rs` was deleted in Step 2, and `src/transaction.rs` (old, already unreachable from Python since `Connection.begin()` never existed) is deleted now too:

```bash
git rm src/transaction.rs
```

- [ ] **Step 7: Build**

Run: `cargo check`
Expected: succeeds. If `bind_params`/`pg_value_to_py` or the `Pool::inner`/`from_pg_pool` helpers show unused-code warnings, that's expected — Task 4 consumes them.

- [ ] **Step 8: Write the integration test**

Create `tests/pool_and_row.py`:

```python
import asyncio
import PostPyro


async def main():
    pool = await PostPyro.connect("postgresql://postgres:postgres@localhost:5433/postgres", max_size=5)

    await pool.execute("DROP TABLE IF EXISTS pool_and_row_test")
    await pool.execute(
        "CREATE TABLE pool_and_row_test (id INT4, name TEXT, score FLOAT8, active BOOL)"
    )

    affected = await pool.execute(
        "INSERT INTO pool_and_row_test (id, name, score, active) VALUES ($1, $2, $3, $4)",
        [1, "Ada", 3.14159265358979, True],
    )
    assert affected == 1, f"expected 1 row affected, got {affected}"

    rows = await pool.query("SELECT * FROM pool_and_row_test")
    assert len(rows) == 1
    row = rows[0]

    # Column-name access must return the RIGHT column - this was the bug
    # in the old driver (always returned column 0 regardless of name).
    assert row["name"] == "Ada", f"expected 'Ada', got {row['name']!r}"
    assert row["id"] == 1
    assert row["active"] is True

    # Float precision must round-trip exactly through FLOAT8 - the old
    # driver forced every float to f32 and lost precision here.
    assert row["score"] == 3.14159265358979, f"precision lost: {row['score']!r}"

    assert row.keys() == ["id", "name", "score", "active"]
    assert row.to_dict() == {"id": 1, "name": "Ada", "score": 3.14159265358979, "active": True}
    assert dict(row.items()) == row.to_dict()
    assert list(row) == [1, "Ada", 3.14159265358979, True]
    assert row.get("nonexistent", "default") == "default"
    assert row[0] == 1
    assert len(row) == 4

    one = await pool.query_one("SELECT * FROM pool_and_row_test WHERE id = $1", [1])
    assert one["name"] == "Ada"

    # NULL handling
    await pool.execute("INSERT INTO pool_and_row_test (id) VALUES ($1)", [2])
    null_row = await pool.query_one("SELECT * FROM pool_and_row_test WHERE id = $1", [2])
    assert null_row["name"] is None

    # Error mapping: unique-violation-shaped error surfaces as IntegrityError
    await pool.execute("ALTER TABLE pool_and_row_test ADD CONSTRAINT id_unique UNIQUE (id)")
    try:
        await pool.execute("INSERT INTO pool_and_row_test (id) VALUES ($1)", [1])
        assert False, "expected IntegrityError"
    except PostPyro.IntegrityError:
        pass

    await pool.execute("DROP TABLE pool_and_row_test")
    await pool.close()
    assert pool.is_closed()
    print("OK: pool + row + types + error mapping all verified")


asyncio.run(main())
```

- [ ] **Step 9: Run the integration test**

Run:
```bash
docker run -d --rm --name postpyro-test-pg -e POSTGRES_PASSWORD=postgres -p 5433:5432 postgres:16
sleep 3
maturin develop
python3 tests/pool_and_row.py
docker stop postpyro-test-pg
```
Expected: prints `OK: pool + row + types + error mapping all verified`, exit code 0.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
Replace Connection/ConnectionPool with async sqlx-backed Pool

Row now has real name-based column access (was previously stubbed to
always return column 0) and float parameters bind as f64/FLOAT8
without the old forced f32 downcast. Pool is constructed only via the
async connect() factory - PyO3 constructors can't be async, and
sqlx's own connection handshake is.

Co-Authored-By: mj7841@srmist.edu.in
EOF
)"
```

- [ ] **Step 11: Push and open the PR**

```bash
git push -u origin rewrite/03-row-types-pool
gh pr create --title "Async sqlx-backed Pool, real Row column access" --body "$(cat <<'EOF'
## Summary
- Deletes the old sync `Connection`/`ConnectionPool`/`Transaction` (Transaction was already unreachable - `Connection.begin()` never existed)
- New `Pool` class: `execute`/`query`/`query_one`/`close`/`is_closed`, all async via pyo3-asyncio, no GIL held during I/O
- New `Row`: real `keys()`/`values()`/`items()`/`to_dict()`/`get()`/`__iter__`/`__repr__`, and name-based `__getitem__` actually looks up the column instead of always returning column 0
- Float parameters bind as native f64/FLOAT8 - no more forced f32 downcast losing precision
- `Pool.transaction()` intentionally not included here - lands with the Transaction rewrite in the next PR

## Test plan
- [x] `cargo check` passes
- [x] `tests/pool_and_row.py` against a live Postgres: insert/query round-trip, column-name access, float precision, NULL handling, IntegrityError mapping
EOF
)"
```

---

## Task 4: Rewrite `Transaction`, add `Pool.transaction()`

**Files:**
- Create: `src/transaction.rs`
- Modify: `src/pool.rs` (add `transaction()` method)
- Modify: `src/lib.rs` (re-add `mod transaction;` and its registration)
- Test: `tests/transaction.py` (new)

**Interfaces:**
- Consumes: `types::bind_params`, `types::pg_value_to_py` (via `Row::from_pg_row`), `error::{map_db_error, transaction_completed_error}`, `Pool::inner()` (Task 3).
- Produces: `Transaction` pyclass (`execute`, `query`, `query_one`, `commit`, `rollback`, `is_active`, `__aenter__`, `__aexit__`), consumed only by Python callers from here on.

- [ ] **Step 1: Start the branch**

```bash
git checkout master && git pull
git checkout -b rewrite/04-transaction
```

- [ ] **Step 2: Create `src/transaction.rs`**

```rust
use pyo3::prelude::*;
use pyo3::types::PyList;
use sqlx::{Postgres, Transaction as SqlxTransaction};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{map_db_error, transaction_completed_error};
use crate::row::Row;
use crate::types::bind_params;

/// A running transaction. Obtained via `pool.transaction()`; used as
/// `async with pool.transaction() as tx:` for auto-commit on success and
/// auto-rollback on exception, or explicit commit()/rollback().
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
                let py_rows: PyResult<Vec<Row>> =
                    rows.iter().map(|r| Row::from_pg_row(py, r)).collect();
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
```

If `cargo check` (Step 5) reports an executor-trait error on `q.execute(&mut **txn)` / `.fetch_all(&mut **txn)` / `.fetch_one(&mut **txn)`, try `&mut *txn` instead (one fewer deref) — `Transaction<'static, Postgres>` implements `DerefMut<Target = PgConnection>`, and depending on exactly which reference level `guard.as_mut()` hands back, the compiler error will say plainly which one is missing.

- [ ] **Step 3: Add `Pool.transaction()`**

In `src/pool.rs`, add to the `impl Pool` `#[pymethods]` block:

```rust
    /// Begin a transaction. Use as `async with pool.transaction() as tx:`.
    fn transaction<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let pool = self.pool.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let txn = pool.begin().await.map_err(map_db_error)?;
            Ok(crate::transaction::Transaction::new(txn))
        })
    }
```

And drop the now-used `pub(crate) fn inner(&self) -> &PgPool` helper added in Task 3 if this method ends up not needing it (it doesn't — `pool.begin()` works directly off the cloned `PgPool`); remove that helper and the `_pg_value_to_py` unused-import workaround from Task 3 now that nothing needs them.

- [ ] **Step 4: Re-wire `src/lib.rs`**

Add back:
```rust
mod transaction;
```
next to the other `mod` declarations, and register the class in the `PostPyro` function body:
```rust
    m.add_class::<crate::transaction::Transaction>()?;
```

- [ ] **Step 5: Build**

Run: `cargo check`
Expected: succeeds, no unused-code warnings left over from Task 3's placeholders.

- [ ] **Step 6: Write the integration test**

Create `tests/transaction.py`:

```python
import asyncio
import PostPyro


async def main():
    pool = await PostPyro.connect("postgresql://postgres:postgres@localhost:5433/postgres", max_size=5)
    await pool.execute("DROP TABLE IF EXISTS txn_test")
    await pool.execute("CREATE TABLE txn_test (id INT4, balance INT4)")
    await pool.execute("INSERT INTO txn_test (id, balance) VALUES (1, 100)")

    # Explicit commit persists.
    tx = await pool.transaction()
    await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [10, 1])
    await tx.commit()
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 90, row["balance"]

    # Explicit rollback discards.
    tx = await pool.transaction()
    await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [50, 1])
    await tx.rollback()
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 90, row["balance"]

    # Context manager: exception triggers auto-rollback.
    try:
        async with pool.transaction() as tx:
            await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [50, 1])
            raise RuntimeError("boom")
    except RuntimeError:
        pass
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 90, row["balance"]

    # Context manager: clean exit auto-commits.
    async with pool.transaction() as tx:
        await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [40, 1])
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 50, row["balance"]

    # Using a transaction after commit raises ProgrammingError, not a panic/hang.
    tx = await pool.transaction()
    await tx.commit()
    try:
        await tx.execute("SELECT 1")
        assert False, "expected ProgrammingError"
    except PostPyro.ProgrammingError:
        pass

    await pool.execute("DROP TABLE txn_test")
    await pool.close()
    print("OK: transaction commit/rollback/context-manager verified")


asyncio.run(main())
```

- [ ] **Step 7: Run the integration test**

Run:
```bash
docker run -d --rm --name postpyro-test-pg -e POSTGRES_PASSWORD=postgres -p 5433:5432 postgres:16
sleep 3
maturin develop
python3 tests/transaction.py
docker stop postpyro-test-pg
```
Expected: prints `OK: transaction commit/rollback/context-manager verified`, exit code 0.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
Rewrite Transaction on real sqlx::Transaction, add Pool.transaction()

Backed by an owned sqlx::Transaction<'static, Postgres> from
Pool::begin() rather than manual BEGIN/COMMIT/ROLLBACK SQL strings.
Supports both explicit commit()/rollback() and
`async with pool.transaction() as tx:` with auto-commit on clean exit
and auto-rollback on exception.

Co-Authored-By: mj7841@srmist.edu.in
EOF
)"
```

- [ ] **Step 9: Push and open the PR**

```bash
git push -u origin rewrite/04-transaction
gh pr create --title "Rewrite Transaction on sqlx, add Pool.transaction()" --body "$(cat <<'EOF'
## Summary
- New `Transaction`, backed by a real owned `sqlx::Transaction<'static, Postgres>` (no more manual BEGIN/COMMIT/ROLLBACK SQL strings)
- `Pool.transaction()` async factory
- `async with pool.transaction() as tx:` - auto-commit on clean exit, auto-rollback on exception
- Using a transaction after commit/rollback raises `ProgrammingError` cleanly

## Test plan
- [x] `cargo check` passes
- [x] `tests/transaction.py` against a live Postgres: commit, rollback, context-manager auto-commit, context-manager auto-rollback-on-exception, post-completion use raises
EOF
)"
```

---

## Task 5: Drop now-unused dependencies, update the Python package surface

**Files:**
- Modify: `Cargo.toml`
- Modify: `python/PostPyro/__init__.py`
- Modify: `python/PostPyro/__init__.pyi`

**Interfaces:**
- Consumes: final `Pool`/`Row`/`Transaction`/`connect` surface from Tasks 3-4.
- Produces: the public Python import surface (`PostPyro.connect`, `PostPyro.Pool`, `PostPyro.Row`, `PostPyro.Transaction`, exceptions, constants).

- [ ] **Step 1: Start the branch**

```bash
git checkout master && git pull
git checkout -b rewrite/05-cleanup-and-python-package
```

- [ ] **Step 2: Check which old dependencies are actually unused now**

Run: `cargo machete 2>/dev/null || cargo +nightly udeps 2>/dev/null || true`

If neither is installed, check manually: `tokio-postgres`, `deadpool-postgres`, `lru`, `compact_str`, `hex` were only used by the deleted `connection.rs`/old `pool.rs`/old `types.rs`. Confirm with:
```bash
grep -rln "tokio_postgres\|deadpool_postgres\|compact_str\|lru::" src/
```
Expected: no matches (everything that used them was deleted in Tasks 3-4).

- [ ] **Step 3: Remove the unused dependencies from `Cargo.toml`**

Remove `tokio-postgres`, `deadpool-postgres`, `lru`, `compact_str`, `hex` from `[dependencies]`. Keep `tokio`, `postgres-types` (sqlx doesn't re-export it, but confirm nothing references it before removing — if unused, drop it too), `bytes`, `chrono`, `serde_json`, `uuid`, `once_cell`, `smallvec`, `parking_lot` only if something still uses them (`once_cell` is still used by `runtime.rs`; `smallvec`/`parking_lot` were only used by the old `types.rs` string-interning cache, which is gone — remove them too unless Step 2's grep shows otherwise).

- [ ] **Step 4: Verify the build after trimming**

Run: `cargo check`
Expected: succeeds with no new "unresolved import" errors. If something breaks, the dependency was still in use — put it back and re-check.

- [ ] **Step 5: Rewrite `python/PostPyro/__init__.py`**

```python
"""
Async PostgreSQL driver for Python, built on sqlx and pyo3-asyncio.

Features:
- Fully async: every I/O-bound call is `await`-able and releases the GIL
  while waiting on Postgres (no blocking, no held GIL during a query)
- Connection pooling (sqlx::PgPool) - construct via `connect()`
- Row access by index or column name, `keys()`/`values()`/`items()`/`to_dict()`
- DB-API 2.0 compliant exception hierarchy
- TLS-capable connections (rustls)

Usage:
    import asyncio
    import PostPyro

    async def main():
        pool = await PostPyro.connect("postgresql://user:pass@host/db", max_size=20)

        await pool.execute("INSERT INTO users (name) VALUES ($1)", ["John"])
        rows = await pool.query("SELECT * FROM users WHERE active = $1", [True])
        for row in rows:
            print(row["name"], row.to_dict())

        async with pool.transaction() as tx:
            await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [10, 1])
            # auto-commit on success, auto-rollback on exception

        await pool.close()

    asyncio.run(main())
"""

from .PostPyro import (
    # Main classes and factory
    Pool, Row, Transaction, connect,

    # DB-API 2.0 Exceptions
    DatabaseError, InterfaceError, DataError, OperationalError,
    IntegrityError, InternalError, ProgrammingError, NotSupportedError,

    # Constants
    __version__, apilevel, threadsafety, paramstyle,
)

__all__ = [
    "Pool", "Row", "Transaction", "connect",
    "DatabaseError", "InterfaceError", "DataError", "OperationalError",
    "IntegrityError", "InternalError", "ProgrammingError", "NotSupportedError",
    "__version__", "apilevel", "threadsafety", "paramstyle",
]
```

- [ ] **Step 6: Rewrite `python/PostPyro/__init__.pyi`**

```python
"""
Type stubs for PostPyro - async PostgreSQL driver for Python.
"""

from typing import Any, Dict, List, Optional, Union, Iterator, Tuple

__version__: str
apilevel: str
threadsafety: int
paramstyle: str

class DatabaseError(Exception): ...
class InterfaceError(DatabaseError): ...
class DataError(DatabaseError): ...
class OperationalError(DatabaseError): ...
class IntegrityError(DatabaseError): ...
class InternalError(DatabaseError): ...
class ProgrammingError(DatabaseError): ...
class NotSupportedError(DatabaseError): ...

class Row:
    def __len__(self) -> int: ...
    def __getitem__(self, key: Union[int, str]) -> Any: ...
    def __iter__(self) -> Iterator[Any]: ...
    def __repr__(self) -> str: ...
    def get(self, key: Union[int, str], default: Any = None) -> Any: ...
    def keys(self) -> List[str]: ...
    def values(self) -> List[Any]: ...
    def items(self) -> List[Tuple[str, Any]]: ...
    def to_dict(self) -> Dict[str, Any]: ...

class Transaction:
    async def execute(self, query: str, params: Optional[List[Any]] = None) -> int: ...
    async def query(self, query: str, params: Optional[List[Any]] = None) -> List[Row]: ...
    async def query_one(self, query: str, params: Optional[List[Any]] = None) -> Row: ...
    async def commit(self) -> None: ...
    async def rollback(self) -> None: ...
    def is_active(self) -> bool: ...
    async def __aenter__(self) -> "Transaction": ...
    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool: ...

class Pool:
    async def execute(self, query: str, params: Optional[List[Any]] = None) -> int: ...
    async def query(self, query: str, params: Optional[List[Any]] = None) -> List[Row]: ...
    async def query_one(self, query: str, params: Optional[List[Any]] = None) -> Row: ...
    async def transaction(self) -> Transaction: ...
    async def close(self) -> None: ...
    def is_closed(self) -> bool: ...

async def connect(dsn: str, max_size: int = 10, min_size: int = 0) -> Pool: ...
```

- [ ] **Step 7: Full end-to-end smoke test through the actual `PostPyro` package (not the raw `.PostPyro` extension)**

Run:
```bash
docker run -d --rm --name postpyro-test-pg -e POSTGRES_PASSWORD=postgres -p 5433:5432 postgres:16
sleep 3
maturin develop
python3 -c "
import asyncio
import PostPyro

async def main():
    pool = await PostPyro.connect('postgresql://postgres:postgres@localhost:5433/postgres')
    rows = await pool.query('SELECT 1 AS one')
    assert rows[0]['one'] == 1
    await pool.close()
    print('OK: PostPyro package surface works end to end')

asyncio.run(main())
"
docker stop postpyro-test-pg
```
Expected: prints `OK: PostPyro package surface works end to end`, exit code 0.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
Drop unused deps, update Python package surface for async API

Removes tokio-postgres/deadpool-postgres/lru/compact_str/hex (only
used by the deleted sync driver). __init__.py and the .pyi stub now
describe the async Pool/Row/Transaction/connect surface instead of
the old Connection/ConnectionPool/begin() API that never actually
matched the implementation.

Co-Authored-By: mj7841@srmist.edu.in
EOF
)"
```

- [ ] **Step 9: Push and open the PR**

```bash
git push -u origin rewrite/05-cleanup-and-python-package
gh pr create --title "Drop unused deps, update Python package for async API" --body "$(cat <<'EOF'
## Summary
- Removes dependencies only the deleted sync driver used
- `__init__.py`/`__init__.pyi` now describe the real async `Pool`/`Row`/`Transaction`/`connect` surface

## Test plan
- [x] `cargo check` passes after dependency trim
- [x] End-to-end smoke test importing `PostPyro` (the Python package, not the raw extension) against a live Postgres
EOF
)"
```

---

## Task 6: Rewrite documentation for the async API

**Files:**
- Modify: `README.md`
- Modify: `PostPyro_documentation.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: the final API surface from Tasks 3-5. No code interfaces produced.

- [ ] **Step 1: Start the branch**

```bash
git checkout master && git pull
git checkout -b rewrite/06-docs
```

- [ ] **Step 2: Rewrite `README.md`**

Replace every code example using `PostPyro.Connection(...)`, `conn.execute(...)` (sync), `conn.begin()`, and `PostPyro.ConnectionPool(...)` with the async equivalents:

```python
import asyncio
import PostPyro

async def main():
    pool = await PostPyro.connect("postgresql://user:pass@host/db", max_size=20)
    await pool.execute("INSERT INTO users (name) VALUES ($1)", ["John"])
    rows = await pool.query("SELECT * FROM users WHERE active = $1", [True])
    async with pool.transaction() as tx:
        await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [10, 1])
    await pool.close()

asyncio.run(main())
```

Update the API reference sections to match the `.pyi` stub written in Task 5 exactly (method names, async-ness, signatures) — this is the gap that caused the original bug report (`Connection.begin()` was documented but never implemented); the fix for this task is making sure every documented method is one Task 5's stub actually declares. Update the status/classifiers language to describe this as a beta async rewrite, not "Production/Stable" (also fix `pyproject.toml`'s `Development Status :: 5 - Production/Stable` classifier to `Development Status :: 3 - Alpha` while touching this).

- [ ] **Step 3: Rewrite `PostPyro_documentation.md`**

Same treatment — replace every `conn.begin()`/sync example with the async `Pool`/`Transaction` API, and remove any documented method that Task 5's `.pyi` doesn't actually declare.

- [ ] **Step 4: Add a `CHANGELOG.md` entry**

```markdown
## [2.0.0] - Unreleased

### Changed
- **Breaking:** Replaced the synchronous `tokio-postgres`/`deadpool-postgres`-backed driver with an async driver built on `sqlx` + `pyo3-asyncio`. Every I/O method is now `async def`/`await` and releases the GIL during I/O instead of blocking the whole process.
- **Breaking:** `Connection` and `ConnectionPool` are unified into a single `Pool` class, constructed via `await PostPyro.connect(dsn, max_size=10, min_size=0)` instead of `PostPyro.Connection(dsn)`/`PostPyro.ConnectionPool(dsn)`.
- **Breaking:** `Transaction` is obtained via `await pool.transaction()` (previously undocumented-but-broken `Connection.begin()`, which never actually worked).

### Fixed
- `Row` column-name access (`row["column"]`) now looks up the actual column instead of always returning column 0.
- Float parameters no longer lose precision to a forced `f32` downcast before binding.
- `Row.keys()`/`values()`/`items()`/`to_dict()`/`get()`/`__iter__`/`__repr__` are implemented (previously documented but missing).

### Added
- TLS-capable connections (via `sqlx`'s `rustls` backend) - the old driver hardcoded `NoTls`.
```

- [ ] **Step 5: Update `pyproject.toml`'s classifier**

Change `"Development Status :: 5 - Production/Stable"` to `"Development Status :: 3 - Alpha"`.

- [ ] **Step 6: Commit**

```bash
git add README.md PostPyro_documentation.md CHANGELOG.md pyproject.toml
git commit -m "$(cat <<'EOF'
Rewrite README/documentation/changelog for the async API

Every example now matches the real Pool/Row/Transaction/connect
surface implemented in the previous PRs - closing the doc-vs-
implementation gap that had Connection.begin() documented for
releases where it was never actually implemented. Also corrects the
PyPI classifier from Production/Stable to Alpha to match reality.

Co-Authored-By: mj7841@srmist.edu.in
EOF
)"
```

- [ ] **Step 7: Push and open the PR**

```bash
git push -u origin rewrite/06-docs
gh pr create --title "Rewrite documentation for the async API" --body "$(cat <<'EOF'
## Summary
- README and PostPyro_documentation.md now describe the real async Pool/Row/Transaction/connect surface
- CHANGELOG entry for the 2.0.0 breaking rewrite
- pyproject.toml classifier corrected from Production/Stable to Alpha

## Test plan
- [x] Every code sample in README.md matches a method actually declared in python/PostPyro/__init__.pyi
EOF
)"
```
