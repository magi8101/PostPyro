use bigdecimal::BigDecimal;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use sqlx::postgres::PgRow;
use sqlx::postgres::types::Oid;
use sqlx::{Column, Postgres, Row as SqlxRow, TypeInfo};
use uuid::Uuid;

use crate::error::{map_db_error, NotSupportedError};

/// A bind value for SQL `NULL` that declares its Postgres parameter type as
/// OID 0 ("unspecified") instead of a concrete type.
///
/// This is what lets Postgres infer the real column type from context
/// (`column_description` / assignment target) during Parse, exactly like
/// libpq/asyncpg do for untyped NULL parameters, instead of us guessing and
/// getting it wrong (see `bind_params` doc comment for why guessing is
/// wrong). `Type::type_info()`/`Encode::produces()` both point at
/// `PgTypeInfo::with_oid(Oid(0))`, which sqlx resolves locally (no DB round
/// trip - see `PgConnection::try_type_to_oid`) and sends verbatim as the
/// parameter's declared OID in the Parse message.
struct UnspecifiedNull;

impl sqlx::Type<Postgres> for UnspecifiedNull {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_oid(Oid(0))
    }

    fn compatible(_ty: &sqlx::postgres::PgTypeInfo) -> bool {
        true
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for UnspecifiedNull {
    fn encode_by_ref(
        &self,
        _buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        Ok(sqlx::encode::IsNull::Yes)
    }

    fn produces(&self) -> Option<sqlx::postgres::PgTypeInfo> {
        Some(sqlx::postgres::PgTypeInfo::with_oid(Oid(0)))
    }
}

/// Bind a Python parameter list onto a query, one `.bind()` call per
/// parameter. sqlx only exposes `.bind()` (not a standalone Arguments
/// builder) for the default `Query<DB, DB::Arguments>` returned by
/// `sqlx::query()`, so this takes and returns that concrete type.
///
/// ponytail: Python ints always bind as Postgres BIGINT (i8/int8). This is
/// simple and deterministic, unlike guessing i16/i32/i64 from the value's
/// magnitude (what the old driver did). If an INSERT/UPDATE against a
/// narrower INT2/INT4 column needs an exact type match, cast in the SQL
/// text (`$1::int4`). Revisit only if this becomes a real friction point.
///
/// Every query built here is marked non-persistent (`.persistent(false)`).
/// sqlx's prepared-statement cache is keyed on the raw SQL text only, not on
/// the bound argument types (see `PgConnection::get_or_prepare`). Since a
/// `None` parameter is now bound with `PgTypeInfo::with_oid(Oid(0))`
/// ("unspecified" - let Postgres infer it), the *first* execution of a given
/// SQL text bakes whatever type Postgres inferred into the cached prepared
/// statement; a later call to the same SQL text with a real (non-NULL) value
/// of a different wire size (e.g. our i64 for Python ints vs. an inferred
/// INT4 column) then fails with "incorrect binary data format in bind
/// parameter" - live-verified against Postgres 16. Disabling the
/// server-side statement cache avoids the collision at the cost of a fresh
/// Parse+Describe round trip per query. Upgrade path if that cost matters:
/// key the cache on (SQL text, argument type fingerprint) instead of SQL
/// text alone.
pub fn bind_params<'q>(
    query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    py: Python,
    params: &[PyObject],
) -> PyResult<sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>> {
    let mut has_null = false;
    let mut query = query;
    for obj in params {
        let obj_ref = obj.as_ref(py);
        if obj.is_none(py) {
            has_null = true;
            query = query.bind(UnspecifiedNull);
        } else if let Ok(b) = obj_ref.downcast::<PyBool>() {
            query = query.bind(b.extract::<bool>()?)
        } else if let Ok(i) = obj_ref.downcast::<PyInt>() {
            query = query.bind(i.extract::<i64>()?)
        } else if let Ok(f) = obj_ref.downcast::<PyFloat>() {
            query = query.bind(f.extract::<f64>()?)
        } else if let Ok(s) = obj_ref.downcast::<PyString>() {
            query = query.bind(s.extract::<String>()?)
        } else {
            let s = obj_ref.str()?.extract::<String>()?;
            query = query.bind(s)
        };
    }
    Ok(query.persistent(!has_null))
}

/// Decode one column of a type that has both sqlx's wire decode (`Decode`/`Type`)
/// and pyo3's Python conversion (`IntoPy`). Handles NULLs by returning Python's `None`.
fn decode_scalar<'r, T>(py: Python, row: &'r PgRow, idx: usize) -> PyResult<PyObject>
where
    T: sqlx::Decode<'r, Postgres> + sqlx::Type<Postgres> + IntoPy<PyObject>,
{
    let value: Option<T> = row.try_get(idx).map_err(map_db_error)?;
    Ok(value.map(|v| v.into_py(py)).unwrap_or_else(|| py.None()))
}

/// NUMERIC has no native Python scalar equivalent - decode via
/// `bigdecimal::BigDecimal` (sqlx's `bigdecimal` feature, arbitrary
/// precision - unlike `rust_decimal::Decimal`, which caps out around 28-29
/// significant digits) and hand the exact string representation to Python's
/// `decimal.Decimal` so we don't round-trip through a lossy `f64`.
fn decode_numeric(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let value: Option<BigDecimal> = row.try_get(idx).map_err(map_db_error)?;
    match value {
        None => Ok(py.None()),
        Some(d) => {
            let decimal_cls = py.import("decimal")?.getattr("Decimal")?;
            Ok(decimal_cls.call1((d.to_string(),))?.into_py(py))
        }
    }
}

/// UUID -> Python `str`. pyo3 has no built-in UUID conversion (no `uuid`
/// feature), and a string is simplest/least-surprising for a DB-API driver.
fn decode_uuid(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let value: Option<Uuid> = row.try_get(idx).map_err(map_db_error)?;
    Ok(value
        .map(|u| u.to_string().into_py(py))
        .unwrap_or_else(|| py.None()))
}

/// pyo3's `chrono` feature (auto `IntoPy` for chrono types) is compiled out
/// under `abi3` (`#![cfg(all(feature = "chrono", not(Py_LIMITED_API)))]` in
/// pyo3's own source), and so - less obviously - is pyo3's *own*
/// `PyDate`/`PyTime`/`PyDateTime` wrapper API
/// (`#[cfg(not(Py_LIMITED_API))]` on the whole module) - this crate builds
/// `abi3-py38`. So instead we build `datetime.date`/`datetime.time`/
/// `datetime.datetime` the same way plain Python code would: by calling the
/// stdlib `datetime` module's constructors through the general `PyAny` API,
/// which has no abi3 restriction.
fn naive_date_to_py(py: Python, d: NaiveDate) -> PyResult<PyObject> {
    let date_cls = py.import("datetime")?.getattr("date")?;
    Ok(date_cls.call1((d.year(), d.month(), d.day()))?.into_py(py))
}

fn naive_time_to_py(py: Python, t: NaiveTime) -> PyResult<PyObject> {
    let micros = t.nanosecond() % 1_000_000_000 / 1_000;
    let time_cls = py.import("datetime")?.getattr("time")?;
    Ok(time_cls
        .call1((t.hour(), t.minute(), t.second(), micros))?
        .into_py(py))
}

fn naive_datetime_to_py(py: Python, dt: NaiveDateTime) -> PyResult<PyObject> {
    let micros = dt.and_utc().timestamp_subsec_micros();
    let datetime_cls = py.import("datetime")?.getattr("datetime")?;
    Ok(datetime_cls
        .call1((
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
            micros,
        ))?
        .into_py(py))
}

fn utc_datetime_to_py(py: Python, dt: DateTime<Utc>) -> PyResult<PyObject> {
    let datetime_mod = py.import("datetime")?;
    let datetime_cls = datetime_mod.getattr("datetime")?;
    let utc_tz = datetime_mod.getattr("timezone")?.getattr("utc")?;
    let micros = dt.timestamp_subsec_micros();
    Ok(datetime_cls
        .call1((
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
            micros,
            utc_tz,
        ))?
        .into_py(py))
}

fn decode_date(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let value: Option<NaiveDate> = row.try_get(idx).map_err(map_db_error)?;
    value.map_or(Ok(py.None()), |d| naive_date_to_py(py, d))
}

fn decode_time(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let value: Option<NaiveTime> = row.try_get(idx).map_err(map_db_error)?;
    value.map_or(Ok(py.None()), |t| naive_time_to_py(py, t))
}

fn decode_timestamp(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let value: Option<NaiveDateTime> = row.try_get(idx).map_err(map_db_error)?;
    value.map_or(Ok(py.None()), |dt| naive_datetime_to_py(py, dt))
}

fn decode_timestamptz(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let value: Option<DateTime<Utc>> = row.try_get(idx).map_err(map_db_error)?;
    value.map_or(Ok(py.None()), |dt| utc_datetime_to_py(py, dt))
}

/// JSON/JSONB -> native Python object (dict/list/str/int/float/bool/None),
/// via `serde_json::Value` (already a workspace dependency) recursively
/// converted rather than round-tripping through `json.loads` on raw text -
/// sqlx's `Decode` for `serde_json::Value` already handles JSONB's leading
/// binary-format version byte correctly, so we don't have to.
fn decode_json(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let value: Option<serde_json::Value> = row.try_get(idx).map_err(map_db_error)?;
    Ok(value
        .map(|v| json_to_py(py, &v))
        .unwrap_or_else(|| py.None()))
}

fn json_to_py(py: Python, value: &serde_json::Value) -> PyObject {
    match value {
        serde_json::Value::Null => py.None(),
        serde_json::Value::Bool(b) => b.into_py(py),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py(py)
            } else if let Some(u) = n.as_u64() {
                u.into_py(py)
            } else if let Some(f) = n.as_f64() {
                f.into_py(py)
            } else {
                n.to_string().into_py(py)
            }
        }
        serde_json::Value::String(s) => s.into_py(py),
        serde_json::Value::Array(items) => {
            let list = PyList::new(py, items.iter().map(|item| json_to_py(py, item)));
            list.into_py(py)
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (key, val) in map {
                dict.set_item(key, json_to_py(py, val))
                    .expect("setting an item on a fresh PyDict should not fail");
            }
            dict.into_py(py)
        }
    }
}

/// Convert one PgRow column to a Python object, type-specialized per
/// Postgres type. Anything not listed here raises `NotSupportedError`
/// naming the unhandled type, rather than silently decoding as `None` -
/// silent data loss (a real value read back indistinguishable from an
/// actual SQL NULL) is worse than a loud failure.
pub fn pg_value_to_py(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let type_name = row.column(idx).type_info().name();
    match type_name {
        "BOOL" => decode_scalar::<bool>(py, row, idx),
        "INT2" => decode_scalar::<i16>(py, row, idx),
        "INT4" => decode_scalar::<i32>(py, row, idx),
        "INT8" => decode_scalar::<i64>(py, row, idx),
        "FLOAT4" => decode_scalar::<f32>(py, row, idx),
        "FLOAT8" => decode_scalar::<f64>(py, row, idx),
        "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" => decode_scalar::<String>(py, row, idx),
        "NUMERIC" => decode_numeric(py, row, idx),
        "UUID" => decode_uuid(py, row, idx),
        "TIMESTAMP" => decode_timestamp(py, row, idx),
        "TIMESTAMPTZ" => decode_timestamptz(py, row, idx),
        "DATE" => decode_date(py, row, idx),
        "TIME" => decode_time(py, row, idx),
        "JSON" | "JSONB" => decode_json(py, row, idx),
        other => Err(NotSupportedError::new_err(format!(
            "column {} has unsupported Postgres type '{}' - PostPyro cannot decode this type yet",
            idx, other
        ))),
    }
}
