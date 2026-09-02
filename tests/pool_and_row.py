import asyncio
import PostPyro


async def main():
    pool = await PostPyro.connect("postgresql://postgres:postgres@localhost:5433/postgres", max_size=5)

    await pool.execute("DROP TABLE IF EXISTS pool_and_row_test")
    await pool.execute(
        "CREATE TABLE pool_and_row_test (id INT4, name TEXT, score FLOAT8, active BOOL)"
    )

    affected = await pool.execute(
        "INSERT INTO pool_and_row_test (id, name, score, active) VALUES ($1, $2, $3, $4)",
        [1, "Ada", 3.14159265358979, True],
    )
    assert affected == 1, f"expected 1 row affected, got {affected}"

    rows = await pool.query("SELECT * FROM pool_and_row_test")
    assert len(rows) == 1
    row = rows[0]

    # Column-name access must return the RIGHT column - this was the bug
    # in the old driver (always returned column 0 regardless of name).
    assert row["name"] == "Ada", f"expected 'Ada', got {row['name']!r}"
    assert row["id"] == 1
    assert row["active"] is True

    # Float precision must round-trip exactly through FLOAT8 - the old
    # driver forced every float to f32 and lost precision here.
    assert row["score"] == 3.14159265358979, f"precision lost: {row['score']!r}"

    assert row.keys() == ["id", "name", "score", "active"]
    assert row.to_dict() == {"id": 1, "name": "Ada", "score": 3.14159265358979, "active": True}
    assert dict(row.items()) == row.to_dict()
    assert list(row) == [1, "Ada", 3.14159265358979, True]
    assert row.get("nonexistent", "default") == "default"
    assert row[0] == 1
    assert row[-1] is True, row[-1]  # negative indexing counts from the end
    try:
        row[-99]
        raise AssertionError("expected IndexError for an out-of-range negative index")
    except IndexError:
        pass
    assert len(row) == 4

    one = await pool.query_one("SELECT * FROM pool_and_row_test WHERE id = $1", [1])
    assert one["name"] == "Ada"

    # NULL handling
    await pool.execute("INSERT INTO pool_and_row_test (id) VALUES ($1)", [2])
    null_row = await pool.query_one("SELECT * FROM pool_and_row_test WHERE id = $1", [2])
    assert null_row["name"] is None

    # Error mapping: unique-violation-shaped error surfaces as IntegrityError
    await pool.execute("ALTER TABLE pool_and_row_test ADD CONSTRAINT id_unique UNIQUE (id)")
    try:
        await pool.execute("INSERT INTO pool_and_row_test (id) VALUES ($1)", [1])
        raise AssertionError("expected IntegrityError")
    except PostPyro.IntegrityError:
        pass

    await pool.execute("DROP TABLE pool_and_row_test")
    await pool.close()
    assert pool.is_closed()
    print("OK: pool + row + types + error mapping all verified")


asyncio.run(main())
