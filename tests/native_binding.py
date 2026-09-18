"""Integration tests: native Python -> Postgres binding for non-primitive types.

Before this feature, only bool/int/float/str/None bound natively; a
datetime/uuid/Decimal/dict parameter forced str() + a manual $1::type cast
in the SQL text. These tests pin the native paths end to end against a real
Postgres: write with a native Python object, read back through the decoder,
and assert the round trip is exact.

Same fixture/connection pattern as tests/pool_and_row.py (local Postgres on
localhost:5433).
"""

import asyncio
import datetime
import decimal
import uuid as uuid_module

import PostPyro


async def main():
    pool = await PostPyro.connect("postgresql://postgres:postgres@localhost:5433/postgres", max_size=5)

    await pool.execute("DROP TABLE IF EXISTS native_binding_test")
    await pool.execute(
        """
        CREATE TABLE native_binding_test (
            id INT4,
            ts TIMESTAMP,
            tstz TIMESTAMPTZ,
            d DATE,
            t TIME,
            uid UUID,
            price NUMERIC,
            j JSONB,
            data BYTEA
        )
        """
    )

    # === datetime / date / time / uuid / Decimal / dict / bytes bind natively,
    # with NO ::type casts in the SQL text ===
    naive_dt = datetime.datetime(2024, 3, 15, 10, 30, 0, 123456)
    aware_dt = datetime.datetime(
        2024, 3, 15, 10, 30, 0, 123456, tzinfo=datetime.timezone(datetime.timedelta(hours=2), "EET")
    )
    test_date = datetime.date(2024, 3, 15)
    test_time = datetime.time(10, 30, 0, 123456)
    test_uuid = uuid_module.uuid4()
    test_price = decimal.Decimal("1234.56")
    test_json = {"name": "John", "scores": [85, 92, 78], "nested": {"ok": True, "n": None}}
    test_bytes = b"\x00\x01\xff binary \x00 payload"

    await pool.execute(
        """
        INSERT INTO native_binding_test (id, ts, tstz, d, t, uid, price, j, data)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        """,
        [1, naive_dt, aware_dt, test_date, test_time, test_uuid, test_price, test_json, test_bytes],
    )

    row = await pool.query_one("SELECT * FROM native_binding_test WHERE id = $1", [1])

    # TIMESTAMP round trip, exact to the microsecond
    assert row["ts"] == naive_dt, f"naive datetime round trip: {row['ts']!r}"
    assert row["ts"].tzinfo is None

    # TIMESTAMPTZ: aware datetime stored as UTC instant; Postgres returns it
    # in the session timezone (UTC here) - compare instants, not wall clocks.
    got_tstz = row["tstz"]
    assert got_tstz.tzinfo is not None, f"expected aware datetime, got {got_tstz!r}"
    assert got_tstz.utcoffset() == datetime.timedelta(0), got_tstz.utcoffset()
    assert got_tstz == aware_dt.astimezone(datetime.timezone.utc), (
        f"aware datetime instant changed: sent {aware_dt!r}, got {got_tstz!r}"
    )

    assert row["d"] == test_date, f"date round trip: {row['d']!r}"
    assert type(row["d"]) is datetime.date
    assert row["t"] == test_time, f"time round trip: {row['t']!r}"
    assert row["uid"] == str(test_uuid), f"uuid round trip: {row['uid']!r}"  # decode side is str
    assert row["price"] == test_price, f"decimal round trip: {row['price']!r}"
    assert isinstance(row["price"], decimal.Decimal)
    assert row["j"] == test_json, f"json round trip: {row['j']!r}"
    assert row["data"] == test_bytes, f"bytea round trip: {row['data']!r}"
    assert isinstance(row["data"], bytes)

    # === Native types usable in WHERE clauses too, not just INSERTs ===
    hit = await pool.query_one("SELECT id FROM native_binding_test WHERE uid = $1", [test_uuid])
    assert hit is not None and hit["id"] == 1, "uuid used as a query parameter"
    hit = await pool.query_one("SELECT id FROM native_binding_test WHERE tstz = $1", [aware_dt])
    assert hit is not None and hit["id"] == 1, "aware datetime used as a query parameter"

    # === Aware datetime with a non-UTC offset converts to the same instant ===
    await pool.execute("DELETE FROM native_binding_test WHERE id = 2")
    await pool.execute(
        "INSERT INTO native_binding_test (id, tstz) VALUES ($1, $2)",
        [2, aware_dt.astimezone(datetime.timezone(datetime.timedelta(hours=-5)))],
    )
    row2 = await pool.query_one("SELECT tstz FROM native_binding_test WHERE id = 2")
    assert row2["tstz"] == aware_dt.astimezone(datetime.timezone.utc), row2["tstz"]

    # === json.dumps compatibility: dict/list/tuple/None mix, tuple -> JSON array ===
    await pool.execute("DELETE FROM native_binding_test WHERE id = 3")
    await pool.execute(
        "INSERT INTO native_binding_test (id, j) VALUES ($1, $2)",
        [3, {"list": [1, 2.5, "x", None, True], "tuple": (1, 2), "int_key": {1: "one"}}],
    )
    row3 = await pool.query_one("SELECT j FROM native_binding_test WHERE id = 3")
    assert row3["j"] == {"list": [1, 2.5, "x", None, True], "tuple": [1, 2], "int_key": {"1": "one"}}, row3["j"]

    # === bytearray and empty bytes bind as BYTEA ===
    await pool.execute("DELETE FROM native_binding_test WHERE id = 4")
    ba = bytearray(b"mutable!")
    await pool.execute("INSERT INTO native_binding_test (id, data) VALUES ($1, $2)", [4, ba])
    row4 = await pool.query_one("SELECT data FROM native_binding_test WHERE id = 4")
    assert row4["data"] == b"mutable!", row4["data"]
    # Mutating the bytearray AFTER the call must not corrupt what was sent.
    ba[0] = ord("X")
    row4b = await pool.query_one("SELECT data FROM native_binding_test WHERE id = 4")
    assert row4b["data"] == b"mutable!", row4b["data"]

    await pool.execute("DELETE FROM native_binding_test WHERE id = 5")
    await pool.execute("INSERT INTO native_binding_test (id, data) VALUES ($1, $2)", [5, b""])
    row5 = await pool.query_one("SELECT data FROM native_binding_test WHERE id = 5")
    assert row5["data"] == b"", row5["data"]

    # === Within a transaction as well ===
    tx = await pool.transaction()
    async with tx:
        await tx.execute(
            "INSERT INTO native_binding_test (id, ts, uid, price) VALUES ($1, $2, $3, $4)",
            [10, naive_dt, test_uuid, test_price],
        )
    row_tx = await pool.query_one("SELECT ts, uid, price FROM native_binding_test WHERE id = 10")
    assert row_tx["ts"] == naive_dt and row_tx["uid"] == str(test_uuid) and row_tx["price"] == test_price

    # === Loud failures, not silent corruption ===
    # A time with tzinfo has no native TIME mapping - NotSupportedError, not
    # a silent wall-clock interpretation.
    try:
        await pool.execute(
            "INSERT INTO native_binding_test (id, t) VALUES ($1, $2)",
            [99, datetime.time(10, 30, tzinfo=datetime.timezone.utc)],
        )
        raise AssertionError("expected NotSupportedError for tz-aware time")
    except PostPyro.NotSupportedError:
        pass

    # A JSON int beyond exact i64/u64 range must fail, not degrade to float.
    try:
        await pool.execute(
            "INSERT INTO native_binding_test (id, j) VALUES ($1, $2)",
            [99, {"big": 2**64 + 1}],
        )
        raise AssertionError("expected DataError for out-of-range JSON integer")
    except PostPyro.DataError:
        pass

    # A JSON-incompatible top-level object fails naming the type.
    try:
        await pool.execute(
            "INSERT INTO native_binding_test (id, j) VALUES ($1, $2)",
            [99, {1, 2, 3}],  # a set is not JSON-mappable
        )
        raise AssertionError("expected DataError for a set parameter")
    except PostPyro.DataError:
        pass

    await pool.execute("DROP TABLE native_binding_test")
    await pool.close()
    print("OK: native binding (datetime/date/time/uuid/Decimal/JSON/bytes) verified")


asyncio.run(main())
