use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString};
use sqlx::postgres::PgRow;
use sqlx::{Column, Postgres, Row as SqlxRow, TypeInfo};

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
pub fn bind_params<'q>(
    mut query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    py: Python,
    params: &[PyObject],
) -> PyResult<sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>> {
    for obj in params {
        let obj_ref = obj.as_ref(py);
        query = if obj.is_none(py) {
            query.bind(None::<String>)
        } else if let Ok(b) = obj_ref.downcast::<PyBool>() {
            query.bind(b.extract::<bool>()?)
        } else if let Ok(i) = obj_ref.downcast::<PyInt>() {
            query.bind(i.extract::<i64>()?)
        } else if let Ok(f) = obj_ref.downcast::<PyFloat>() {
            query.bind(f.extract::<f64>()?)
        } else if let Ok(s) = obj_ref.downcast::<PyString>() {
            query.bind(s.extract::<String>()?)
        } else {
            let s = obj_ref.str()?.extract::<String>()?;
            query.bind(s)
        };
    }
    Ok(query)
}

/// Convert one PgRow column to a Python object, type-specialized for the
/// common scalar types. Anything else falls back to a string
/// representation rather than failing the whole row.
pub fn pg_value_to_py(py: Python, row: &PgRow, idx: usize) -> PyResult<PyObject> {
    let type_name = row.column(idx).type_info().name();
    let value = match type_name {
        "BOOL" => row.try_get::<Option<bool>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "INT2" => row.try_get::<Option<i16>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "INT4" => row.try_get::<Option<i32>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "INT8" => row.try_get::<Option<i64>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "FLOAT4" => row.try_get::<Option<f32>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "FLOAT8" => row.try_get::<Option<f64>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
        "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" => {
            row.try_get::<Option<String>, _>(idx).ok().flatten().map(|v| v.into_py(py))
        }
        _ => row.try_get::<Option<String>, _>(idx).ok().flatten().map(|v| v.into_py(py)),
    };
    Ok(value.unwrap_or_else(|| py.None()))
}
