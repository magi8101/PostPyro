"""
High-performance PostgreSQL driver for Python using PyO3 and tokio-postgres.

This package provides a complete PostgreSQL database driver with:
- Full DB-API 2.0 compliance
- High performance Rust backend
- Async I/O support
- Type-safe parameter binding
- Comprehensive error handling
- Connection pooling
- Transaction management

Basic usage:
    import PostPyro as pg

    conn = pg.connect("postgresql://user:pass@localhost/dbname")
    rows = conn.query("SELECT * FROM users")
    conn.close()
"""

from .pypg_driver import (
    # Classes
    Connection,
    Row,
    Transaction,

    # Exceptions
    DatabaseError,
    InterfaceError,
    DataError,
    OperationalError,
    IntegrityError,
    InternalError,
    ProgrammingError,
    NotSupportedError,

    # Functions
    connect,
    get_version,

    # Constants
    __version__,
    apilevel,
    threadsafety,
    paramstyle,
)

__all__ = [
    # Classes
    "Connection",
    "Row",
    "Transaction",

    # Exceptions
    "DatabaseError",
    "InterfaceError",
    "DataError",
    "OperationalError",
    "IntegrityError",
    "InternalError",
    "ProgrammingError",
    "NotSupportedError",

    # Functions
    "connect",
    "get_version",

    # Constants
    "__version__",
    "apilevel",
    "threadsafety",
    "paramstyle",
]