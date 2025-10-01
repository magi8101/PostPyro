# PostPyro

A high-performance PostgreSQL driver for Python using PyO3 and tokio-postgres.

[![PyPI version](https://badge.fury.io/py/PostPyro.svg)](https://pypi.org/project/PostPyro/)
[![Python versions](https://img.shields.io/pypi/pyversions/PostPyro)](https://pypi.org/project/PostPyro/)
[![License](https://img.shields.io/pypi/l/PostPyro)](https://github.com/magi8101/PostPyro/blob/main/LICENSE)

## Features

- **High Performance**: Rust backend with PyO3 bindings for maximum speed
- **Full DB-API 2.0 Compliance**: Compatible with existing Python database code
- **Async I/O**: Built on tokio-postgres for efficient asynchronous operations
- **Type Safety**: Comprehensive type conversion between Python and PostgreSQL
- **Transaction Support**: Full ACID transaction management with savepoints
- **Connection Pooling**: Efficient connection reuse (planned)
- **Broad Compatibility**: Supports Python 3.8+ and multiple PostgreSQL versions

## Installation

```bash
pip install PostPyro
```

### From Source

Prerequisites:
- Rust 1.70+
- Python 3.8+
- PostgreSQL development headers (for compilation)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/yourusername/pypg-driver.git
cd pypg-driver
pip install -e .
```

## Quick Start

```python
import PostPyro as pg

# Connect to database
conn = pg.connect("postgresql://user:password@localhost:5432/mydb")

# Execute queries
conn.execute("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, age INTEGER)")
conn.execute("INSERT INTO users (name, age) VALUES ($1, $2)", ["Alice", 30])

# Query data
rows = conn.query("SELECT * FROM users WHERE age > $1", [25])
for row in rows:
    print(f"ID: {row['id']}, Name: {row['name']}, Age: {row['age']}")

# Use transactions
with conn.begin() as txn:
    txn.execute("UPDATE users SET age = age + 1 WHERE name = $1", ["Alice"])
    txn.commit()  # or automatic rollback on exception

conn.close()
```

## API Reference

### Connection

#### `pg.connect(connection_string)`

Create a new database connection.

**Parameters:**
- `connection_string` (str): PostgreSQL connection string
  - Format: `postgresql://user:password@host:port/database?options`

**Returns:** `Connection` object

**Example:**
```python
conn = pg.connect("postgresql://user:pass@localhost:5432/mydb")
```

#### `Connection.execute(query, params=None)`

Execute a query that doesn't return rows (INSERT, UPDATE, DELETE).

**Parameters:**
- `query` (str): SQL query string
- `params` (list, optional): Query parameters

**Returns:** Number of rows affected (int)

**Example:**
```python
affected = conn.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
print(f"Inserted {affected} rows")
```

#### `Connection.query(query, params=None)`

Execute a query and return all rows.

**Parameters:**
- `query` (str): SQL query string
- `params` (list, optional): Query parameters

**Returns:** List of `Row` objects

**Example:**
```python
rows = conn.query("SELECT * FROM users WHERE age > $1", [21])
for row in rows:
    print(row['name'], row['age'])
```

#### `Connection.query_one(query, params=None)`

Execute a query and return exactly one row.

**Parameters:**
- `query` (str): SQL query string
- `params` (list, optional): Query parameters

**Returns:** Single `Row` object

**Raises:** `ProgrammingError` if query returns 0 or multiple rows

**Example:**
```python
user = conn.query_one("SELECT * FROM users WHERE id = $1", [1])
print(f"User: {user['name']}")
```

#### `Connection.begin()`

Begin a new transaction.

**Returns:** `Transaction` object

**Example:**
```python
txn = conn.begin()
txn.execute("INSERT INTO logs (message) VALUES ($1)", ["Started process"])
txn.commit()
```

#### `Connection.close()`

Close the database connection.

**Example:**
```python
conn.close()
```

### Row

Represents a single row from a query result.

#### Row Access

```python
row = conn.query_one("SELECT id, name FROM users WHERE id = 1")

# Access by column name
print(row['id'], row['name'])

# Access by column index
print(row[0], row[1])

# Get with default value
age = row.get('age', 0)

# Iterate over values
for value in row:
    print(value)

# Get column names
columns = row.keys()

# Convert to dictionary
user_dict = row.to_dict()
```

### Transaction

Represents a database transaction.

#### `Transaction.execute(query, params=None)`

Execute a query within the transaction.

#### `Transaction.query(query, params=None)`

Query within the transaction.

#### `Transaction.query_one(query, params=None)`

Query one row within the transaction.

#### `Transaction.commit()`

Commit the transaction.

#### `Transaction.rollback()`

Roll back the transaction.

#### `Transaction.savepoint(name)`

Create a savepoint.

**Parameters:**
- `name` (str): Savepoint name

#### `Transaction.rollback_to(name)`

Roll back to a savepoint.

**Parameters:**
- `name` (str): Savepoint name

**Example:**
```python
with conn.begin() as txn:
    txn.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    txn.savepoint("after_insert")

    try:
        txn.execute("INSERT INTO users (name) VALUES ($1)", ["Bob"])
        # Some validation...
    except:
        txn.rollback_to("after_insert")  # Undo the second insert

    txn.commit()  # Commit only Alice
```

## Data Types

pypg-driver supports comprehensive type conversion between Python and PostgreSQL:

| PostgreSQL Type | Python Type | Example |
|----------------|-------------|---------|
| INTEGER/SMALLINT/BIGINT | int | `42` |
| REAL/DOUBLE PRECISION | float | `3.14` |
| TEXT/VARCHAR | str | `"hello"` |
| BYTEA | bytes | `b"data"` |
| BOOLEAN | bool | `True` |
| DATE | datetime.date | `date(2023, 12, 25)` |
| TIME | datetime.time | `time(14, 30, 0)` |
| TIMESTAMP | datetime.datetime | `datetime(2023, 12, 25, 14, 30, 0)` |
| TIMESTAMPTZ | datetime.datetime | `datetime(2023, 12, 25, 14, 30, 0, tzinfo=timezone.utc)` |
| UUID | uuid.UUID | `uuid.uuid4()` |
| JSON/JSONB | dict/list | `{"key": "value"}` |
| Arrays | list | `[1, 2, 3]` |

## Error Handling

pypg-driver raises DB-API 2.0 compliant exceptions:

- `DatabaseError`: Base exception for all database errors
- `InterfaceError`: Client-side errors (connection issues)
- `DataError`: Data processing errors (type conversion)
- `OperationalError`: Database operational errors
- `IntegrityError`: Constraint violations
- `InternalError`: Database internal errors
- `ProgrammingError`: SQL syntax errors, wrong parameters
- `NotSupportedError`: Unsupported operations

**Example:**
```python
try:
    conn.execute("INVALID SQL")
except pg.ProgrammingError as e:
    print(f"SQL Error: {e}")
except pg.InterfaceError as e:
    print(f"Connection Error: {e}")
```

## Transactions

Transactions provide ACID properties for database operations:

```python
# Manual transaction management
txn = conn.begin()
try:
    txn.execute("INSERT INTO accounts (name, balance) VALUES ($1, $2)", ["Alice", 1000])
    txn.execute("INSERT INTO accounts (name, balance) VALUES ($1, $2)", ["Bob", 1000])
    txn.commit()
except Exception:
    txn.rollback()
    raise

# Context manager (auto-rollback on exception)
with conn.begin() as txn:
    txn.execute("UPDATE accounts SET balance = balance - 100 WHERE name = $1", ["Alice"])
    txn.execute("UPDATE accounts SET balance = balance + 100 WHERE name = $1", ["Bob"])
    # Automatic commit on success, rollback on exception
```

## Performance

pypg-driver is designed for high performance:

- **Rust Backend**: Compiled Rust code for maximum speed
- **Zero-Copy**: Efficient data transfer between Python and Rust
- **Async I/O**: Non-blocking database operations
- **Connection Reuse**: Keep connections open for multiple operations
- **Prepared Statements**: Cache query plans for repeated execution

## Development

### Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/yourusername/pypg-driver.git
cd pypg-driver
pip install -e .
```

### Running Tests

```bash
# Install test dependencies
pip install pytest

# Start PostgreSQL test instance (using Docker)
docker run -d --name postgres-test -e POSTGRES_PASSWORD=test -p 5432:5432 postgres:15

# Run tests
pytest tests/
```

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- [tokio-postgres](https://github.com/sfackler/rust-postgres) for the async PostgreSQL driver
- [PyO3](https://github.com/PyO3/pyo3) for Python-Rust bindings