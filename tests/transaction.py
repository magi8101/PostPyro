import asyncio
import gc
import PostPyro


async def main():
    pool = await PostPyro.connect("postgresql://postgres:postgres@localhost:5433/postgres", max_size=5)
    await pool.execute("DROP TABLE IF EXISTS txn_test")
    await pool.execute("CREATE TABLE txn_test (id INT4, balance INT4)")
    await pool.execute("INSERT INTO txn_test (id, balance) VALUES (1, 100)")

    # Explicit commit persists.
    tx = await pool.transaction()
    await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [10, 1])
    await tx.commit()
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 90, row["balance"]

    # Explicit rollback discards.
    tx = await pool.transaction()
    await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [50, 1])
    await tx.rollback()
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 90, row["balance"]

    # Context manager: exception triggers auto-rollback.
    tx = await pool.transaction()
    try:
        async with tx:
            await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [50, 1])
            raise RuntimeError("boom")
    except RuntimeError:
        pass
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 90, row["balance"]

    # Context manager: clean exit auto-commits, and __aenter__ returns the
    # same object it was called on (not a distinct wrapper).
    tx = await pool.transaction()
    async with tx as entered:
        assert entered is tx, "expected __aenter__ to return self"
        await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [40, 1])
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 50, row["balance"]

    # Abandoning a transaction (letting it get garbage-collected without
    # commit/rollback, e.g. because an exception propagated past it without
    # going through `async with`) must not crash the process. sqlx's
    # PoolConnection::drop spawns onto the ambient Tokio runtime, which
    # doesn't exist on the thread Python does GC on - without Transaction's
    # own Drop impl entering the shared runtime first, this panics, and
    # under this crate's panic="abort" release profile that aborts the
    # whole interpreter instead of raising a catchable Python exception.
    tx = await pool.transaction()
    await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [999, 1])
    del tx
    gc.collect()
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 50, row["balance"]  # abandoned tx must roll back, not commit

    # Using a transaction after commit raises ProgrammingError, not a panic/hang.
    tx = await pool.transaction()
    await tx.commit()
    try:
        await tx.execute("SELECT 1")
        raise AssertionError("expected ProgrammingError")
    except PostPyro.ProgrammingError:
        pass

    await pool.execute("DROP TABLE txn_test")
    await pool.close()
    print("OK: transaction commit/rollback/context-manager verified")


asyncio.run(main())
