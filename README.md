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
git clone https://github.com/magi8101/pypg-driver.git
cd pypg-driver
pip install -e .
```

## Quick Start

```python
import PostPyro as pg

# Connect to database (two ways)
conn = pg.connect("postgresql://user:password@localhost:5432/mydb")
# OR
conn = pg.Connection("postgresql://user:password@localhost:5432/mydb")

# Execute DDL and DML queries
conn.execute("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, age INTEGER)")
affected = conn.execute("INSERT INTO users (name, age) VALUES ($1, $2)", ["Alice", 30])
print(f"Inserted {affected} rows")

# Query data (multiple rows)
rows = conn.query("SELECT * FROM users WHERE age > $1", [25])
for row in rows:
    print(f"ID: {row['id']}, Name: {row['name']}, Age: {row['age']}")

# Query single row
user = conn.query_one("SELECT * FROM users WHERE id = $1", [1])
print(f"Found user: {user['name']}")

# Batch execute multiple queries
queries = [
    "INSERT INTO users (name, age) VALUES ('Bob', 25)",
    "INSERT INTO users (name, age) VALUES ('Charlie', 35)",
    "INSERT INTO users (name, age) VALUES ('Diana', 28)"
]
results = conn.execute_batch(queries)
print(f"Batch inserted {sum(results)} rows")

# Check connection health
if conn.ping():
    print("✅ Connection is healthy")

# Get connection info
info = conn.info()
print(f"Connection status: {info}")

# Use transactions
with conn.begin() as txn:
    txn.execute("UPDATE users SET age = age + 1 WHERE name = $1", ["Alice"])
    # Automatically commits on successful exit or rolls back on exception

# Prepared statements for repeated queries
stmt_id = conn.prepare("SELECT * FROM users WHERE age > $1")
young_users = conn.query("SELECT * FROM users WHERE age > $1", [20])

# Always close when done
conn.close()
# OR check if closed
if not conn.is_closed():
    conn.close()
```

## API Reference

### Module Constants

```python
pg.apilevel        # "2.0" - DB-API 2.0 compliant
pg.threadsafety    # 2 - Thread-safe connections
pg.paramstyle      # "numeric" - Uses $1, $2, ... parameters
```

### Module Functions

#### `pg.connect(connection_string)`

Create a new database connection using the connect function.

**Parameters:**
- `connection_string` (str): PostgreSQL connection string
  - Format: `postgresql://user:password@host:port/database?options`

**Returns:** `Connection` object

**Example:**
```python
conn = pg.connect("postgresql://user:pass@localhost:5432/mydb")
```

#### `pg.Connection(connection_string)`

Create a new database connection using the Connection class.

**Parameters:**
- `connection_string` (str): PostgreSQL connection string

**Returns:** `Connection` object

**Example:**
```python
conn = pg.Connection("postgresql://user:pass@localhost:5432/mydb")
```

#### `pg.get_version()`

Get the PostPyro driver version.

**Returns:** Version string (e.g., "0.1.2")

**Example:**
```python
version = pg.get_version()
print(f"PostPyro version: {version}")
```

### Connection Class Methods

#### `conn.execute(query, params=None)`

Execute INSERT, UPDATE, DELETE, or DDL statements.

**Parameters:**
- `query` (str): SQL query string
- `params` (list, optional): Query parameters using $1, $2, ... placeholders

**Returns:** Number of rows affected (int)

**Examples:**
```python
# INSERT
affected = conn.execute("INSERT INTO users (name, age) VALUES ($1, $2)", ["Alice", 30])

# UPDATE
affected = conn.execute("UPDATE users SET age = $1 WHERE name = $2", [31, "Alice"])

# DELETE
affected = conn.execute("DELETE FROM users WHERE age < $1", [18])

# DDL
conn.execute("CREATE TABLE products (id SERIAL PRIMARY KEY, name TEXT)")
```

#### `conn.query(query, params=None)`

Execute SELECT queries and return all matching rows.

**Parameters:**
- `query` (str): SQL SELECT statement
- `params` (list, optional): Query parameters

**Returns:** List of `Row` objects

**Example:**
```python
rows = conn.query("SELECT id, name, age FROM users WHERE age > $1", [25])
for row in rows:
    print(f"ID: {row['id']}, Name: {row['name']}, Age: {row['age']}")
```

#### `conn.query_one(query, params=None)`

Execute SELECT query and return exactly one row.

**Parameters:**
- `query` (str): SQL SELECT statement  
- `params` (list, optional): Query parameters

**Returns:** Single `Row` object

**Raises:** Error if zero or multiple rows returned

**Example:**
```python
user = conn.query_one("SELECT * FROM users WHERE id = $1", [1])
print(f"User name: {user['name']}")
```

#### `conn.execute_batch(queries)`

Execute multiple SQL statements in a batch for improved performance.

**Parameters:**
- `queries` (list): List of SQL query strings

**Returns:** List of integers (rows affected for each query)

**Example:**
```python
queries = [
    "INSERT INTO users (name) VALUES ('Bob')",
    "INSERT INTO users (name) VALUES ('Charlie')",
    "INSERT INTO users (name) VALUES ('Diana')"
]
results = conn.execute_batch(queries)
print(f"Total rows inserted: {sum(results)}")
```

#### `conn.prepare(query)`

Prepare a SQL statement for repeated execution.

**Parameters:**
- `query` (str): SQL statement to prepare

**Returns:** Statement identifier string

**Example:**
```python
stmt_id = conn.prepare("SELECT * FROM users WHERE department = $1")
# Use with regular query methods
```

#### `conn.begin()`

Begin a new transaction and return a Transaction object.

**Returns:** `Transaction` object (context manager)

**Example:**
```python
with conn.begin() as txn:
    txn.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    txn.execute("UPDATE accounts SET balance = balance - 100 WHERE user_id = $1", [1])
    # Automatically commits on success, rolls back on exception
```

#### `conn.ping()`

Test if the connection is alive and responsive.

**Returns:** `True` if healthy, `False` if connection issues

**Example:**
```python
if conn.ping():
    print("✅ Connection is healthy")
else:
    print("❌ Connection has issues")
```

#### `conn.info()`

Get detailed connection information and status.

**Returns:** Dictionary with connection details

**Example:**
```python
info = conn.info()
print(f"Closed: {info['closed']}, Healthy: {info['healthy']}")
```

#### `conn.close()`

Close the database connection and free resources.

**Example:**
```python
conn.close()
```

#### `conn.is_closed()`

Check if the connection has been closed.

**Returns:** `True` if closed, `False` if still open

**Example:**
```python
if not conn.is_closed():
    conn.query("SELECT 1")  # Safe to use
```

### Row Class

Represents a single row from a query result with dict-like interface.

#### Row Methods

**Dict-like Access:**
```python
row = conn.query_one("SELECT id, name, email FROM users WHERE id = $1", [1])

# Access by column name
print(row['name'])
print(row['email'])

# Access by index  
print(row[0])  # id
print(row[1])  # name

# Get with default
age = row.get('age', 0)

# Check length
print(f"Row has {len(row)} columns")

# Iterate over values
for value in row:
    print(value)

# Get column names
columns = row.keys()
print(f"Columns: {list(columns)}")

# Get all values
values = row.values()
print(f"Values: {list(values)}")

# Get (column, value) pairs
for column, value in row.items():
    print(f"{column}: {value}")

# Convert to dictionary
user_dict = row.to_dict()
```

### Transaction Class

Represents a database transaction with automatic rollback on errors.

#### Transaction Methods

```python
# Automatic transaction management
with conn.begin() as txn:
    # Execute statements within transaction
    txn.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    txn.execute("UPDATE accounts SET balance = balance - 100 WHERE id = $1", [1])
    
    # Query within transaction
    users = txn.query("SELECT * FROM users WHERE created_today = true")
    for user in users:
        txn.execute("UPDATE users SET welcomed = true WHERE id = $1", [user['id']])
    
    # Query single row within transaction  
    account = txn.query_one("SELECT balance FROM accounts WHERE id = $1", [1])
    
    # Transaction commits automatically on successful exit
    # OR rolls back automatically if exception occurs
```

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

#### Error Handling Examples

```python
import PostPyro as pg

try:
    conn = pg.Connection("postgresql://user:pass@localhost/db")
    conn.execute("INSERT INTO users (email) VALUES ($1)", ["invalid-email"])
    
except pg.IntegrityError as e:
    print(f"Constraint violation: {e}")
    
except pg.OperationalError as e:
    print(f"Database operation failed: {e}")
    
except pg.ProgrammingError as e:
    print(f"SQL syntax error: {e}")
    
except pg.DatabaseError as e:
    print(f"General database error: {e}")
```

### Type System

PostPyro automatically converts between Python and PostgreSQL types.

#### Supported Type Conversions

| PostgreSQL Type | Python Type | Example |
|----------------|-------------|---------|
| `BOOLEAN` | `bool` | `True`, `False` |
| `SMALLINT`, `INTEGER` | `int` | `42`, `-123` |
| `BIGINT` | `int` | `9223372036854775807` |
| `REAL`, `DOUBLE PRECISION` | `float` | `3.14`, `2.718` |
| `TEXT`, `VARCHAR` | `str` | `"Hello World"` |
| `BYTEA` | `bytes` | `b"binary data"` |
| `DATE` | `datetime.date` | `date(2023, 12, 25)` |
| `TIME` | `datetime.time` | `time(14, 30, 0)` |
| `TIMESTAMP` | `datetime.datetime` | `datetime(2023, 12, 25, 14, 30)` |
| `TIMESTAMPTZ` | `datetime.datetime` | With timezone info |
| `UUID` | `uuid.UUID` | `UUID('550e8400-e29b-...')` |
| `JSON`, `JSONB` | `dict`, `list` | `{"key": "value"}`, `[1, 2, 3]` |
| `ARRAY` | `list` | `[1, 2, 3]`, `["a", "b", "c"]` |
| `INET`, `CIDR` | `str` | `"192.168.1.1"`, `"192.168.0.0/24"` |

#### Type Usage Examples

```python
from datetime import datetime, date
import uuid

# Insert various types
conn.execute("""
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
row = conn.query_one("SELECT * FROM mixed_types WHERE id = $1", [1])
assert isinstance(row['bool_col'], bool)
assert isinstance(row['json_col'], dict)
```

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
git clone https://github.com/magi8101/pypg-driver.git
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
