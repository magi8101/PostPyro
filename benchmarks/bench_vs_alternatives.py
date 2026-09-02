#!/usr/bin/env python3
"""
PostPyro vs. asyncpg vs. psycopg (v3, async), same Postgres, same operations.

Every number this prints comes from actually running the operation against a
live Postgres - nothing here is estimated or "illustrative". See
benchmarks/README.md for what these numbers do and don't mean before citing
them anywhere.

asyncpg and psycopg are optional: each is imported in a try/except, and a
driver that isn't installed is skipped with a printed note rather than
crashing the run. See benchmarks/requirements.txt to install both.

Usage:
    python3 benchmarks/bench_vs_alternatives.py [dsn]
"""
import asyncio
import statistics
import sys
import time

import PostPyro

try:
    import asyncpg
except ImportError:
    asyncpg = None

try:
    import psycopg
    from psycopg_pool import AsyncConnectionPool
except ImportError:
    psycopg = None  # type: ignore[assignment]
    AsyncConnectionPool = None  # type: ignore[assignment, misc]

DSN = sys.argv[1] if len(sys.argv) > 1 else "postgresql://postgres:postgres@localhost:5433/postgres"

POOL_SIZE = 10
REPEATS = 5          # repetitions per benchmark - reported as min/median/mean
ROUND_TRIPS = 200    # SELECT 1 round trips per repetition
BULK_ROWS = 1000     # single-row INSERTs per repetition (none of the three
                     # drivers get a batched fast path here - PostPyro's
                     # execute() only takes one parameter set per call, so
                     # this compares the same one-row-at-a-time path in all
                     # three rather than giving asyncpg's executemany an
                     # advantage PostPyro has no equivalent for)
CONCURRENCY = 20     # concurrent tasks for the concurrency benchmark

results: list[tuple[str, str, list[float]]] = []  # (driver, benchmark, [elapsed_seconds, ...])


def record(driver, name, samples):
    results.append((driver, name, samples))


def print_table():
    header = f"{'driver':<10} {'benchmark':<16} {'min ms':>10} {'median ms':>10} {'mean ms':>10}"
    print(header)
    print("-" * len(header))
    for driver, name, samples in results:
        ms = [s * 1000 for s in samples]
        print(
            f"{driver:<10} {name:<16} {min(ms):>10.2f} {statistics.median(ms):>10.2f} "
            f"{statistics.mean(ms):>10.2f}"
        )


# ---------- PostPyro ----------

async def bench_postpyro():
    pool = await PostPyro.connect(DSN, max_size=POOL_SIZE)

    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        for _ in range(ROUND_TRIPS):
            await pool.query("SELECT 1")
        samples.append(time.perf_counter() - t0)
    record("PostPyro", "round_trip", samples)

    await pool.execute("DROP TABLE IF EXISTS bench_bulk")
    await pool.execute("CREATE TABLE bench_bulk (id INT4, val TEXT)")
    samples = []
    for _ in range(REPEATS):
        await pool.execute("TRUNCATE bench_bulk")
        t0 = time.perf_counter()
        for i in range(BULK_ROWS):
            await pool.execute("INSERT INTO bench_bulk (id, val) VALUES ($1, $2)", [i, "row"])
        samples.append(time.perf_counter() - t0)
    record("PostPyro", "bulk_insert", samples)
    await pool.execute("DROP TABLE bench_bulk")

    await pool.execute("DROP TABLE IF EXISTS bench_tx")
    await pool.execute("CREATE TABLE bench_tx (id INT4, val INT4)")
    await pool.execute("INSERT INTO bench_tx (id, val) VALUES (1, 0)")
    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        tx = await pool.transaction()
        async with tx:
            await tx.execute("UPDATE bench_tx SET val = val + 1 WHERE id = $1", [1])
            await tx.query_one("SELECT val FROM bench_tx WHERE id = $1", [1])
            await tx.execute("UPDATE bench_tx SET val = val + 1 WHERE id = $1", [1])
            await tx.query_one("SELECT val FROM bench_tx WHERE id = $1", [1])
        samples.append(time.perf_counter() - t0)
    record("PostPyro", "transaction", samples)
    await pool.execute("DROP TABLE bench_tx")

    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        await asyncio.gather(*(pool.query("SELECT 1") for _ in range(CONCURRENCY)))
        samples.append(time.perf_counter() - t0)
    record("PostPyro", "concurrency", samples)

    await pool.close()


# ---------- asyncpg ----------

async def bench_asyncpg():
    pool = await asyncpg.create_pool(DSN, min_size=1, max_size=POOL_SIZE)

    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        for _ in range(ROUND_TRIPS):
            await pool.fetch("SELECT 1")
        samples.append(time.perf_counter() - t0)
    record("asyncpg", "round_trip", samples)

    await pool.execute("DROP TABLE IF EXISTS bench_bulk")
    await pool.execute("CREATE TABLE bench_bulk (id INT4, val TEXT)")
    samples = []
    for _ in range(REPEATS):
        await pool.execute("TRUNCATE bench_bulk")
        t0 = time.perf_counter()
        for i in range(BULK_ROWS):
            await pool.execute("INSERT INTO bench_bulk (id, val) VALUES ($1, $2)", i, "row")
        samples.append(time.perf_counter() - t0)
    record("asyncpg", "bulk_insert", samples)
    await pool.execute("DROP TABLE bench_bulk")

    await pool.execute("DROP TABLE IF EXISTS bench_tx")
    await pool.execute("CREATE TABLE bench_tx (id INT4, val INT4)")
    await pool.execute("INSERT INTO bench_tx (id, val) VALUES (1, 0)")
    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        async with pool.acquire() as conn:
            async with conn.transaction():
                await conn.execute("UPDATE bench_tx SET val = val + 1 WHERE id = $1", 1)
                await conn.fetchrow("SELECT val FROM bench_tx WHERE id = $1", 1)
                await conn.execute("UPDATE bench_tx SET val = val + 1 WHERE id = $1", 1)
                await conn.fetchrow("SELECT val FROM bench_tx WHERE id = $1", 1)
        samples.append(time.perf_counter() - t0)
    record("asyncpg", "transaction", samples)
    await pool.execute("DROP TABLE bench_tx")

    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        await asyncio.gather(*(pool.fetch("SELECT 1") for _ in range(CONCURRENCY)))
        samples.append(time.perf_counter() - t0)
    record("asyncpg", "concurrency", samples)

    await pool.close()


# ---------- psycopg (v3, async) ----------

async def bench_psycopg():
    pool = AsyncConnectionPool(DSN, min_size=1, max_size=POOL_SIZE, kwargs={"autocommit": True}, open=False)
    await pool.open()
    await pool.wait()

    async def run(query, params=()):
        async with pool.connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(query, params)
                if cur.description is not None:
                    return await cur.fetchall()
                return None

    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        for _ in range(ROUND_TRIPS):
            await run("SELECT 1")
        samples.append(time.perf_counter() - t0)
    record("psycopg", "round_trip", samples)

    await run("DROP TABLE IF EXISTS bench_bulk")
    await run("CREATE TABLE bench_bulk (id INT4, val TEXT)")
    samples = []
    for _ in range(REPEATS):
        await run("TRUNCATE bench_bulk")
        t0 = time.perf_counter()
        for i in range(BULK_ROWS):
            await run("INSERT INTO bench_bulk (id, val) VALUES (%s, %s)", (i, "row"))
        samples.append(time.perf_counter() - t0)
    record("psycopg", "bulk_insert", samples)
    await run("DROP TABLE bench_bulk")

    await run("DROP TABLE IF EXISTS bench_tx")
    await run("CREATE TABLE bench_tx (id INT4, val INT4)")
    await run("INSERT INTO bench_tx (id, val) VALUES (1, 0)")
    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        async with pool.connection() as conn:
            async with conn.transaction():
                async with conn.cursor() as cur:
                    await cur.execute("UPDATE bench_tx SET val = val + 1 WHERE id = %s", (1,))
                    await cur.execute("SELECT val FROM bench_tx WHERE id = %s", (1,))
                    await cur.fetchone()
                    await cur.execute("UPDATE bench_tx SET val = val + 1 WHERE id = %s", (1,))
                    await cur.execute("SELECT val FROM bench_tx WHERE id = %s", (1,))
                    await cur.fetchone()
        samples.append(time.perf_counter() - t0)
    record("psycopg", "transaction", samples)
    await run("DROP TABLE bench_tx")

    samples = []
    for _ in range(REPEATS):
        t0 = time.perf_counter()
        await asyncio.gather(*(run("SELECT 1") for _ in range(CONCURRENCY)))
        samples.append(time.perf_counter() - t0)
    record("psycopg", "concurrency", samples)

    await pool.close()


async def main():
    print(f"Target: {DSN}")
    print(f"REPEATS={REPEATS} ROUND_TRIPS={ROUND_TRIPS} BULK_ROWS={BULK_ROWS} CONCURRENCY={CONCURRENCY}\n")

    print("Running PostPyro...")
    await bench_postpyro()

    if asyncpg is not None:
        print("Running asyncpg...")
        await bench_asyncpg()
    else:
        print("asyncpg not installed - skipping (pip install -r benchmarks/requirements.txt)")

    if psycopg is not None:
        print("Running psycopg...")
        await bench_psycopg()
    else:
        print("psycopg not installed - skipping (pip install -r benchmarks/requirements.txt)")

    print()
    print_table()


if __name__ == "__main__":
    asyncio.run(main())
