# 🔥 PostPyro

**Async PostgreSQL driver for Python built with Rust**

[![PyPI version](https://badge.fury.io/py/PostPyro.svg)](https://pypi.org/project/PostPyro/)
[![Python versions](https://img.shields.io/pypi/pyversions/PostPyro)](https://pypi.org/project/PostPyro/)
[![License](https://img.shields.io/pypi/l/PostPyro)](https://github.com/magi8101/PostPyro/blob/main/LICENSE)

PostPyro combines the **speed of Rust** with the **simplicity of Python** for PostgreSQL database operations. Built on `sqlx` and `PyO3`/`pyo3-asyncio`, it delivers a fully async driver with full DB-API 2.0-flavored exception handling.

> **Status: Beta.** PostPyro is mid-rewrite from a synchronous driver to a fully async one. The API described below is the real, current surface (`python/PostPyro/__init__.pyi`) - not a roadmap. Expect breaking changes before a 1.0/stable release.

## 🚀 Why PostPyro?

- **🏎️ Fast**: Rust-powered binary protocol communication via `sqlx`
- **🔒 Type Safe**: Comprehensive Python ↔ PostgreSQL type conversion
- **⚡ Fully Async**: Every I/O method is `async def` and releases the GIL while waiting on Postgres
- **🔐 TLS-capable**: Connections can use `sqlx`'s `rustls` backend
- **🎯 Familiar Errors**: DB-API 2.0-style exception hierarchy
- **🔧 Transactions**: Explicit commit/rollback and `async with` support

## ⚡ Installation

```bash
pip install PostPyro
```

That's it! No compilation, no system dependencies - just pure speed.

## 🎯 Quick Start

```python
import asyncio
import PostPyro

async def main():
    pool = await PostPyro.connect("postgresql://user:pass@localhost:5432/mydb", max_size=20)

    # Execute & Query
    await pool.execute("CREATE TABLE users (id SERIAL, name TEXT, age INTEGER)")
    await pool.execute("INSERT INTO users (name, age) VALUES ($1, $2)", ["Alice", 30])

    # Fetch results
    rows = await pool.query("SELECT * FROM users WHERE age > $1", [25])
    for row in rows:
        print(f"{row['name']} is {row['age']} years old")

    # Transactions
    tx = await pool.transaction()
    async with tx:
        await tx.execute("UPDATE users SET age = age + 1")
        # Auto-commit on success, rollback on error

    await pool.close()

asyncio.run(main())
```

> **Note:** `pool.transaction()` is itself a real `BEGIN` round-trip, so it must be `await`-ed before it can be used as a context manager - `async with pool.transaction():` raises `TypeError` because the un-awaited coroutine has no `__aenter__`.

## API Reference

The reference below matches `python/PostPyro/__init__.pyi` exactly. If a method isn't listed here, it doesn't exist yet.

### Module Constants

```python
PostPyro.__version__   # driver version
PostPyro.apilevel      # "2.0" - DB-API 2.0-flavored
PostPyro.threadsafety  # thread-safety level
PostPyro.paramstyle    # "numeric" - uses $1, $2, ... parameters
```

### `await PostPyro.connect(dsn, max_size=10, min_size=0) -> Pool`

Create a connection pool. This is the only way to get a `Pool` - there is no synchronous constructor, since establishing the first connection is itself async.

```python
pool = await PostPyro.connect("postgresql://user:pass@localhost:5432/mydb", max_size=20, min_size=2)
```

### `Pool` Class

#### `await pool.execute(query, params=None) -> int`

Execute INSERT, UPDATE, DELETE, or DDL statements. Returns the number of rows affected.

```python
affected = await pool.execute("UPDATE users SET age = $1 WHERE name = $2", [31, "Alice"])
```

#### `await pool.query(query, params=None) -> list[Row]`

Execute a SELECT and return all matching rows.

```python
rows = await pool.query("SELECT id, name, age FROM users WHERE age > $1", [25])
for row in rows:
    print(f"ID: {row['id']}, Name: {row['name']}, Age: {row['age']}")
```

#### `await pool.query_one(query, params=None) -> Row`

Execute a SELECT and return exactly one row. Raises an error if zero or multiple rows are returned.

```python
user = await pool.query_one("SELECT * FROM users WHERE id = $1", [1])
print(f"User name: {user['name']}")
```

#### `await pool.transaction() -> Transaction`

Start a new transaction (a real `BEGIN`). Await it, then use the result as an `async with` block.

```python
tx = await pool.transaction()
async with tx:
    await tx.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    await tx.execute("UPDATE accounts SET balance = balance - 100 WHERE user_id = $1", [1])
    # Automatically commits on success, rolls back on exception
```

#### `await pool.close() -> None`

Close the pool and free its connections.

```python
await pool.close()
```

#### `pool.is_closed() -> bool`

Check if the pool has been closed. Synchronous - no `await`.

```python
if not pool.is_closed():
    await pool.query("SELECT 1")
```

### `Row` Class

Represents a single row from a query result with a dict-like interface.

```python
row = await pool.query_one("SELECT id, name, email FROM users WHERE id = $1", [1])

# Access by column name or index
print(row['name'])
print(row[0])  # id

# Get with default
age = row.get('age', 0)

# Length and iteration
print(f"Row has {len(row)} columns")
for value in row:
    print(value)

# Column names / values / pairs
print(list(row.keys()))
print(list(row.values()))
for column, value in row.items():
    print(f"{column}: {value}")

# Convert to dictionary
user_dict = row.to_dict()
```

### `Transaction` Class

Obtained via `await pool.transaction()`. Represents a database transaction.

```python
tx = await pool.transaction()
async with tx:
    await tx.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    users = await tx.query("SELECT * FROM users WHERE created_today = true")
    for user in users:
        await tx.execute("UPDATE users SET welcomed = true WHERE id = $1", [user['id']])
    account = await tx.query_one("SELECT balance FROM accounts WHERE id = $1", [1])
    # Commits automatically on clean exit, rolls back automatically on exception
```

- `await tx.execute(query, params=None) -> int`
- `await tx.query(query, params=None) -> list[Row]`
- `await tx.query_one(query, params=None) -> Row`
- `await tx.commit() -> None` - explicit commit; also works outside a `with` block
- `await tx.rollback() -> None` - explicit rollback
- `tx.is_active() -> bool` - synchronous; `False` after commit/rollback

Using a transaction after it has been committed or rolled back raises `ProgrammingError` rather than hanging or panicking.

### Error Handling

PostPyro provides comprehensive PostgreSQL error mapping with specific exception types.

#### Exception Hierarchy

```python
DatabaseError                    # Base database error
├── InterfaceError              # Driver interface problems
├── DataError                   # Data processing errors
├── OperationalError            # Database operation errors
├── IntegrityError              # Constraint violations
├── InternalError               # Internal database errors
├── ProgrammingError            # SQL programming errors
└── NotSupportedError           # Unsupported operations
```

#### Error Handling Example

```python
import PostPyro

async def main():
    pool = await PostPyro.connect("postgresql://user:pass@localhost/db")
    try:
        await pool.execute("INSERT INTO users (email) VALUES ($1)", ["invalid-email"])
    except PostPyro.IntegrityError as e:
        print(f"Constraint violation: {e}")
    except PostPyro.OperationalError as e:
        print(f"Database operation failed: {e}")
    except PostPyro.ProgrammingError as e:
        print(f"SQL syntax error: {e}")
    except PostPyro.DatabaseError as e:
        print(f"General database error: {e}")
```

### Type System

PostPyro automatically converts between Python and PostgreSQL types.

#### Supported Type Conversions

| PostgreSQL Type            | Python Type         | Example                             |
| --------------------------- | ------------------- | ------------------------------------ |
| `BOOLEAN`                  | `bool`              | `True`, `False`                     |
| `SMALLINT`, `INTEGER`      | `int`               | `42`, `-123`                        |
| `BIGINT`                   | `int`               | `9223372036854775807`               |
| `REAL`, `DOUBLE PRECISION` | `float`             | `3.14`, `2.718`                     |
| `TEXT`, `VARCHAR`          | `str`               | `"Hello World"`                     |
| `BYTEA`                    | `bytes`             | `b"binary data"`                    |
| `DATE`                     | `datetime.date`     | `date(2023, 12, 25)`                |
| `TIME`                     | `datetime.time`     | `time(14, 30, 0)`                   |
| `TIMESTAMP`                | `datetime.datetime` | `datetime(2023, 12, 25, 14, 30)`    |
| `TIMESTAMPTZ`              | `datetime.datetime` | With timezone info                  |
| `UUID`                     | `uuid.UUID`         | `UUID('550e8400-e29b-...')`         |
| `JSON`, `JSONB`            | `dict`, `list`      | `{"key": "value"}`, `[1, 2, 3]`     |
| `ARRAY`                    | `list`              | `[1, 2, 3]`, `["a", "b", "c"]`      |
| `INET`, `CIDR`             | `str`               | `"192.168.1.1"`, `"192.168.0.0/24"` |

#### Type Usage Example

```python
from datetime import datetime, date
import uuid

# Insert various types
await pool.execute("""
    INSERT INTO mixed_types (
        bool_col, int_col, float_col, text_col,
        date_col, timestamp_col, uuid_col, json_col
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
""", [
    True,                                    # boolean
    42,                                      # integer
    3.14159,                                 # float
    "Hello PostgreSQL",                      # text
    date(2023, 12, 25),                     # date
    datetime(2023, 12, 25, 14, 30, 0),      # timestamp
    uuid.uuid4(),                           # uuid
    {"name": "John", "scores": [85, 92, 78]} # json
])

# Query returns properly typed values
row = await pool.query_one("SELECT * FROM mixed_types WHERE id = $1", [1])
assert isinstance(row['bool_col'], bool)
assert isinstance(row['json_col'], dict)
```

## Development

### Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/magi8101/PostPyro.git
cd PostPyro
pip install -e .
```

### Running Tests

```bash
# Start PostgreSQL test instance (using Docker)
docker run -d --name postgres-test -e POSTGRES_PASSWORD=postgres -p 5433:5432 postgres:15

# Run tests
python tests/pool_and_row.py
python tests/transaction.py
python tests/type_conversion_bugs.py
```

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- [sqlx](https://github.com/launchbadge/sqlx) for the async PostgreSQL driver
- [PyO3](https://github.com/PyO3/pyo3) / [pyo3-asyncio](https://github.com/PyO3/pyo3-async-runtimes) for Python-Rust async bindings
