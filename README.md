# PostPyro

**Async PostgreSQL driver for Python, built in Rust.**

[![PyPI version](https://badge.fury.io/py/PostPyro.svg)](https://pypi.org/project/PostPyro/)
[![Python versions](https://img.shields.io/pypi/pyversions/PostPyro)](https://pypi.org/project/PostPyro/)
[![License](https://img.shields.io/pypi/l/PostPyro)](https://github.com/magi8101/PostPyro/blob/main/LICENSE)

PostPyro wraps `sqlx` in a `PyO3`/`pyo3-asyncio` binding: every I/O method is `async def` and releases the GIL while waiting on Postgres, errors come back as a DB-API 2.0-flavored exception hierarchy, and connections can use `sqlx`'s `rustls` backend for TLS.

> **Status: Beta.** PostPyro is mid-rewrite from a synchronous driver to a fully async one. The API described below is the real, current surface (`python/PostPyro/__init__.pyi`) - not a roadmap. Expect breaking changes before a 1.0/stable release.

## Installation

```bash
pip install PostPyro
```

Pre-built wheels - no Rust toolchain or system PostgreSQL libraries needed at install time.

## Quick Start

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

## Documentation

That's the whole surface you need to get moving. For everything else - every method signature, the full type-conversion table, the exception hierarchy, transaction semantics, framework integration patterns, and the gotchas - see [`PostPyro_documentation.md`](PostPyro_documentation.md).

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
