# PostPyro Architecture (v2 — async rewrite)

## Status

Beta, pre-1.0. This document describes a ground-up rewrite of the driver core.
The rewrite has landed and is documented in the README - not for production
use yet regardless, per the project's overall beta status.

## Why this rewrite

The original implementation (`tokio-postgres` + `deadpool-postgres` + hand-rolled
pooling/type/statement-cache code) called every database operation synchronously
via `runtime.block_on(...)` while holding the Python GIL for the full duration —
no method anywhere released it (`allow_threads` was never called). That has two
consequences:

- Every query blocks the entire Python process, not just the calling thread —
  other Python threads, including ones talking to different idle pooled
  connections, cannot run while one query is in flight. `ConnectionPool`'s
  concurrency is unreachable from Python regardless of `max_size`.
- The async runtime, task scheduling, and `Mutex<Client>` locking exist to
  support concurrent, non-blocking operation, but are driven in the most
  blocking way possible — pure overhead with none of the upside.

Review of the original code also found two real correctness bugs baked into
the hand-rolled layers this rewrite retires:

- `Row.__getitem__(str)` was stubbed to always return column 0 regardless of
  the name requested (`src/row.rs`, old version) — silent wrong data.
- Parameter binding unconditionally downcast Python floats to `f32` before
  sending them, truncating precision against `DOUBLE PRECISION`/`FLOAT8`
  columns (`src/types.rs`, old version).

And the documented public API (`Connection.begin()`, most of `Row`'s methods —
`keys()`, `values()`, `items()`, `to_dict()`, `__iter__`) didn't exist in the
Rust implementation at all, despite being in the README, docstrings, and
`.pyi` stub.

## Decisions

1. **Foundation library: `sqlx`** (`postgres` feature, tokio runtime),
   replacing `tokio-postgres` + `deadpool-postgres` + the hand-rolled
   `types.rs` conversion/statement-cache layer.
   - Fixes column-name row access by construction: `sqlx::Row::try_get::<T>("name")`
     is a real, correct lookup — no name→index stub needed.
   - Fixes float precision: `sqlx`'s `Encode`/`Decode` bind against the actual
     Postgres wire type instead of a forced `f32` downcast.
   - Adds TLS support for free (the original code hardcoded `NoTls` — no
     encrypted connections were possible at all).
   - Broader native type coverage via feature flags (`chrono`, `uuid`, `json`,
     `bigdecimal`) instead of hand-rolling conversions. (Started on
     `rust_decimal`; switched to `bigdecimal` during implementation once
     `rust_decimal`'s ~28-29 significant-digit limit turned out to be
     narrower than PostgreSQL's actual `NUMERIC` range.)

2. **Python binding: `pyo3-asyncio` 0.20** (matches the pinned `pyo3 = "0.20"`),
   tokio feature. **Async-only** — no native synchronous Rust API, no
   Python-side thread-pool sync wrapper. Python callers use `async def`/`await`
   directly. (A sync-facade API was considered and explicitly deferred — see
   "Parked ideas" below.)

3. **One shared Tokio runtime**, built once at module init and handed to both
   `pyo3_asyncio::tokio::init_with_runtime` and `sqlx::PgPool`'s builder — a
   single executor backs both the Python↔Rust bridge and the actual I/O, so
   there's no second competing runtime/thread pool. This is the mechanism that
   keeps the two sides balanced: Tokio performs the I/O concurrency, `pyo3-asyncio`
   resumes the caller's own asyncio coroutine on completion, and the GIL is
   held only briefly — at call setup and at result marshaling — never during
   the actual wait on Postgres.

4. **Single crate, no Cargo workspace split.** An earlier draft of this design
   considered splitting a PyO3-free `core` crate from thin sync/async binding
   crates, specifically to stop sync and async APIs from duplicating query/
   type-conversion logic. That motivation is gone now that there's only one
   binding layer (async-only) — a plain, well-organized single crate is
   sufficient and simpler.

5. **`Connection` and `ConnectionPool` unify into one `Pool` class.**
   `sqlx::PgPool` handles `max_size=1` gracefully, so a separate
   single-connection type is redundant complexity — one primitive, one code
   path, no "which one do I use" ambiguity for callers. (Flagged during
   design as the one open question; recorded here as decided since it wasn't
   objected to — revisit if single-connection semantics turn out to matter.)

6. **Same DB-API 2.0 exception hierarchy** (`DatabaseError`, `InterfaceError`,
   `DataError`, `OperationalError`, `IntegrityError`, `InternalError`,
   `ProgrammingError`, `NotSupportedError`) is retained. Mapping is retargeted
   from `sqlx::Error`/its SQLSTATE surface instead of `tokio_postgres::Error`;
   application code catching these exception types doesn't change.

7. **`Row` implements its full previously-documented API for real:**
   `__getitem__` (index and name), `__len__`, `__iter__`, `__repr__`, `get()`,
   `keys()`, `values()`, `items()`, `to_dict()`.

8. **`Transaction`** is used as `async with pool.transaction() as tx:`, backed
   by a real `sqlx::Transaction` (not manual `BEGIN`/`COMMIT`/`ROLLBACK` SQL
   strings as in the original).

## Module layout (target)

- `src/lib.rs` — pymodule init: build the shared Tokio runtime, wire
  `pyo3-asyncio`, register classes and exceptions.
- `src/runtime.rs` — shared runtime construction/handle.
- `src/pool.rs` — `Pool` pyclass wrapping `sqlx::PgPool`; `execute`/`query`/
  `query_one`/`transaction`, all returning awaitables via `future_into_py`.
- `src/transaction.rs` — `Transaction` pyclass wrapping `sqlx::Transaction`.
- `src/row.rs` — `Row` pyclass wrapping `sqlx::postgres::PgRow` with a real
  column-name index.
- `src/types.rs` — Python ↔ `sqlx` value conversion, mostly delegating to
  `sqlx`'s `Encode`/`Decode` via feature flags rather than hand-rolling.
- `src/error.rs` — `sqlx::Error` → DB-API 2.0 exception mapping, same
  SQLSTATE-based classification shape as the original.

## Data flow (one query)

1. Python calls `await pool.query(sql, params)` — an awaitable is returned
   immediately.
2. `pyo3-asyncio` spawns the future on the shared runtime; the GIL is
   released for the duration of the wait.
3. `sqlx` acquires a pooled connection, binds parameters via `query_with`,
   executes, and awaits the result.
4. Result rows convert to `Row` objects (column names resolved once per
   statement, not per cell).
5. The GIL is reacquired only to marshal the final Python objects and resolve
   the awaitable.

## Testing

- `cargo test`: pure-Rust unit tests for type conversion and error
  classification — no Postgres or Python runtime required.
- A small integration suite against a real Postgres (docker-compose in CI)
  covering `query`/`transaction`/`Row` behavior end to end.

## Process

- Implementation proceeds as feature-branch PRs per slice of this design (not
  direct commits to the default branch).
- `README.md` and the `.pyi` stub are rewritten once the implementation
  plan's slices land and the API has actually stabilized — sequenced as part
  of implementation, not ahead of it, so documentation describes real,
  working code rather than an aspirational surface (this is exactly the gap
  found in the original codebase's docs vs. implementation).

## Parked ideas (not in scope for this rewrite)

- **Out-of-process pool daemon** — a separate long-lived process owning the
  connection pool, letting multiple Python *processes* (e.g. Gunicorn workers
  under an ASGI deployment) share one real pool instead of each opening its
  own. Architecturally the "correct" answer to multi-process pool
  fragmentation, but it's a different product (a service to operate, not a
  library) — revisit if that fragmentation becomes an actual pain point.
- **Python-side synchronous thread-pool wrapper** — a compatibility class
  running a background event loop per worker thread, forwarding calls to the
  async core via `run_coroutine_threadsafe`, for callers who don't use
  `asyncio`. Deferred so the async core's API gets to stabilize before a
  second surface is layered on top of it.
