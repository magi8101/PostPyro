#!/usr/bin/env python3
"""
Correctness under concurrency - not a speed benchmark.

This exercises exactly the class of bug the async rewrite exists to fix: the
old driver held the GIL during I/O, so concurrent asyncio tasks never
actually overlapped and couldn't corrupt each other's in-flight state. This
one runs a genuinely concurrent Pool under load and checks the final state
is exactly right - no lost updates, no leaked writes from an abandoned
transaction, no leaked connections.

Run it for real. If an assertion fails, that's a real bug - report it, don't
loosen the assertion to make the run green.

Usage:
    python3 benchmarks/bench_concurrency_correctness.py [dsn]
"""
import asyncio
import sys
import time

import PostPyro

DSN = sys.argv[1] if len(sys.argv) > 1 else "postgresql://postgres:postgres@localhost:5433/postgres"

# Pool is deliberately smaller than the task counts below: if a connection
# ever leaked instead of being returned to the pool, the pool would run out
# and this would hang (and get caught by the wait_for timeouts) instead of
# silently passing.
POOL_SIZE = 10
CONCURRENT_INCREMENTS = 500
ABANDON_TASKS = 100
NORMAL_TASKS = 100
TIMEOUT_S = 60


async def increment(pool):
    tx = await pool.transaction()
    async with tx:
        await tx.execute("UPDATE counters SET value = value + 1 WHERE id = $1", [1])


async def abandon(pool):
    tx = await pool.transaction()
    # Deliberately never commit or roll back - simulates an exception
    # unwinding past a transaction before it finishes. `Transaction`'s Rust
    # `Drop` impl is what has to roll this back; if it didn't, this write
    # would leak into the row `increment()` above is also updating.
    await tx.execute("UPDATE counters SET value = value + 999999 WHERE id = $1", [1])
    # `del tx` before raising is load-bearing, not defensive style: a raised
    # exception's traceback pins its frame's locals (including `tx`) alive
    # for as long as the exception object itself is reachable. Cleanup here
    # relies on Python's refcount dropping to zero to run the Rust `Drop`
    # (and thus the rollback) - so with `return_exceptions=True` below,
    # skipping this `del` leaves every abandoned transaction's connection
    # sitting "idle in transaction", still holding its row lock, for as
    # long as `asyncio.gather` keeps the exception list alive. Under load
    # that serializes every task behind those held locks and looks exactly
    # like a driver hang. It isn't one - it's this exact interaction,
    # confirmed by isolating a single abandoned transaction (rolls back in
    # single-digit milliseconds) and only reproducing the pileup once
    # 100 abandoned transactions' tracebacks were kept alive at once.
    del tx
    raise RuntimeError("simulated failure before commit")


async def scenario_concurrent_increments(pool):
    print(f"Scenario 1: {CONCURRENT_INCREMENTS} concurrent increments on one row, pool size {POOL_SIZE}")
    await pool.execute("DROP TABLE IF EXISTS counters")
    await pool.execute("CREATE TABLE counters (id INT4 PRIMARY KEY, value INT4)")
    await pool.execute("INSERT INTO counters (id, value) VALUES (1, 0)")

    t0 = time.perf_counter()
    await asyncio.wait_for(
        asyncio.gather(*(increment(pool) for _ in range(CONCURRENT_INCREMENTS))),
        timeout=TIMEOUT_S,
    )
    elapsed = time.perf_counter() - t0

    row = await pool.query_one("SELECT value FROM counters WHERE id = $1", [1])
    final = row["value"]
    assert final == CONCURRENT_INCREMENTS, (
        f"lost updates: expected {CONCURRENT_INCREMENTS}, got {final}"
    )
    print(f"  PASS: final value == {final} ({elapsed:.2f}s, no lost updates)")
    await pool.execute("DROP TABLE counters")


async def scenario_abandoned_transactions(pool):
    print(
        f"Scenario 2: {ABANDON_TASKS} abandoned (never-committed) transactions "
        f"mixed with {NORMAL_TASKS} normal ones, concurrently"
    )
    await pool.execute("DROP TABLE IF EXISTS counters")
    await pool.execute("CREATE TABLE counters (id INT4 PRIMARY KEY, value INT4)")
    await pool.execute("INSERT INTO counters (id, value) VALUES (1, 0)")

    tasks = [abandon(pool) for _ in range(ABANDON_TASKS)] + [increment(pool) for _ in range(NORMAL_TASKS)]

    t0 = time.perf_counter()
    results = await asyncio.wait_for(asyncio.gather(*tasks, return_exceptions=True), timeout=TIMEOUT_S)
    elapsed = time.perf_counter() - t0

    abandon_results = results[:ABANDON_TASKS]
    normal_results = results[ABANDON_TASKS:]
    abandon_failures = [r for r in abandon_results if isinstance(r, RuntimeError)]
    normal_failures = [r for r in normal_results if isinstance(r, BaseException)]
    assert len(abandon_failures) == ABANDON_TASKS, (
        f"expected all {ABANDON_TASKS} abandon tasks to raise RuntimeError, "
        f"got {len(abandon_failures)} (results: {abandon_results})"
    )
    assert not normal_failures, f"normal traffic tasks failed unexpectedly: {normal_failures}"

    row = await pool.query_one("SELECT value FROM counters WHERE id = $1", [1])
    final = row["value"]
    assert final == NORMAL_TASKS, (
        f"abandoned-transaction leakage: expected {NORMAL_TASKS} "
        f"(only the committed increments), got {final}"
    )
    print(f"  PASS: final value == {final} ({elapsed:.2f}s) - none of the abandoned +999999 writes leaked in")
    await pool.execute("DROP TABLE counters")

    # Pool must still be fully usable after all that.
    sanity = await pool.query_one("SELECT 1 AS ok")
    assert sanity["ok"] == 1
    print("  PASS: pool still healthy after abandoned transactions")


async def main():
    print(f"Target: {DSN}\n")
    pool = await PostPyro.connect(DSN, max_size=POOL_SIZE)
    try:
        await scenario_concurrent_increments(pool)
        await scenario_abandoned_transactions(pool)
    finally:
        await pool.close()
    assert pool.is_closed()
    print("PASS: pool closed cleanly")


if __name__ == "__main__":
    asyncio.run(main())
