"""A homogeneous Python list/tuple binds as a real Postgres array
(bool[]/int8[]/float8[]/text[]) instead of being JSON-encoded - see
py_sequence_to_array in src/types.rs. Mixed-type or nested lists, and any
dict, still bind as JSON/JSONB via py_to_json (tests/native_binding.py
covers that path). A bare set/frozenset must raise DataError, not silently
bind as TEXT against a placeholder another call already prepared as JSONB.

Same fixture/connection pattern as tests/native_binding.py (a local
Postgres on localhost:5433).
"""

import asyncio

import PostPyro


async def main():
    pool = await PostPyro.connect("postgresql://postgres:postgres@localhost:5433/postgres", max_size=5)

    await pool.execute("DROP TABLE IF EXISTS native_array_binding_test")
    await pool.execute(
        """
        CREATE TABLE native_array_binding_test (
            id INT4,
            ints INT8[],
            floats FLOAT8[],
            bools BOOL[],
            texts TEXT[],
            mixed JSONB
        )
        """
    )

    # === Homogeneous arrays round-trip natively, no cast needed ===
    await pool.execute(
        "INSERT INTO native_array_binding_test (id, ints, floats, bools, texts) VALUES ($1, $2, $3, $4, $5)",
        [1, [1, 2, 3], [1.5, 2.5], [True, False, True], ["a", "b", "c"]],
    )
    row = await pool.query_one("SELECT ints, floats, bools, texts FROM native_array_binding_test WHERE id = 1")
    assert row["ints"] == [1, 2, 3]
    assert row["floats"] == [1.5, 2.5]
    assert row["bools"] == [True, False, True]
    assert row["texts"] == ["a", "b", "c"]

    # === Tuples bind the same way as lists ===
    await pool.execute(
        "INSERT INTO native_array_binding_test (id, ints) VALUES ($1, $2)",
        [2, (10, 20, 30)],
    )
    row = await pool.query_one("SELECT ints FROM native_array_binding_test WHERE id = 2")
    assert row["ints"] == [10, 20, 30]

    # === None entries inside a list become SQL NULL array elements ===
    await pool.execute(
        "INSERT INTO native_array_binding_test (id, ints) VALUES ($1, $2)",
        [3, [1, None, 3]],
    )
    row = await pool.query_one("SELECT ints FROM native_array_binding_test WHERE id = 3")
    assert row["ints"] == [1, None, 3]

    # === Empty list binds as an empty array ===
    await pool.execute(
        "INSERT INTO native_array_binding_test (id, texts) VALUES ($1, $2)",
        [4, []],
    )
    row = await pool.query_one("SELECT texts FROM native_array_binding_test WHERE id = 4")
    assert row["texts"] == []

    # === Mixed-type list falls back to JSON, not TEXT and not an array ===
    await pool.execute(
        "INSERT INTO native_array_binding_test (id, mixed) VALUES ($1, $2)",
        [5, [1, "two", 3.0]],
    )
    row = await pool.query_one("SELECT mixed FROM native_array_binding_test WHERE id = 5")
    assert row["mixed"] == [1, "two", 3.0]

    # === Nested list falls back to JSON too (not a flat array) ===
    await pool.execute(
        "INSERT INTO native_array_binding_test (id, mixed) VALUES ($1, $2)",
        [6, [[1, 2], [3, 4]]],
    )
    row = await pool.query_one("SELECT mixed FROM native_array_binding_test WHERE id = 6")
    assert row["mixed"] == [[1, 2], [3, 4]]

    # === A bare set/frozenset raises DataError, doesn't corrupt a JSONB
    # placeholder already cached from the inserts above ===
    for bad in ({1, 2, 3}, frozenset({1, 2})):
        try:
            await pool.execute(
                "INSERT INTO native_array_binding_test (id, mixed) VALUES ($1, $2)",
                [99, bad],
            )
            raise AssertionError(f"expected DataError for {type(bad).__name__} parameter")
        except PostPyro.DataError:
            pass

    await pool.close()
    print("native_array_binding.py: all assertions passed")


if __name__ == "__main__":
    asyncio.run(main())
