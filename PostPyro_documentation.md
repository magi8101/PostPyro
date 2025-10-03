# PostPyro Documentation

**High-Performance PostgreSQL Driver for Python Built with Rust**

PostPyro is a modern, blazingly fast PostgreSQL driver for Python that combines the safety and performance of Rust with the simplicity of Python. Built with PyO3 and tokio-postgres, it provides DB-API 2.0 compliance while delivering superior performance through native Rust implementation.

## 🚀 Key Features

- **🔥 High Performance**: Rust-powered backend with zero-copy data handling
- **🛡️ Memory Safe**: Rust's ownership system prevents memory leaks and segfaults
- **🌐 Full PostgreSQL Support**: All data types, arrays, JSON, UUIDs, network types
- **⚡ Tokio Async I/O**: Native async I/O under the hood for excellent performance
- **🔒 Type Safety**: Comprehensive type checking and conversion
- **🎯 DB-API 2.0 Compliant**: Standard Python database interface
- **🌊 Simple API**: Clean, intuitive interface that's easier than alternatives
- **📦 Zero Dependencies**: Self-contained with no external Python dependencies

## 📦 Installation

```bash
pip install PostPyro
```

## 🚀 Quick Start

```python
import PostPyro as pg

# Connect to PostgreSQL
conn = pg.Connection("postgresql://user:password@localhost:5432/database")

# Simple query
users = conn.query("SELECT id, name, email FROM users WHERE active = $1", [True])
for user in users:
    print(f"User: {user['name']} ({user['email']})")

# Insert data
user_id = conn.execute(
    "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id",
    ["John Doe", "john@example.com"]
)

# Close connection
conn.close()
```

## 📚 API Reference

### Module Constants

```python
PostPyro.__version__      # Driver version (e.g., "0.1.2")
PostPyro.apilevel         # "2.0" (DB-API 2.0 compliant)
PostPyro.threadsafety     # 2 (Thread-safe connections)
PostPyro.paramstyle       # "numeric" (PostgreSQL $1, $2 style)
```

### Functions

#### `connect(connection_string: str) -> Connection`

Create a new database connection.

```python
conn = PostPyro.connect("postgresql://user:pass@host:port/database")

# Connection string formats supported:
# - postgresql://user:password@host:port/database
# - postgres://user:password@host:port/database
# - With SSL: postgresql://user:pass@host/db?sslmode=require
```

#### `get_version() -> str`

Get the driver version.

```python
version = PostPyro.get_version()  # Returns "0.1.2"
```

## 🔌 Connection Class

### Constructor

#### `Connection(connection_string: str)`

Create a new connection directly.

```python
conn = PostPyro.Connection("postgresql://user:pass@localhost/db")
```

### Methods

#### `query(sql: str, params: List = None) -> List[Row]`

Execute a SELECT query and return all rows.

```python
# Simple query
rows = conn.query("SELECT * FROM users")

# Parameterized query
rows = conn.query("SELECT * FROM users WHERE age > $1 AND city = $2", [25, "New York"])

# Process results
for row in rows:
    print(f"ID: {row['id']}, Name: {row['name']}")
```

#### `query_one(sql: str, params: List = None) -> Row`

Execute a query and return exactly one row. Raises an error if zero or multiple rows returned.

```python
user = conn.query_one("SELECT * FROM users WHERE id = $1", [123])
print(f"User: {user['name']}")
```

#### `execute(sql: str, params: List = None) -> int`

Execute INSERT, UPDATE, DELETE, or DDL statements. Returns the number of affected rows.

```python
# Insert
affected = conn.execute(
    "INSERT INTO users (name, email) VALUES ($1, $2)",
    ["Alice", "alice@example.com"]
)

# Update
affected = conn.execute(
    "UPDATE users SET email = $1 WHERE id = $2",
    ["newemail@example.com", 123]
)

# Delete
affected = conn.execute("DELETE FROM users WHERE active = $1", [False])

# DDL
conn.execute("CREATE TABLE products (id SERIAL PRIMARY KEY, name TEXT)")
```

#### `execute_batch(queries: List[str]) -> List[int]`

Execute multiple SQL statements in a batch for improved performance.

```python
queries = [
    "INSERT INTO users (name) VALUES ('User 1')",
    "INSERT INTO users (name) VALUES ('User 2')",
    "INSERT INTO users (name) VALUES ('User 3')"
]

results = conn.execute_batch(queries)
print(f"Inserted {sum(results)} total rows")
```

#### `prepare(sql: str) -> str`

Prepare a SQL statement for repeated execution. Returns a statement identifier.

```python
stmt_id = conn.prepare("SELECT * FROM users WHERE department = $1")
# Use with regular query/execute methods
```

#### `ping() -> bool`

Test if the connection is alive and responsive.

```python
if conn.ping():
    print("Connection is healthy")
else:
    print("Connection is dead")
```

#### `info() -> Dict[str, Any]`

Get detailed connection information and status.

```python
info = conn.info()
print(f"Connection closed: {info['closed']}")
print(f"Connection healthy: {info['healthy']}")
```

#### `begin() -> Transaction`

Begin a new transaction and return a Transaction object.

```python
with conn.begin() as tx:
    tx.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    tx.execute("INSERT INTO orders (user_id) VALUES ($1)", [123])
    # Automatically commits on successful exit
```

#### `close() -> None`

Close the database connection.

```python
conn.close()
```

#### `is_closed() -> bool`

Check if the connection is closed.

```python
if not conn.is_closed():
    conn.query("SELECT 1")
```

## 📄 Row Class

Represents a single row from a query result with dict-like interface.

### Methods

#### `__getitem__(key: Union[int, str]) -> Any`

Access column values by index or name.

```python
row = conn.query_one("SELECT id, name, email FROM users WHERE id = $1", [1])

# Access by column name
print(row['name'])
print(row['email'])

# Access by index
print(row[0])  # id
print(row[1])  # name
```

#### `__len__() -> int`

Get the number of columns.

```python
column_count = len(row)
```

#### `__iter__()`

Iterate over column values.

```python
for value in row:
    print(value)
```

#### `get(key: Union[int, str], default: Any = None) -> Any`

Get a column value with a default if not found.

```python
age = row.get('age', 0)
```

#### `keys() -> List[str]`

Get all column names.

```python
columns = row.keys()
print(f"Columns: {columns}")
```

#### `values() -> List[Any]`

Get all column values.

```python
values = row.values()
```

#### `items() -> List[Tuple[str, Any]]`

Get (column, value) pairs.

```python
for column, value in row.items():
    print(f"{column}: {value}")
```

#### `to_dict() -> Dict[str, Any]`

Convert row to a Python dictionary.

```python
user_dict = row.to_dict()
```

## 🔄 Transaction Class

Represents a database transaction with automatic rollback on errors.

### Methods

#### `execute(sql: str, params: List = None) -> int`

Execute a statement within the transaction.

```python
with conn.begin() as tx:
    tx.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])
    tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [100, 1])
```

#### `query(sql: str, params: List = None) -> List[Row]`

Execute a query within the transaction.

```python
with conn.begin() as tx:
    users = tx.query("SELECT * FROM users WHERE created_today = true")
    for user in users:
        tx.execute("UPDATE users SET welcomed = true WHERE id = $1", [user['id']])
```

#### `query_one(sql: str, params: List = None) -> Row`

Execute a query returning one row within the transaction.

## ⚠️ Error Handling

PostPyro provides comprehensive PostgreSQL error mapping with specific exception types.

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

### Error Handling Examples

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
    print(f"SQL error: {e}")

except pg.DatabaseError as e:
    print(f"General database error: {e}")
```

## 🎯 Type System

PostPyro automatically converts between Python and PostgreSQL types.

### Supported Type Conversions

| PostgreSQL Type            | Python Type         | Example                                        |
| -------------------------- | ------------------- | ---------------------------------------------- |
| `BOOLEAN`                  | `bool`              | `True`, `False`                                |
| `SMALLINT`, `INTEGER`      | `int`               | `42`, `-123`                                   |
| `BIGINT`                   | `int`               | `9223372036854775807`                          |
| `REAL`, `DOUBLE PRECISION` | `float`             | `3.14`, `2.718`                                |
| `TEXT`, `VARCHAR`          | `str`               | `"Hello World"`                                |
| `BYTEA`                    | `bytes`             | `b"binary data"`                               |
| `DATE`                     | `datetime.date`     | `date(2023, 12, 25)`                           |
| `TIME`                     | `datetime.time`     | `time(14, 30, 0)`                              |
| `TIMESTAMP`                | `datetime.datetime` | `datetime(2023, 12, 25, 14, 30)`               |
| `TIMESTAMPTZ`              | `datetime.datetime` | With timezone info                             |
| `UUID`                     | `uuid.UUID`         | `UUID('550e8400-e29b-41d4-a716-446655440000')` |
| `JSON`, `JSONB`            | `dict`, `list`      | `{"key": "value"}`, `[1, 2, 3]`                |
| `ARRAY`                    | `list`              | `[1, 2, 3]`, `["a", "b", "c"]`                 |
| `INET`, `CIDR`             | `str`               | `"192.168.1.1"`, `"192.168.0.0/24"`            |

### Type Usage Examples

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

## 🌐 Integration with Other Libraries

### Pandas Integration

```python
import pandas as pd
import PostPyro as pg

conn = pg.Connection("postgresql://user:pass@localhost/db")

# Query to DataFrame
rows = conn.query("SELECT * FROM sales_data")
df = pd.DataFrame([row.to_dict() for row in rows])

# Or using list comprehension
df = pd.DataFrame([dict(row.items()) for row in rows])

print(df.head())
```

### SQLAlchemy Style Usage

```python
import PostPyro as pg

class DatabaseManager:
    def __init__(self, connection_string):
        self.conn = pg.Connection(connection_string)

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.conn.close()

    def fetch_users(self, active_only=True):
        return self.conn.query(
            "SELECT * FROM users WHERE active = $1",
            [active_only]
        )

# Usage
with DatabaseManager("postgresql://user:pass@localhost/db") as db:
    users = db.fetch_users()
    for user in users:
        print(user['name'])
```

### FastAPI Integration

```python
from fastapi import FastAPI, HTTPException
import PostPyro as pg

app = FastAPI()

# Database connection
conn = pg.Connection("postgresql://user:pass@localhost/db")

@app.get("/users/{user_id}")
async def get_user(user_id: int):
    try:
        user = conn.query_one(
            "SELECT id, name, email FROM users WHERE id = $1",
            [user_id]
        )
        return user.to_dict()
    except pg.DatabaseError:
        raise HTTPException(status_code=404, detail="User not found")

@app.post("/users")
async def create_user(name: str, email: str):
    try:
        result = conn.execute(
            "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id",
            [name, email]
        )
        return {"id": result, "message": "User created"}
    except pg.IntegrityError:
        raise HTTPException(status_code=400, detail="Email already exists")
```

### Django-Style Models

```python
import PostPyro as pg

class Model:
    def __init__(self, connection_string):
        self.conn = pg.Connection(connection_string)

    def save(self):
        # Implement save logic
        pass

    @classmethod
    def find(cls, **kwargs):
        # Implement find logic
        pass

class User(Model):
    def __init__(self, connection_string, name=None, email=None):
        super().__init__(connection_string)
        self.name = name
        self.email = email

    def save(self):
        return self.conn.execute(
            "INSERT INTO users (name, email) VALUES ($1, $2)",
            [self.name, self.email]
        )

    @classmethod
    def find_by_email(cls, conn, email):
        row = conn.query_one(
            "SELECT * FROM users WHERE email = $1",
            [email]
        )
        return cls(None, row['name'], row['email'])
```

## ⚡ Performance Advantages

### Why PostPyro is Faster

1. **🦀 Rust Backend**: Native performance without Python interpreter overhead
2. **⚡ Zero-Copy**: Direct memory mapping between PostgreSQL and Python
3. **🌊 Async I/O**: Tokio-powered async networking under the hood
4. **🎯 Optimized Parsing**: Fast binary protocol parsing in Rust
5. **📦 No Dependencies**: No external Python dependencies to slow things down

### Comparison with Other Drivers

| Feature        | PostPyro      | psycopg2       | asyncpg       | psycopg3       |
| -------------- | ------------- | -------------- | ------------- | -------------- |
| Language       | Rust + Python | C + Python     | Cython        | C + Python     |
| Performance    | ⭐⭐⭐⭐⭐    | ⭐⭐⭐         | ⭐⭐⭐⭐      | ⭐⭐⭐         |
| Memory Safety  | ✅ Rust       | ❌ Manual C    | ⚠️ Cython     | ❌ Manual C    |
| Installation   | 📦 Wheel      | 🔧 Compilation | 📦 Wheel      | 🔧 Compilation |
| Dependencies   | 🎯 Zero       | 📦 Many        | 📦 Few        | 📦 Many        |
| API Simplicity | ✅ Simple     | ⚠️ Complex     | ⚠️ Async Only | ⚠️ Complex     |

### Performance Example

```python
import time
import PostPyro as pg

conn = pg.Connection("postgresql://user:pass@localhost/db")

# Benchmark: Insert 10,000 records
start_time = time.time()

queries = []
for i in range(10000):
    queries.append(f"INSERT INTO benchmark (value) VALUES ({i})")

conn.execute_batch(queries)

elapsed = time.time() - start_time
print(f"Inserted 10,000 records in {elapsed:.2f} seconds")
print(f"Rate: {10000/elapsed:.0f} inserts/second")
```

## 🛠️ Advanced Usage

### Connection Pooling Pattern

```python
import PostPyro as pg
from threading import Lock
from queue import Queue, Empty

class ConnectionPool:
    def __init__(self, connection_string, pool_size=5):
        self.connection_string = connection_string
        self.pool = Queue(maxsize=pool_size)
        self.lock = Lock()

        # Initialize pool
        for _ in range(pool_size):
            conn = pg.Connection(connection_string)
            self.pool.put(conn)

    def get_connection(self):
        try:
            return self.pool.get(timeout=10)
        except Empty:
            raise RuntimeError("No connections available")

    def return_connection(self, conn):
        if not conn.is_closed():
            self.pool.put(conn)

    def close_all(self):
        while not self.pool.empty():
            try:
                conn = self.pool.get_nowait()
                conn.close()
            except Empty:
                break

# Usage
pool = ConnectionPool("postgresql://user:pass@localhost/db")

def process_user(user_id):
    conn = pool.get_connection()
    try:
        user = conn.query_one("SELECT * FROM users WHERE id = $1", [user_id])
        # Process user...
        return user
    finally:
        pool.return_connection(conn)
```

### Batch Processing Pattern

```python
def bulk_insert_users(conn, users_data):
    """Efficiently insert many users using batch operations"""

    # Method 1: Single transaction with multiple inserts
    with conn.begin() as tx:
        for user_data in users_data:
            tx.execute(
                "INSERT INTO users (name, email, age) VALUES ($1, $2, $3)",
                [user_data['name'], user_data['email'], user_data['age']]
            )

    # Method 2: Batch execution (faster for large datasets)
    queries = []
    for user_data in users_data:
        queries.append(
            f"INSERT INTO users (name, email, age) VALUES "
            f"('{user_data['name']}', '{user_data['email']}', {user_data['age']})"
        )

    conn.execute_batch(queries)
```

### Error Recovery Pattern

```python
import time
import PostPyro as pg

def robust_query(connection_string, sql, params=None, max_retries=3):
    """Execute query with automatic retry on connection errors"""

    for attempt in range(max_retries):
        try:
            conn = pg.Connection(connection_string)

            if not conn.ping():
                raise pg.OperationalError("Connection failed ping test")

            result = conn.query(sql, params)
            conn.close()
            return result

        except (pg.OperationalError, pg.InterfaceError) as e:
            if attempt == max_retries - 1:
                raise

            print(f"Connection error (attempt {attempt + 1}): {e}")
            time.sleep(2 ** attempt)  # Exponential backoff
            continue

        except pg.DatabaseError:
            # Don't retry on SQL errors
            raise

# Usage
try:
    users = robust_query(
        "postgresql://user:pass@localhost/db",
        "SELECT * FROM users WHERE active = $1",
        [True]
    )
except pg.DatabaseError as e:
    print(f"Database error: {e}")
```

## 🌟 Best Practices

### 1. Connection Management

```python
# ✅ Good: Use context managers or explicit close
with pg.Connection("postgresql://...") as conn:
    result = conn.query("SELECT * FROM users")

# ✅ Good: Explicit cleanup
conn = pg.Connection("postgresql://...")
try:
    result = conn.query("SELECT * FROM users")
finally:
    conn.close()

# ❌ Bad: No cleanup
conn = pg.Connection("postgresql://...")
result = conn.query("SELECT * FROM users")  # Connection leaks
```

### 2. Parameter Binding

```python
# ✅ Good: Always use parameters
user_id = 123
conn.query("SELECT * FROM users WHERE id = $1", [user_id])

# ❌ Bad: String formatting (SQL injection risk)
conn.query(f"SELECT * FROM users WHERE id = {user_id}")
```

### 3. Transaction Usage

```python
# ✅ Good: Use transactions for multiple operations
with conn.begin() as tx:
    tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [100, 1])
    tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", [100, 2])

# ❌ Bad: Multiple separate operations
conn.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [100, 1])
conn.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", [100, 2])
```

### 4. Error Handling

```python
# ✅ Good: Specific error handling
try:
    conn.execute("INSERT INTO users (email) VALUES ($1)", [email])
except pg.IntegrityError:
    print("Email already exists")
except pg.ProgrammingError:
    print("SQL syntax error")

# ❌ Bad: Generic error handling
try:
    conn.execute("INSERT INTO users (email) VALUES ($1)", [email])
except Exception as e:
    print(f"Something went wrong: {e}")
```

## 📊 Performance Tips

1. **Use batch operations** for multiple inserts/updates
2. **Prepare statements** for repeated queries
3. **Use transactions** to group related operations
4. **Close connections** explicitly to free resources
5. **Use connection pooling** in multi-threaded applications
6. **Leverage Row.to_dict()** for pandas integration
7. **Use query_one()** when expecting single results

## 🔧 Configuration

### Connection String Options

```python
# Basic connection
conn = pg.Connection("postgresql://user:pass@localhost:5432/database")

# With SSL
conn = pg.Connection("postgresql://user:pass@host/db?sslmode=require")

# With connection timeout
conn = pg.Connection("postgresql://user:pass@host/db?connect_timeout=10")

# Multiple parameters
conn = pg.Connection("postgresql://user:pass@host/db?sslmode=require&connect_timeout=10&application_name=myapp")
```

## 🆚 Migration from Other Drivers

### From psycopg2

```python
# psycopg2
import psycopg2
conn = psycopg2.connect("host=localhost dbname=test user=postgres")
cur = conn.cursor()
cur.execute("SELECT * FROM users WHERE id = %s", (123,))
rows = cur.fetchall()
conn.close()

# PostPyro
import PostPyro as pg
conn = pg.Connection("postgresql://postgres@localhost/test")
rows = conn.query("SELECT * FROM users WHERE id = $1", [123])
conn.close()
```

### From asyncpg

```python
# asyncpg (async)
import asyncpg
conn = await asyncpg.connect("postgresql://postgres@localhost/test")
rows = await conn.fetch("SELECT * FROM users WHERE id = $1", 123)
await conn.close()

# PostPyro (sync, but faster due to Rust)
import PostPyro as pg
conn = pg.Connection("postgresql://postgres@localhost/test")
rows = conn.query("SELECT * FROM users WHERE id = $1", [123])
conn.close()
```

## 🐛 Troubleshooting

### Common Issues

**Connection refused**

```python
# Check if PostgreSQL is running
if not conn.ping():
    print("PostgreSQL server is not responding")
```

**Authentication failed**

```python
try:
    conn = pg.Connection("postgresql://user:wrongpass@localhost/db")
except pg.OperationalError as e:
    print(f"Authentication error: {e}")
```

**Type conversion errors**

```python
# Use proper Python types
conn.execute("INSERT INTO users (age) VALUES ($1)", [25])  # ✅ int
conn.execute("INSERT INTO users (age) VALUES ($1)", ["25"])  # ❌ string
```

## 🎯 Conclusion

PostPyro provides the perfect balance of **performance**, **safety**, and **simplicity**:

- **🚀 Faster than psycopg2** thanks to Rust implementation
- **🛡️ Safer than C-based drivers** with Rust's memory safety
- **🎯 Simpler than asyncpg** with synchronous interface
- **📦 Easier to install** with pre-built wheels
- **🌊 More efficient** with zero-copy operations

**Start using PostPyro today for your PostgreSQL projects!**

```bash
pip install PostPyro
```

---

**Built with ❤️ using Rust, PyO3, and tokio-postgres**

For more examples and advanced usage, visit our [GitHub repository](https://github.com/magi8101/PostPyro).
