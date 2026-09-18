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

    # === DST: same aware wall-clock in a UTC+2 zone at both offsets must
    # land on different instants, and each must round trip exactly. This is
    # the test that catches calling tzinfo.utcoffset() without the datetime
    # (wrong object - TypeError) or ignoring the offset entirely.
    zone_plus2 = datetime.timezone(datetime.timedelta(hours=2), "UTC+2")
    summer = datetime.datetime(2024, 7, 15, 12, 0, 0, tzinfo=zone_plus2)
    winter = datetime.datetime(2024, 1, 15, 12, 0, 0, tzinfo=zone_plus2)
    assert summer.utcoffset() == winter.utcoffset(), "fixed-offset zone sanity"
    await pool.execute("DELETE FROM native_binding_test WHERE id IN (6, 7)")
    await pool.execute(
        "INSERT INTO native_binding_test (id, tstz) VALUES ($1, $2), ($3, $4)",
        [6, summer, 7, winter],
    )
    for rid, sent in ((6, summer), (7, winter)):
        got = (await pool.query_one("SELECT tstz FROM native_binding_test WHERE id = $1", [rid]))["tstz"]
        assert got == sent.astimezone(datetime.timezone.utc), f"id={rid}: sent {sent!r}, got {got!r}"

    # A real DST-observing zone (us/Eastern-like via two fixed offsets, since
    # zoneinfo may be absent on some wheels): offset differs by month, and
    # each datetime must convert using ITS OWN offset.
    dst_summer = datetime.datetime(2024, 7, 15, 12, 0, 0, tzinfo=datetime.timezone(datetime.timedelta(hours=-4)))
    dst_winter = datetime.datetime(2024, 1, 15, 12, 0, 0, tzinfo=datetime.timezone(datetime.timedelta(hours=-5)))
    assert dst_summer.utcoffset() != dst_winter.utcoffset()
    await pool.execute("DELETE FROM native_binding_test WHERE id IN (8, 9)")
    await pool.execute(
        "INSERT INTO native_binding_test (id, tstz) VALUES ($1, $2), ($3, $4)",
        [8, dst_summer, 9, dst_winter],
    )
    for rid, sent in ((8, dst_summer), (9, dst_winter)):
        got = (await pool.query_one("SELECT tstz FROM native_binding_test WHERE id = $1", [rid]))["tstz"]
        assert got == sent.astimezone(datetime.timezone.utc), f"id={rid}: sent {sent!r}, got {got!r}"

    # === datetime.min / datetime.max extremes round trip exactly ===
    await pool.execute("DELETE FROM native_binding_test WHERE id = 10")
    await pool.execute(
        "INSERT INTO native_binding_test (id, ts) VALUES ($1, $2)",
        [10, datetime.datetime.min],
    )
    got = (await pool.query_one("SELECT ts FROM native_binding_test WHERE id = 10"))["ts"]
    assert got == datetime.datetime.min, got
    await pool.execute("UPDATE native_binding_test SET ts = $1 WHERE id = 10", [datetime.datetime.max])
    got = (await pool.query_one("SELECT ts FROM native_binding_test WHERE id = 10"))["ts"]
    assert got == datetime.datetime.max, got

    # === Decimal extremes: high precision, trailing zeros preserved through
    # NUMERIC (read back equal, not float-comparable) ===
    await pool.execute("DELETE FROM native_binding_test WHERE id = 11")
    hi_prec = decimal.Decimal("1234567890.123456789012345678901234567890")
    trailing = decimal.Decimal("10.500")
    await pool.execute(
        "INSERT INTO native_binding_test (id, price) VALUES ($1, $2), ($3, $4)",
        [11, hi_prec, 12, trailing],
    )
    got = (await pool.query_one("SELECT price FROM native_binding_test WHERE id = 11"))["price"]
    assert got == hi_prec, f"high-precision decimal: {got!r}"
    got = (await pool.query_one("SELECT price FROM native_binding_test WHERE id = 12"))["price"]
    assert got == trailing, f"trailing zeros: {got!r}"

    # === A datetime subclass binds as a datetime (isinstance-based check),
    # and deep-but-legal JSON nesting works ===
    class MyDatetime(datetime.datetime):
        pass

    await pool.execute("DELETE FROM native_binding_test WHERE id = 13")
    await pool.execute(
        "INSERT INTO native_binding_test (id, ts) VALUES ($1, $2)",
        [13, MyDatetime(2024, 3, 15, 10, 30, 0, 123456)],
    )
    got = (await pool.query_one("SELECT ts FROM native_binding_test WHERE id = 13"))["ts"]
    assert got == naive_dt, f"datetime subclass: {got!r}"

    deep = current = {}
    for i in range(100):
        current["child"] = {}
        current = current["child"]
    await pool.execute("DELETE FROM native_binding_test WHERE id = 14")
    await pool.execute("INSERT INTO native_binding_test (id, j) VALUES ($1, $2)", [14, deep])
    got = (await pool.query_one("SELECT j FROM native_binding_test WHERE id = 14"))["j"]
    assert got == deep, "100-level nested JSON"

    # === memoryview binds as BYTEA (repr must never leak in as TEXT) ===
    await pool.execute("DELETE FROM native_binding_test WHERE id = 15")
    await pool.execute(
        "INSERT INTO native_binding_test (id, data) VALUES ($1, $2)",
        [15, memoryview(b"through memoryview")],
    )
    got = (await pool.query_one("SELECT data FROM native_binding_test WHERE id = 15"))["data"]
    assert got == b"through memoryview", got

    # === A JSON parameter nested past the guard fails loudly (no stack
    # overflow / abort), and an object with a self-referential dict too ===
    beyond = current = {}
    for _ in range(200):
        current["child"] = {}
        current = current["child"]
    try:
        await pool.execute("INSERT INTO native_binding_test (id, j) VALUES ($1, $2)", [99, beyond])
        raise AssertionError("expected DataError for over-deep JSON nesting")
    except PostPyro.DataError:
        pass

    selfref = {}
    selfref["self"] = selfref
    try:
        await pool.execute("INSERT INTO native_binding_test (id, j) VALUES ($1, $2)", [99, selfref])
        raise AssertionError("expected DataError for a self-referential dict")
    except PostPyro.DataError:
        pass

    # === Mixed naive + aware datetimes in one call to the same SQL text
    # must not poison sqlx's prepared-statement cache (TIMESTAMP vs
    # TIMESTAMPTZ wire types for one parameter slot) ===
    await pool.execute("DELETE FROM native_binding_test WHERE id IN (16, 17)")
    await pool.execute(
        "INSERT INTO native_binding_test (id, tstz) VALUES ($1, $2)",
        [16, datetime.datetime(2024, 3, 15, 10, 30, 0, tzinfo=datetime.timezone.utc)],
    )
    # Same SQL text, now naive (different wire type, same parameter slot).
    # With the non-persistent fix this re-prepares; without it Postgres
    # rejects the bind or silently misreads the value.
    await pool.execute(
        "INSERT INTO native_binding_test (id, tstz) VALUES ($1, $2)",
        [17, datetime.datetime(2024, 3, 16, 10, 30, 0)],
    )
    got16 = (await pool.query_one("SELECT tstz FROM native_binding_test WHERE id = 16"))["tstz"]
    got17 = (await pool.query_one("SELECT tstz FROM native_binding_test WHERE id = 17"))["tstz"]
    assert got16 == datetime.datetime(2024, 3, 15, 10, 30, 0, tzinfo=datetime.timezone.utc), got16
    # Naive into timestamptz is interpreted in the session zone (UTC here).
    assert got17 == datetime.datetime(2024, 3, 16, 10, 30, 0, tzinfo=datetime.timezone.utc), got17

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
