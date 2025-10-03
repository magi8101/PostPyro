"""
High-performance PostgreSQL driver for Python with DB-API 2.0 compliance.

Features:
- Connection pooling and prepared statement caching
- Full PostgreSQL type support
- Async I/O with tokio-postgres backend
- DB-API 2.0 compliant exception hierarchy
- Performance optimized Rust implementation

Usage:
    import PostPyro
    
    # Create single connection
    conn = PostPyro.Connection("postgresql://user:pass@host/db")
    
    # Execute queries
    conn.execute("INSERT INTO users (name) VALUES ($1)", ["John"])
    rows = conn.query("SELECT * FROM users WHERE active = $1", [True])
    
    # Use connection pool for high-concurrency applications
    pool = PostPyro.ConnectionPool("postgresql://user:pass@host/db", max_size=20)
    rows = pool.query("SELECT * FROM users")
    
    # Use transactions
    txn = conn.begin()
    try:
        txn.execute("INSERT INTO accounts (balance) VALUES ($1)", [100])
        txn.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [10, 1])
        txn.commit()
    except Exception:
        txn.rollback()
        raise
    
    # Or use context manager
    with conn.begin() as txn:
        txn.execute("INSERT INTO logs (message) VALUES ($1)", ["Transaction started"])
        # Auto-rollback on exception, auto-commit on success
    
    conn.close()
"""

from .PostPyro import (
    # Main classes
    Connection, ConnectionPool, Row, Transaction,
    
    # DB-API 2.0 Exceptions
    DatabaseError, InterfaceError, DataError, OperationalError,
    IntegrityError, InternalError, ProgrammingError, NotSupportedError,
    
    # Constants
    __version__, apilevel, threadsafety, paramstyle
)

# Convenience functions
def connect(connection_string):
    """Create a new PostgreSQL connection."""
    return Connection(connection_string)

def create_pool(connection_string, max_size=10, min_size=0):
    """Create a new connection pool."""
    return ConnectionPool(connection_string, max_size, min_size)

__all__ = [
    # Classes
    "Connection", "ConnectionPool", "Row", "Transaction", "connect", "create_pool",
    
    # Exceptions
    "DatabaseError", "InterfaceError", "DataError", "OperationalError",
    "IntegrityError", "InternalError", "ProgrammingError", "NotSupportedError",
    
    # Constants
    "__version__", "apilevel", "threadsafety", "paramstyle"
]