"""Regression tests for two Critical bugs found in code review of PR #4:

1. NULL always bound as TEXT (OID 25) regardless of the target column's
   real type, so `None` into an INT4/FLOAT8/BOOL column raised
   "column ... is of type X but expression is of type text" (SQLSTATE
   42804).
2. Non-NULL values of Postgres types not in `pg_value_to_py`'s hardcoded
   list (NUMERIC, UUID, TIMESTAMP, ...) silently decoded as Python `None`
   instead of failing loudly - indistinguishable from a real SQL NULL.

Same fixture/connection pattern as tests/pool_and_row.py (a local Postgres
on localhost:5433).
"""

import asyncio
import datetime
import decimal
import uuid as uuid_module

import PostPyro


async def main():
    pool = await PostPyro.connect("postgresql://postgres:postgres@localhost:5433/postgres", max_size=5)

    await pool.execute("DROP TABLE IF EXISTS type_conversion_bugs_test")
    await pool.execute(
        """
        CREATE TABLE type_conversion_bugs_test (
            id INT4,
            n INT4,
            f FLOAT8,
            b BOOL,
            price NUMERIC(12, 2),
            uid UUID,
            ts TIMESTAMP,
            tstz TIMESTAMPTZ,
            d DATE,
            t TIME,
            j JSONB,
            big_num NUMERIC
        )
        """
    )

    # === Bug 1: NULL into non-TEXT columns must not raise SQLSTATE 42804 ===
    await pool.execute(
        "INSERT INTO type_conversion_bugs_test (id, n, f, b) VALUES ($1, $2, $3, $4)",
        [1, None, None, None],
    )
    row = await pool.query_one("SELECT n, f, b FROM type_conversion_bugs_test WHERE id = 1")
    assert row["n"] is None
    assert row["f"] is None
    assert row["b"] is None

    # Same SQL text, now with real values: guards against a prepared-statement
    # cache collision from the NULL call locking in a mismatched wire type.
    await pool.execute(
        "INSERT INTO type_conversion_bugs_test (id, n, f, b) VALUES ($1, $2, $3, $4)",
        [2, 42, 3.5, True],
    )
    row2 = await pool.query_one("SELECT n, f, b FROM type_conversion_bugs_test WHERE id = 2")
    assert row2["n"] == 42, row2["n"]
    assert row2["f"] == 3.5
    assert row2["b"] is True

    # === Bug 2: previously-unlisted types must decode as their real value,
    # not silently as None ===
    test_uuid = str(uuid_module.uuid4())
    await pool.execute(
        """
        INSERT INTO type_conversion_bugs_test (id, price, uid, ts, tstz, d, t, j)
        VALUES ($1, $2::numeric, $3::uuid, $4::timestamp, $5::timestamptz, $6::date, $7::time, $8::jsonb)
        """,
        [
            3,
            "1234.56",
            test_uuid,
            "2024-03-15 10:30:00.123456",
            "2024-03-15 10:30:00.123456+00",
            "2024-03-15",
            "10:30:00.123456",
            '{"a": 1, "b": [1, 2, 3], "c": null, "d": "x"}',
        ],
    )
    row3 = await pool.query_one(
        "SELECT price, uid, ts, tstz, d, t, j FROM type_conversion_bugs_test WHERE id = 3"
    )

    price = row3["price"]
    assert isinstance(price, decimal.Decimal), f"expected Decimal, got {type(price)}: {price!r}"
    assert price == decimal.Decimal("1234.56")

    uid = row3["uid"]
    assert isinstance(uid, str), f"expected str, got {type(uid)}: {uid!r}"
    assert uid == test_uuid

    ts = row3["ts"]
    assert isinstance(ts, datetime.datetime), f"expected datetime, got {type(ts)}: {ts!r}"
    assert ts == datetime.datetime(2024, 3, 15, 10, 30, 0, 123456)
    assert ts.tzinfo is None

    tstz = row3["tstz"]
    assert isinstance(tstz, datetime.datetime)
    assert tstz.tzinfo is not None
    assert tstz.utcoffset() == datetime.timedelta(0)
    assert (tstz.year, tstz.month, tstz.day) == (2024, 3, 15)
    assert (tstz.hour, tstz.minute, tstz.microsecond) == (10, 30, 123456)

    d = row3["d"]
    assert type(d) is datetime.date, f"expected date, got {type(d)}: {d!r}"
    assert d == datetime.date(2024, 3, 15)

    t = row3["t"]
    assert isinstance(t, datetime.time), f"expected time, got {type(t)}: {t!r}"
    assert t == datetime.time(10, 30, 0, 123456)

    j = row3["j"]
    assert j == {"a": 1, "b": [1, 2, 3], "c": None, "d": "x"}, j

    # A genuinely unsupported type must raise loudly, not decode as None.
    await pool.execute("ALTER TABLE type_conversion_bugs_test ADD COLUMN pt POINT")
    await pool.execute("UPDATE type_conversion_bugs_test SET pt = '(1,2)' WHERE id = 3")
    try:
        await pool.query_one("SELECT pt FROM type_conversion_bugs_test WHERE id = 3")
        # `assert False` is stripped entirely under `python -O`, silently
        # defeating this check - raise unconditionally instead.
        raise AssertionError("expected NotSupportedError for an unhandled Postgres type")
    except PostPyro.NotSupportedError:
        pass

    # === NUMERIC beyond rust_decimal's ~28-29 significant digit limit -
    # bigdecimal::BigDecimal (arbitrary precision) must round-trip exactly ===
    big_num_str = "123456789012345678901234567890.123456789"  # 39 significant digits
    await pool.execute(
        "INSERT INTO type_conversion_bugs_test (id, big_num) VALUES ($1, $2::numeric)",
        [4, big_num_str],
    )
    row4 = await pool.query_one("SELECT big_num FROM type_conversion_bugs_test WHERE id = 4")
    big_num = row4["big_num"]
    assert isinstance(big_num, decimal.Decimal), f"expected Decimal, got {type(big_num)}: {big_num!r}"
    assert big_num == decimal.Decimal(big_num_str), big_num

    # === JSON integer above i64::MAX but within u64 must not lose precision
    # by falling through to a lossy f64 ===
    u64_max = 18446744073709551615  # u64::MAX, > i64::MAX
    await pool.execute(
        "INSERT INTO type_conversion_bugs_test (id, j) VALUES ($1, $2::jsonb)",
        [5, f'{{"big": {u64_max}}}'],
    )
    row5 = await pool.query_one("SELECT j FROM type_conversion_bugs_test WHERE id = 5")
    j5 = row5["j"]
    assert isinstance(j5["big"], int), f"expected int, got {type(j5['big'])}: {j5['big']!r}"
    assert j5["big"] == u64_max, j5["big"]

    await pool.execute("DROP TABLE type_conversion_bugs_test")
    await pool.close()
    print("OK: NULL binding + non-text type decoding regressions verified")


asyncio.run(main())
