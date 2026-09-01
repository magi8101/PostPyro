"""
High-performance async PostgreSQL driver for Python with DB-API-flavored
exceptions, built on sqlx + pyo3-asyncio.

Usage:
    import asyncio
    import PostPyro

    async def main():
        pool = await PostPyro.connect("postgresql://user:pass@host/db", max_size=20)
        rows = await pool.query("SELECT * FROM users WHERE active = $1", [True])
        await pool.execute("INSERT INTO users (name) VALUES ($1)", ["John"])
        await pool.close()

    asyncio.run(main())
"""

from .PostPyro import (
    # Main classes
    Pool, Row, connect,

    # DB-API 2.0 Exceptions
    DatabaseError, InterfaceError, DataError, OperationalError,
    IntegrityError, InternalError, ProgrammingError, NotSupportedError,

    # Constants
    __version__, apilevel, threadsafety, paramstyle
)

__all__ = [
    # Classes / factory
    "Pool", "Row", "connect",

    # Exceptions
    "DatabaseError", "InterfaceError", "DataError", "OperationalError",
    "IntegrityError", "InternalError", "ProgrammingError", "NotSupportedError",

    # Constants
    "__version__", "apilevel", "threadsafety", "paramstyle"
]