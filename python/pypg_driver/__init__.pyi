"""
Type stubs for pypg_driver - High-performance PostgreSQL driver for Python.

This file provides type hints for IDE support and static type checking.
"""

from typing import Any, Dict, List, Optional, Union, Iterator, Tuple
import datetime
import uuid

__version__: str
apilevel: str
threadsafety: int
paramstyle: str

class DatabaseError(Exception):
    """Base exception for all database-related errors."""
    pass

class InterfaceError(DatabaseError):
    """Exception for client-side errors (connection, etc.)."""
    pass

class DataError(DatabaseError):
    """Exception for data processing errors."""
    pass

class OperationalError(DatabaseError):
    """Exception for database operational errors."""
    pass

class IntegrityError(DatabaseError):
    """Exception for constraint violations."""
    pass

class InternalError(DatabaseError):
    """Exception for database internal errors."""
    pass

class ProgrammingError(DatabaseError):
    """Exception for SQL syntax errors and wrong parameters."""
    pass

class NotSupportedError(DatabaseError):
    """Exception for unsupported operations."""
    pass

class Row:
    """Represents a single row from a query result."""

    def __len__(self) -> int:
        """Return the number of columns in this row."""
        ...

    def __getitem__(self, key: Union[int, str]) -> Any:
        """Get a column value by index (int) or name (str)."""
        ...

    def __iter__(self) -> Iterator[Any]:
        """Iterate over column values."""
        ...

    def __repr__(self) -> str:
        """String representation for debugging."""
        ...

    def get(self, key: Union[int, str], default: Any = None) -> Any:
        """Get a column value with a default if not found."""
        ...

    def keys(self) -> List[str]:
        """Return a list of column names."""
        ...

    def values(self) -> List[Any]:
        """Return a list of column values."""
        ...

    def items(self) -> List[Tuple[str, Any]]:
        """Return a list of (name, value) tuples."""
        ...

    def to_dict(self) -> Dict[str, Any]:
        """Convert the row to a dictionary."""
        ...

class Transaction:
    """Represents a database transaction."""

    def execute(self, query: str, params: Optional[List[Any]] = None) -> int:
        """Execute a query within the transaction that doesn't return rows."""
        ...

    def query(self, query: str, params: Optional[List[Any]] = None) -> List[Row]:
        """Execute a query within the transaction and return all rows."""
        ...

    def query_one(self, query: str, params: Optional[List[Any]] = None) -> Row:
        """Execute a query within the transaction and return exactly one row."""
        ...

    def commit(self) -> None:
        """Commit the transaction."""
        ...

    def rollback(self) -> None:
        """Roll back the transaction."""
        ...

    def savepoint(self, name: str) -> None:
        """Create a savepoint within the transaction."""
        ...

    def rollback_to(self, name: str) -> None:
        """Roll back to a savepoint."""
        ...

    def __enter__(self) -> 'Transaction':
        """Context manager entry."""
        ...

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Context manager exit - auto-rollback on exception."""
        ...

class Connection:
    """PostgreSQL database connection."""

    def __init__(self, connection_string: str) -> None:
        """Create a new database connection."""
        ...

    def execute(self, query: str, params: Optional[List[Any]] = None) -> int:
        """Execute a query that doesn't return rows (INSERT, UPDATE, DELETE)."""
        ...

    def query(self, query: str, params: Optional[List[Any]] = None) -> List[Row]:
        """Execute a query and return all rows."""
        ...

    def query_one(self, query: str, params: Optional[List[Any]] = None) -> Row:
        """Execute a query and return exactly one row."""
        ...

    def prepare(self, query: str) -> str:
        """Prepare a statement for repeated execution."""
        ...

    def close(self) -> None:
        """Close the database connection."""
        ...

    def is_closed(self) -> bool:
        """Check if the connection is closed."""
        ...

    def begin(self) -> Transaction:
        """Begin a new transaction."""
        ...

    def __enter__(self) -> 'Connection':
        """Context manager entry."""
        ...

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Context manager exit."""
        ...

def connect(connection_string: str) -> Connection:
    """Connect to a PostgreSQL database."""
    ...

def get_version() -> str:
    """Get the driver version."""
    ...