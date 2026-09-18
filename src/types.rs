use bigdecimal::BigDecimal;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use sqlx::postgres::PgRow;
use sqlx::postgres::types::Oid;
use sqlx::{Column, Postgres, Row as SqlxRow, TypeInfo};
use std::str::FromStr;
use uuid::Uuid;

use crate::error::{map_db_error, DataError, NotSupportedError};

/// Python stdlib classes used to recognize bindable parameter types.
///
/// We can't `downcast::<PyDateTime>()` for this: pyo3 compiles its own
/// `PyDate`/`PyTime`/`PyDateTime` wrapper API out under `abi3`
/// (`#[cfg(not(Py_LIMITED_API))]` - same finding already documented in
/// `naive_date_to_py` below), and there are no pyo3 wrappers at all for
/// `uuid.UUID` or `decimal.Decimal`. So we hold the class objects themselves
/// (fetched once, `GILOnceCell` mediating with the GIL) and check with
/// Python-level `isinstance`, which also honors subclasses the way users
/// expect (e.g. a `datetime` subclass instance binds as a datetime).
static PY_DATETIME: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
static PY_DATE: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
static PY_TIME: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
static PY_UUID: GILOnceCell<Py<PyAny>> = GILOnceCell::new();
static PY_DECIMAL: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

fn py_class<'py>(
    py: Python<'py>,
    cell: &'static GILOnceCell<Py<PyAny>>,
    module: &str,
    name: &str,
) -> PyResult<&'py PyAny> {
    let cls = cell.get_or_try_init(py, || -> PyResult<Py<PyAny>> {
        Ok(py.import(module)?.getattr(name)?.into_py(py))
    })?;
    Ok(cls.as_ref(py))
}

fn py_isinstance(
    py: Python,
    obj: &PyAny,
    cell: &'static GILOnceCell<Py<PyAny>>,
    module: &str,
    name: &str,
) -> PyResult<bool> {
    let cls = py_class(py, cell, module, name)?;
    py.import("builtins")?
        .getattr("isinstance")?
        .call1((obj, cls))?
        .is_true()
}

/// Convert a Python dict/list/tuple into a `serde_json::Value` for binding
/// against JSON/JSONB columns. Mirrors the read side (`json_to_py` below)
/// in reverse. Python ints beyond JSON's exact range (i64/u64) raise
/// `DataError` rather than silently degrading to a lossy float - the read
/// side hands such numbers through as strings, but guessing on the write
/// side would corrupt data, so it fails loudly instead.
fn py_to_json(py: Python, obj: &PyAny) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.downcast::<PyBool>() {
        Ok(serde_json::Value::Bool(b.extract::<bool>()?))
    } else if let Ok(i) = obj.downcast::<PyInt>() {
        if let Ok(v) = i.extract::<i64>() {
            Ok(serde_json::Value::from(v))
        } else if let Ok(v) = i.extract::<u64>() {
            Ok(serde_json::Value::from(v))
        } else {
            Err(DataError::new_err(
                "JSON integer is outside the exact range JSON can represent (i64/u64) - bind it as a string or numeric instead",
            ))
        }
    } else if let Ok(f) = obj.downcast::<PyFloat>() {
        Ok(serde_json::Value::from(f.extract::<f64>()?))
    } else if let Ok(s) = obj.downcast::<PyString>() {
        Ok(serde_json::Value::from(s.extract::<String>()?))
    } else if let Ok(l) = obj.downcast::<PyList>() {
        let mut items = Vec::with_capacity(l.len());
        for item in l.iter() {
            items.push(py_to_json(py, item)?);
        }
        Ok(serde_json::Value::Array(items))
    } else if let Ok(t) = obj.downcast::<PyTuple>() {
        let mut items = Vec::with_capacity(t.len());
        for item in t.iter() {
            items.push(py_to_json(py, item)?);
        }
        Ok(serde_json::Value::Array(items))
    } else if let Ok(d) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in d.iter() {
            // JSON object keys are strings; non-string keys are stringified
            // (same as json.dumps does for int keys), not an error.
            let key = k.str()?.extract::<String>()?;
            map.insert(key, py_to_json(py, v)?);
        }
        Ok(serde_json::Value::Object(map))
    } else {
        let type_name = obj
            .get_type()
            .name()
            .ok()
            .and_then(|n| n.to_str().ok())
            .unwrap_or("unknown");
        Err(DataError::new_err(format!(
            "cannot bind a {} object to a JSON/JSONB parameter - pass a dict/list/tuple/str/int/float/bool/None",
            type_name
        )))
    }
}

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

/// Read an int attribute off a Python object (e.g. `dt.year`, `d.month`).
fn get_int_attr(obj: &PyAny, attr: &str) -> PyResult<i32> {
    obj.getattr(attr)?.extract::<i32>()
}

/// Which Postgres temporal type a Python `datetime.datetime` binds as:
/// naive wall-clock -> TIMESTAMP, timezone-aware -> TIMESTAMPTZ (instant
/// normalized to UTC). Splitting them keeps the wire type matching the
/// column type instead of relying on the server's implicit
timestamptz->timestamp cast under the session timezone.
enum PyDateTimeParam {
    Timestamp(NaiveDateTime),
    Timestamptz(DateTime<Utc>),
}

/// Python `datetime.datetime` -> the right sqlx bind parameter. Attributes
/// are read through the plain `PyAny` API because pyo3 compiles its chrono
/// `FromPyObject` impls out under `abi3` (same constraint as the decode
/// side below).
fn py_datetime_to_param(obj: &PyAny) -> PyResult<PyDateTimeParam> {
    let naive = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(
            get_int_attr(obj, "year")?,
            get_int_attr(obj, "month")?,
            get_int_attr(obj, "day")?,
        )
        .ok_or_else(|| DataError::new_err("invalid date component in datetime parameter"))?,
        NaiveTime::from_hms_micro_opt(
            get_int_attr(obj, "hour")?,
            get_int_attr(obj, "minute")?,
            get_int_attr(obj, "second")?,
            get_int_attr(obj, "microsecond")?,
        )
        .ok_or_else(|| DataError::new_err("invalid time component in datetime parameter"))?,
    );
    let tzinfo = obj.getattr("tzinfo")?;
    if tzinfo.is_none() {
        return Ok(PyDateTimeParam::Timestamp(naive));
    }
    // utcoffset() -> timedelta (or None, for a tzinfo that doesn't know its
    // own offset - treat like naive rather than guessing).
    let offset = tzinfo.call_method0("utcoffset")?;
    if offset.is_none() {
        return Ok(PyDateTimeParam::Timestamp(naive));
    }
    let total_seconds = offset.call_method0("total_seconds")?.extract::<f64>()?;
    if total_seconds != total_seconds.trunc() || total_seconds.abs() > 86_399.0 {
        return Err(DataError::new_err(format!(
            "datetime parameter has an invalid utcoffset ({} seconds) - Postgres offsets are whole seconds within +/-24h",
            total_seconds
        )));
    }
    let fixed = chrono::FixedOffset::east_opt(total_seconds as i32)
        .ok_or_else(|| DataError::new_err("datetime parameter utcoffset out of range"))?;
    Ok(PyDateTimeParam::Timestamptz(DateTime::from_naive_utc_and_offset(
        naive - fixed,
        Utc,
    )))
}

/// Python `datetime.date` -> `chrono::NaiveDate`.
fn py_date_to_chrono(obj: &PyAny) -> PyResult<NaiveDate> {
    NaiveDate::from_ymd_opt(
        get_int_attr(obj, "year")?,
        get_int_attr(obj, "month")?,
        get_int_attr(obj, "day")?,
    )
    .ok_or_else(|| DataError::new_err("invalid date parameter"))
}

/// Python `datetime.time` -> `chrono::NaiveTime`.
fn py_time_to_chrono(obj: &PyAny) -> PyResult<NaiveTime> {
    let t = NaiveTime::from_hms_micro_opt(
        get_int_attr(obj, "hour")?,
        get_int_attr(obj, "minute")?,
        get_int_attr(obj, "second")?,
        get_int_attr(obj, "microsecond")?,
    )
    .ok_or_else(|| DataError::new_err("invalid time parameter"))?;
    // A time with tzinfo != None binds as its naive wall-clock value; Postgres
    // has TIMETZ but sqlx's chrono support maps NaiveTime to TIME only, and
    // TIMETZ is documented by Postgres itself as mostly for legacy use.
    if !obj.getattr("tzinfo")?.is_none() {
        return Err(NotSupportedError::new_err(
            "bind a tz-aware datetime.time as a TIMESTAMPTZ datetime instead, or strip tzinfo - PostPyro binds times as TIME (no TIMETZ support)",
        ));
    }
    Ok(t)
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
/// Only a query that binds at least one `None` is marked non-persistent
/// (`.persistent(false)`); everything else keeps sqlx's normal prepared-
/// statement caching. sqlx's cache is keyed on the raw SQL text only, not on
/// the bound argument types (see `PgConnection::get_or_prepare`). Since a
/// `None` parameter is bound with `PgTypeInfo::with_oid(Oid(0))`
/// ("unspecified" - let Postgres infer it), the *first* execution of a given
/// SQL text bakes whatever type Postgres inferred into the cached prepared
/// statement; a later call to the same SQL text with a real (non-NULL) value
/// of a different wire size (e.g. our i64 for Python ints vs. an inferred
/// INT4 column) would then fail with "incorrect binary data format in bind
/// parameter" - live-verified against Postgres 16. Marking only the
/// NULL-binding calls non-persistent avoids the collision without paying a
/// fresh Parse+Describe round trip on every query. Upgrade path if a
/// collision is ever seen anyway (e.g. NULL and non-NULL calls interleaved
/// concurrently on the same SQL text): key the cache on (SQL text, argument
/// type fingerprint) instead of SQL text alone.
///
/// Beyond the primitives, the types Python users actually hold bind natively
/// (matching sqlx's own type map - see docs.rs/sqlx postgres::types):
/// `datetime.datetime` -> TIMESTAMP/TIMESTAMPTZ, `datetime.date` -> DATE,
/// `datetime.time` -> TIME, `uuid.UUID` -> UUID, `decimal.Decimal` ->
/// NUMERIC, `dict`/`list`/`tuple` -> JSON/JSONB, `bytes`/`bytearray` ->
/// BYTEA. A timezone-aware datetime binds as TIMESTAMPTZ (utc offset
/// applied), a naive one as TIMESTAMP - Postgres stores both correctly
/// without any `$1::type` cast in the SQL text.
///
/// Anything else still falls through to `str(obj)` and binds as TEXT (see
/// the last arm below) - e.g. pass `INET` values as strings with an
/// explicit `$1::inet` cast. See `tests/native_binding.py` for working
/// examples of every natively-bound type.
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
        } else if py_isinstance(py, obj_ref, &PY_DATETIME, "datetime", "datetime")? {
            match py_datetime_to_param(obj_ref)? {
                PyDateTimeParam::Timestamp(naive) => query = query.bind(naive),
                PyDateTimeParam::Timestamptz(utc) => query = query.bind(utc),
            }
        } else if py_isinstance(py, obj_ref, &PY_DATE, "datetime", "date")? {
            let d = py_date_to_chrono(obj_ref)?;
            query = query.bind(d)
        } else if py_isinstance(py, obj_ref, &PY_TIME, "datetime", "time")? {
            let t = py_time_to_chrono(obj_ref)?;
            query = query.bind(t)
        } else if py_isinstance(py, obj_ref, &PY_UUID, "uuid", "UUID")? {
            let u = obj_ref.extract::<Uuid>()?;
            query = query.bind(u)
        } else if py_isinstance(py, obj_ref, &PY_DECIMAL, "decimal", "Decimal")? {
            let d = BigDecimal::from_str(&obj_ref.str()?.extract::<String>()?)
                .map_err(|e| DataError::new_err(format!("invalid Decimal parameter: {}", e)))?;
            query = query.bind(d)
        } else if obj_ref.downcast::<PyDict>().is_ok()
            || obj_ref.downcast::<PyList>().is_ok()
            || obj_ref.downcast::<PyTuple>().is_ok()
        {
            let json = py_to_json(py, obj_ref)?;
            query = query.bind(json)
        } else if let Ok(b) = obj_ref.downcast::<PyBytes>() {
            // Owned Vec (not the &[u8] borrow): the local borrow can't
            // outlive 'q on the returned Query, and sqlx copies into the
            // argument buffer either way.
            query = query.bind(b.as_bytes().to_vec())
        } else if let Ok(b) = obj_ref.downcast::<PyByteArray>() {
            // to_vec() copies first - see the safety notes on as_bytes():
            // holding the borrow across sqlx's async encode path could race
            // a concurrent Python-side resize of the same bytearray.
            query = query.bind(b.to_vec())
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
        "BYTEA" => decode_scalar::<Vec<u8>>(py, row, idx),
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
