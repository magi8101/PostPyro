# PostPyro Documentation

**Async PostgreSQL Driver for Python Built with Rust**

PostPyro is an async PostgreSQL driver for Python built with PyO3/`pyo3-asyncio` and `sqlx`. Every I/O method is `async def` and releases the GIL while waiting on Postgres.

> **Status: Beta.** This document describes the current, real API surface (`python/PostPyro/__init__.pyi`) of an in-progress async rewrite - not a roadmap or aspiration. If a method isn't documented here, it doesn't exist yet.

This is the full reference - every method, type, and exception. If you just want a query running, [`README.md`](README.md) has a shorter quick start.

PostPyro runs `sqlx`'s binary protocol under PyO3, so every `await` releases the GIL for the duration of the wait instead of blocking the interpreter. Rust's ownership system rules out the memory-safety bugs (leaks, segfaults) that hand-rolled C bindings are prone to. TLS is available through `sqlx`'s `rustls` backend, and errors surface through a DB-API 2.0-flavored exception hierarchy rather than raw `sqlx` errors. The type system covers booleans, integers, floats, `NUMERIC`, text, dates/times, UUIDs, and JSON/JSONB automatically - see the [type table](#supported-type-conversions) for exactly what's decodable today (arrays, `BYTEA`, and network types aren't yet).

## Installation

```bash
pip install PostPyro
```

## Quick Start

```python
import asyncio
import PostPyro

async def main():
    pool = await PostPyro.connect("postgresql://user:password@localhost:5432/database", max_size=20)

    # Simple query
    users = await pool.query("SELECT id, name, email FROM users WHERE active = $1", [True])
    for user in users:
        print(f"User: {user['name']} ({user['email']})")

    # Insert data
    affected = await pool.execute(
        "INSERT INTO users (name, email) VALUES ($1, $2)",
        ["John Doe", "john@example.com"]
    )

    # Close the pool
    await pool.close()

asyncio.run(main())
```

## API Reference

### Module Constants

```python
PostPyro.__version__      # Driver version
PostPyro.apilevel         # "2.0" (DB-API 2.0-flavored)
PostPyro.threadsafety     # Thread-safety level
PostPyro.paramstyle       # "numeric" (PostgreSQL $1, $2 style)
```

### Functions

#### `await connect(dsn: str, max_size: int = 10, min_size: int = 0) -> Pool`

Create a connection pool. This is the only way to get a `Pool` - there is no synchronous constructor, since establishing the first connection is itself async (a real network round-trip).

```python
pool = await PostPyro.connect("postgresql://user:pass@host:port/database", max_size=20, min_size=2)

# Connection string formats supported:
# - postgresql://user:password@host:port/database
# - postgres://user:password@host:port/database
# - With SSL: postgresql://user:pass@host/db?sslmode=require
```

## Pool Class

`Pool` is a connection pool (backed by `sqlx::PgPool`). There is no separate single-connection class - `Pool` is the entry point for all queries, whether you're using one connection or many.

### Methods

#### `await pool.query(sql: str, params: List = None) -> List[Row]`

Execute a SELECT query and return all rows.

```python
# Simple query
rows = await pool.query("SELECT * FROM users")

# Parameterized query
rows = await pool.query("SELECT * FROM users WHERE age > $1 AND city = $2", [25, "New York"])

# Process results
for row in rows:
    print(f"ID: {row['id']}, Name: {row['name']}")
```

#### `await pool.query_one(sql: str, params: List = None) -> Row`

Execute a query and return its first row. Raises `ProgrammingError` if zero
rows are returned. If more than one row matches, the extras are silently
discarded rather than raising - only use this when your query itself
guarantees at most one row (a primary-key lookup, `LIMIT 1`, etc.).

```python
user = await pool.query_one("SELECT * FROM users WHERE id = $1", [123])
print(f"User: {user['name']}")
```

#### `await pool.execute(sql: str, params: List = None) -> int`

Execute INSERT, UPDATE, DELETE, or DDL statements. Returns the number of affected rows.

```python
# Insert
affected = await pool.execute(
    "INSERT INTO users (name, email) VALUES ($1, $2)",
    ["Alice", "alice@example.com"]
)

# Update
affected = await pool.execute(
    "UPDATE users SET email = $1 WHERE id = $2",
    ["newemail@example.com", 123]
)

# Delete
affected = await pool.execute("DELETE FROM users WHERE active = $1", [False])

# DDL
await pool.execute("CREATE TABLE products (id SERIAL PRIMARY KEY, name TEXT)")
```

#### `await pool.transaction() -> Transaction`

Start a new transaction (a real `BEGIN` round-trip). Because starting one is itself async, you must `await` it *before* using it as a context manager - `async with pool.transaction():` raises `TypeError`, since the un-awaited coroutine has no `__aenter__`.

```python
tx = await pool.transaction()
async with tx:
    await tx.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    await tx.execute("INSERT INTO orders (user_id) VALUES ($1)", [123])
    # Automatically commits on successful exit
```

#### `await pool.close() -> None`

Close the pool and its connections.

```python
await pool.close()
```

#### `pool.is_closed() -> bool`

Check if the pool is closed. Synchronous - no `await`.

```python
if not pool.is_closed():
    await pool.query("SELECT 1")
```

## Row Class

Represents a single row from a query result with dict-like interface.

### Methods

#### `__getitem__(key: Union[int, str]) -> Any`

Access column values by index or name.

```python
row = await pool.query_one("SELECT id, name, email FROM users WHERE id = $1", [1])

# Access by column name
print(row['name'])
print(row['email'])

# Access by index
print(row[0])  # id
print(row[1])  # name
```

#### `__len__() -> int`, `__iter__()`, `get(key, default=None)`, `keys()`, `values()`, `items()`, `to_dict()`

`Row` mirrors a read-only mapping's interface without being one: `len()`, iteration, `.get()` with a default, and `keys()`/`values()`/`items()` all work, but the latter three return plain `list`s (per the `.pyi`), not `dict` view objects - no set operations (`&`, `|`) and no live reflection of later changes. `to_dict()` gives you a real `dict` when something downstream (pandas, `json.dumps`, ...) needs one instead of a `Row`.

```python
column_count = len(row)
for value in row:
    print(value)
age = row.get('age', 0)
print(row.keys())
print(row.values())
for column, value in row.items():
    print(f"{column}: {value}")
user_dict = row.to_dict()
```

## Transaction Class

Obtained via `await pool.transaction()`. Represents a database transaction with automatic commit/rollback when used as an `async with` block.

### Methods

#### `await tx.execute(sql: str, params: List = None) -> int`

Execute a statement within the transaction.

```python
tx = await pool.transaction()
async with tx:
    await tx.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [100, 1])
```

#### `await tx.query(sql: str, params: List = None) -> List[Row]`

Execute a query within the transaction.

```python
tx = await pool.transaction()
async with tx:
    users = await tx.query("SELECT * FROM users WHERE created_today = true")
    for user in users:
        await tx.execute("UPDATE users SET welcomed = true WHERE id = $1", [user['id']])
```

#### `await tx.query_one(sql: str, params: List = None) -> Row`

Execute a query returning one row within the transaction.

#### `await tx.commit() -> None`

Explicitly commit the transaction. Also works without an `async with` block.

```python
tx = await pool.transaction()
await tx.execute("INSERT INTO logs (message) VALUES ($1)", ["Started process"])
await tx.commit()
```

#### `await tx.rollback() -> None`

Explicitly roll back the transaction.

#### `tx.is_active() -> bool`

Check whether the transaction is still open (synchronous). Returns `False` after `commit()` or `rollback()`. Using `execute`/`query`/`query_one` after that point raises `ProgrammingError` rather than hanging or panicking.

## Error Handling

PostPyro maps every `sqlx`/Postgres error onto the DB-API 2.0-flavored hierarchy below, so you can catch by class instead of parsing SQLSTATE codes yourself.

### Exception Hierarchy

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

### Error Handling Example

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
        print(f"SQL error: {e}")
    except PostPyro.DatabaseError as e:
        print(f"General database error: {e}")
```

## Type System

PostPyro automatically converts between Python and PostgreSQL types.

### Supported Type Conversions

| PostgreSQL Type                       | Python Type          | Example                                        |
| -------------------------------------- | --------------------- | ------------------------------------------------- |
| `BOOLEAN`                             | `bool`               | `True`, `False`                                |
| `SMALLINT`, `INTEGER`, `BIGINT`       | `int`                | `42`, `-123`, `9223372036854775807`            |
| `REAL`, `DOUBLE PRECISION`            | `float`               | `3.14`, `2.718`                                |
| `NUMERIC`                             | `decimal.Decimal`     | `Decimal('1234.5678')`                         |
| `TEXT`, `VARCHAR`, `CHAR`, `NAME`     | `str`                 | `"Hello World"`                                |
| `DATE`                                | `datetime.date`       | `date(2023, 12, 25)`                           |
| `TIME`                                | `datetime.time`       | `time(14, 30, 0)`                              |
| `TIMESTAMP`                           | `datetime.datetime`   | `datetime(2023, 12, 25, 14, 30)`               |
| `TIMESTAMPTZ`                         | `datetime.datetime`   | With timezone info (UTC)                       |
| `UUID`                                | `str`                 | `'550e8400-e29b-41d4-a716-446655440000'`       |
| `JSON`, `JSONB`                       | `dict`, `list`, etc.  | `{"key": "value"}`, `[1, 2, 3]`                |

Any other type (`BYTEA`, arrays, `INET`/`CIDR`, custom/enum types, etc.) isn't
decodable yet - reading such a column raises `PostPyro.NotSupportedError`
naming the type, rather than silently returning the wrong value or `None`.

### Type Usage Example

Binding parameters only auto-converts `bool`/`int`/`float`/`str`/`None` -
anything else (a `date`, `datetime`, `uuid.UUID`, `dict`, ...) has no
automatic Python→Postgres conversion. Pass it as a string and add an
explicit Postgres cast in the SQL text instead:

```python
import json
import uuid
from datetime import date

# Insert various types - non-primitive values go in as strings, with an
# explicit ::type cast telling Postgres what to parse them as.
await pool.execute("""
    INSERT INTO mixed_types (
        bool_col, int_col, float_col, text_col,
        date_col, timestamp_col, uuid_col, json_col
    ) VALUES ($1, $2, $3, $4, $5::date, $6::timestamp, $7::uuid, $8::jsonb)
""", [
    True,                                     # boolean - auto-converted
    42,                                       # integer - auto-converted
    3.14159,                                  # float - auto-converted
    "Hello PostgreSQL",                       # text - auto-converted
    "2023-12-25",                             # date - string + ::date cast
    "2023-12-25 14:30:00",                    # timestamp - string + ::timestamp cast
    str(uuid.uuid4()),                        # uuid - string + ::uuid cast
    json.dumps({"name": "John", "scores": [85, 92, 78]}),  # json - string + ::jsonb cast
])

# Reading back decodes to the right Python type automatically (this
# direction IS automatic - see the table above).
row = await pool.query_one("SELECT * FROM mixed_types WHERE id = $1", [1])
assert isinstance(row['bool_col'], bool)
assert isinstance(row['date_col'], date)
assert isinstance(row['json_col'], dict)
```

## Integration with Other Libraries

### Pandas Integration

```python
import pandas as pd
import PostPyro

pool = await PostPyro.connect("postgresql://user:pass@localhost/db")

# Query to DataFrame
rows = await pool.query("SELECT * FROM sales_data")
df = pd.DataFrame([row.to_dict() for row in rows])

print(df.head())
```

### FastAPI Integration

FastAPI is natively async, so a `Pool` fits it directly - no thread-pool bridging needed.

```python
from fastapi import FastAPI, HTTPException
import PostPyro

app = FastAPI()
pool: PostPyro.Pool

@app.on_event("startup")
async def startup():
    global pool
    pool = await PostPyro.connect("postgresql://user:pass@localhost/db")

@app.on_event("shutdown")
async def shutdown():
    await pool.close()

@app.get("/users/{user_id}")
async def get_user(user_id: int):
    try:
        user = await pool.query_one(
            "SELECT id, name, email FROM users WHERE id = $1",
            [user_id]
        )
        return user.to_dict()
    except PostPyro.DatabaseError:
        raise HTTPException(status_code=404, detail="User not found")

@app.post("/users")
async def create_user(name: str, email: str):
    try:
        await pool.execute(
            "INSERT INTO users (name, email) VALUES ($1, $2)",
            [name, email]
        )
        return {"message": "User created"}
    except PostPyro.IntegrityError:
        raise HTTPException(status_code=400, detail="Email already exists")
```

### Lightweight Repository Pattern

```python
import PostPyro

class UserRepository:
    def __init__(self, pool: PostPyro.Pool):
        self.pool = pool

    async def find_by_email(self, email: str):
        row = await self.pool.query_one("SELECT * FROM users WHERE email = $1", [email])
        return row.to_dict()

    async def save(self, name: str, email: str):
        return await self.pool.execute(
            "INSERT INTO users (name, email) VALUES ($1, $2)",
            [name, email]
        )

# Usage
pool = await PostPyro.connect("postgresql://user:pass@localhost/db")
users = UserRepository(pool)
await users.save("Alice", "alice@example.com")
```

## Performance Notes

- **Rust backend**: parsing and encoding happen in Rust, not the Python interpreter
- **Async I/O**: `sqlx`'s async networking releases the GIL during I/O instead of blocking the whole process
- **Binary protocol**: query results come back over Postgres's binary wire protocol, parsed in Rust

The performance comparisons published for earlier (pre-rewrite, synchronous) versions of PostPyro no longer apply to this async driver and have not been re-benchmarked yet; treat any such numbers you find elsewhere as stale.

## Advanced Usage

### Pooling

`Pool` (from `connect()`) already *is* a connection pool - there's no need to hand-roll one on top of it. Set `max_size`/`min_size` on `connect()` to size it:

```python
pool = await PostPyro.connect("postgresql://user:pass@localhost/db", max_size=20, min_size=2)
```

### Batch Processing Pattern

Group related writes in a single transaction rather than issuing them one at a time outside of one:

```python
async def bulk_insert_users(pool: PostPyro.Pool, users_data):
    """Insert many users inside a single transaction."""
    tx = await pool.transaction()
    async with tx:
        for user_data in users_data:
            await tx.execute(
                "INSERT INTO users (name, email, age) VALUES ($1, $2, $3)",
                [user_data['name'], user_data['email'], user_data['age']]
            )
```

### Error Recovery Pattern

```python
import asyncio
import PostPyro

async def robust_query(pool: PostPyro.Pool, sql, params=None, max_retries=3):
    """Execute a query with automatic retry on operational errors."""
    for attempt in range(max_retries):
        try:
            return await pool.query(sql, params)
        except (PostPyro.OperationalError, PostPyro.InterfaceError) as e:
            if attempt == max_retries - 1:
                raise
            print(f"Connection error (attempt {attempt + 1}): {e}")
            await asyncio.sleep(2 ** attempt)  # Exponential backoff
        except PostPyro.DatabaseError:
            # Don't retry on SQL errors
            raise
```

## Best Practices

### 1. Pool Lifecycle

```python
# ✅ Good: one Pool for the app's lifetime, closed on shutdown
pool = await PostPyro.connect("postgresql://...")
try:
    result = await pool.query("SELECT * FROM users")
finally:
    await pool.close()

# ❌ Bad: opening a new pool per request/call
async def handler():
    pool = await PostPyro.connect("postgresql://...")  # expensive, leaks connections
    return await pool.query("SELECT * FROM users")
```

### 2. Parameter Binding

```python
# ✅ Good: always use parameters
user_id = 123
await pool.query("SELECT * FROM users WHERE id = $1", [user_id])

# ❌ Bad: string formatting (SQL injection risk)
await pool.query(f"SELECT * FROM users WHERE id = {user_id}")
```

### 3. Transaction Usage

```python
# ✅ Good: use a transaction for multiple related operations
tx = await pool.transaction()
async with tx:
    await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [100, 1])
    await tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", [100, 2])

# ❌ Bad: separate un-transacted operations that should be atomic
await pool.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [100, 1])
await pool.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", [100, 2])
```

### 4. Error Handling

```python
# ✅ Good: specific error handling
try:
    await pool.execute("INSERT INTO users (email) VALUES ($1)", [email])
except PostPyro.IntegrityError:
    print("Email already exists")
except PostPyro.ProgrammingError:
    print("SQL syntax error")

# ❌ Bad: generic error handling
try:
    await pool.execute("INSERT INTO users (email) VALUES ($1)", [email])
except Exception as e:
    print(f"Something went wrong: {e}")
```

## Configuration

### Connection String Options

```python
# Basic connection
pool = await PostPyro.connect("postgresql://user:pass@localhost:5432/database")

# With SSL
pool = await PostPyro.connect("postgresql://user:pass@host/db?sslmode=require")

# With connection timeout
pool = await PostPyro.connect("postgresql://user:pass@host/db?connect_timeout=10")

# Pool sizing
pool = await PostPyro.connect("postgresql://user:pass@host/db", max_size=20, min_size=2)
```

## Migration from Other Drivers

### From psycopg2 (sync)

```python
# psycopg2
import psycopg2
conn = psycopg2.connect("host=localhost dbname=test user=postgres")
cur = conn.cursor()
cur.execute("SELECT * FROM users WHERE id = %s", (123,))
rows = cur.fetchall()
conn.close()

# PostPyro
import PostPyro
pool = await PostPyro.connect("postgresql://postgres@localhost/test")
rows = await pool.query("SELECT * FROM users WHERE id = $1", [123])
await pool.close()
```

### From asyncpg

Both drivers are async; the shapes are close, with `execute`/`query`/`query_one` in place of asyncpg's `execute`/`fetch`/`fetchrow`.

```python
# asyncpg
import asyncpg
conn = await asyncpg.connect("postgresql://postgres@localhost/test")
rows = await conn.fetch("SELECT * FROM users WHERE id = $1", 123)
await conn.close()

# PostPyro
import PostPyro
pool = await PostPyro.connect("postgresql://postgres@localhost/test")
rows = await pool.query("SELECT * FROM users WHERE id = $1", [123])
await pool.close()
```

## Troubleshooting

**Connection refused / authentication failed**

`connect()` raises a `PostPyro.OperationalError` (or subclass) if it can't establish the pool - catch it around the `await`:

```python
try:
    pool = await PostPyro.connect("postgresql://user:wrongpass@localhost/db")
except PostPyro.OperationalError as e:
    print(f"Connection error: {e}")
```

**Type conversion errors**

```python
# Use proper Python types
await pool.execute("INSERT INTO users (age) VALUES ($1)", [25])    # ✅ int
await pool.execute("INSERT INTO users (age) VALUES ($1)", ["25"])  # ❌ string
```

---

Built with Rust, PyO3, and sqlx. For more examples and advanced usage, see the [GitHub repository](https://github.com/magi8101/PostPyro).
