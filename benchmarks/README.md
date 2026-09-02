# Benchmarks

Two scripts, two different purposes:

- `bench_vs_alternatives.py` - timing. PostPyro vs. `asyncpg` vs. `psycopg` (v3, async) on the same operations against the same Postgres.
- `bench_concurrency_correctness.py` - correctness under concurrent load, not speed. Exercises the class of bug the async rewrite exists to fix (the old driver held the GIL during I/O; concurrent asyncio tasks didn't really run concurrently and could step on each other).

## Setup

Start a throwaway Postgres, same pattern the test suite (`tests/*.py`) uses:

```bash
docker run -d --rm --name postpyro-bench-pg -e POSTGRES_PASSWORD=postgres -p 5433:5432 postgres:16
```

Both scripts default to `postgresql://postgres:postgres@localhost:5433/postgres`; pass a different DSN as the first argument if you're pointing at something else.

Install PostPyro itself (`pip install -e .` from the repo root, or a built wheel) - it's the one dependency neither script can skip.

The comparison drivers are optional and dev-only, not something PostPyro depends on at runtime:

```bash
pip install -r benchmarks/requirements.txt
```

Skip that and `bench_vs_alternatives.py` still runs - it just benchmarks PostPyro alone and prints a note for each driver it couldn't import.

## Running

```bash
python3 benchmarks/bench_vs_alternatives.py
python3 benchmarks/bench_concurrency_correctness.py
```

`bench_vs_alternatives.py` prints a min/median/mean table (milliseconds, over 5 repetitions) for: a `SELECT 1` round-trip loop, a 1000-row single-statement-at-a-time INSERT, a multi-statement transaction, and N concurrent tasks each running a query through the pool.

`bench_concurrency_correctness.py` prints pass/fail for each scenario, not a timing table - it's an `assert`-based check, not a benchmark, and exits non-zero if anything fails.

## Teardown

```bash
docker stop postpyro-bench-pg
```

## Caveat

These are numbers from one machine, one Docker Postgres on localhost, at one point in time - not a claim about any other hardware, network, Postgres config, or workload. Re-run them yourself before repeating a number anywhere that matters.
