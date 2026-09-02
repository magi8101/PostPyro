import asyncio
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

    # Context manager: clean exit auto-commits.
    tx = await pool.transaction()
    async with tx:
        await tx.execute("UPDATE txn_test SET balance = balance - $1 WHERE id = $2", [40, 1])
    row = await pool.query_one("SELECT balance FROM txn_test WHERE id = $1", [1])
    assert row["balance"] == 50, row["balance"]

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
