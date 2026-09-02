"""
Async PostgreSQL driver for Python, built on sqlx and pyo3-asyncio.

Features:
- Fully async: every I/O-bound call is `await`-able and releases the GIL
  while waiting on Postgres (no blocking, no held GIL during a query)
- Connection pooling (sqlx::PgPool) - construct via `connect()`
- Row access by index or column name, `keys()`/`values()`/`items()`/`to_dict()`
- DB-API 2.0 compliant exception hierarchy
- TLS-capable connections (rustls)

Usage:
    import asyncio
    import PostPyro

    async def main():
        pool = await PostPyro.connect("postgresql://user:pass@host/db", max_size=20)

        await pool.execute("INSERT INTO users (name) VALUES ($1)", ["John"])
        rows = await pool.query("SELECT * FROM users WHERE active = $1", [True])
        for row in rows:
            print(row["name"], row.to_dict())

        tx = await pool.transaction()
        async with tx:
            await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", [10, 1])
            # auto-commit on success, auto-rollback on exception

        await pool.close()

    asyncio.run(main())
"""

from .PostPyro import (
    # Main classes and factory
    Pool, Row, Transaction, connect,

    # DB-API 2.0 Exceptions
    DatabaseError, InterfaceError, DataError, OperationalError,
    IntegrityError, InternalError, ProgrammingError, NotSupportedError,

    # Constants
    __version__, apilevel, threadsafety, paramstyle,
)

__all__ = [
    "Pool", "Row", "Transaction", "connect",
    "DatabaseError", "InterfaceError", "DataError", "OperationalError",
    "IntegrityError", "InternalError", "ProgrammingError", "NotSupportedError",
    "__version__", "apilevel", "threadsafety", "paramstyle",
]
