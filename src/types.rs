use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use postgres_types::{ToSql, Type};
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString};
use pyo3::{PyObject, PyResult, Python, ToPyObject};
use serde_json::Value as JsonValue;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::error::{type_conversion_error, DataError};

/// Enum representing all PostgreSQL types we support for conversion
#[derive(Debug, Clone)]
pub enum PostgresValue {
    Bool(bool),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    String(String),
    Bytes(Vec<u8>),
    Date(NaiveDate),
    Time(NaiveTime),
    Timestamp(NaiveDateTime),
    TimestampTz(DateTime<Utc>),
    Uuid(Uuid),
    Json(JsonValue),
    Array(Vec<PostgresValue>),
    Null,
}

impl ToSql for PostgresValue {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            PostgresValue::Bool(v) => v.to_sql(ty, out),
            PostgresValue::Int16(v) => v.to_sql(ty, out),
            PostgresValue::Int32(v) => v.to_sql(ty, out),
            PostgresValue::Int64(v) => v.to_sql(ty, out),
            PostgresValue::Float32(v) => v.to_sql(ty, out),
            PostgresValue::Float64(v) => v.to_sql(ty, out),
            PostgresValue::String(v) => v.to_sql(ty, out),
            PostgresValue::Bytes(v) => v.to_sql(ty, out),
            PostgresValue::Date(v) => v.to_sql(ty, out),
            PostgresValue::Time(v) => v.to_sql(ty, out),
            PostgresValue::Timestamp(v) => v.to_sql(ty, out),
            PostgresValue::TimestampTz(v) => v.to_sql(ty, out),
            PostgresValue::Uuid(v) => v.to_sql(ty, out),
            PostgresValue::Json(v) => v.to_sql(ty, out),
            PostgresValue::Array(arr) => {
                // Implement basic array serialization for homogeneous arrays
                if arr.is_empty() {
                    return Ok(postgres_types::IsNull::Yes);
                }

                // For now, serialize arrays as JSON strings
                // This is a fallback - proper array support would need type-specific handling
                let json_array = serde_json::to_value(
                    arr.iter()
                        .map(|v| {
                            match v {
                                PostgresValue::Bool(b) => serde_json::Value::Bool(*b),
                                PostgresValue::Int32(i) => {
                                    serde_json::Value::Number(serde_json::Number::from(*i))
                                }
                                PostgresValue::Int64(i) => {
                                    serde_json::Value::Number(serde_json::Number::from(*i))
                                }
                                PostgresValue::Float64(f) => serde_json::Number::from_f64(*f)
                                    .map(serde_json::Value::Number)
                                    .unwrap_or(serde_json::Value::Null),
                                PostgresValue::String(s) => serde_json::Value::String(s.clone()),
                                PostgresValue::Null => serde_json::Value::Null,
                                _ => serde_json::Value::String(format!("{:?}", v)), // Fallback for complex types
                            }
                        })
                        .collect::<Vec<_>>(),
                )
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Sync + Send>)?;

                json_array.to_sql(ty, out)
            }
            PostgresValue::Null => Ok(postgres_types::IsNull::Yes),
        }
    }

    fn accepts(ty: &Type) -> bool {
        match ty {
            &Type::BOOL => true,
            &Type::INT2 => true,
            &Type::INT4 => true,
            &Type::INT8 => true,
            &Type::FLOAT4 => true,
            &Type::FLOAT8 => true,
            &Type::VARCHAR | &Type::TEXT | &Type::BPCHAR => true,
            &Type::BYTEA => true,
            &Type::DATE => true,
            &Type::TIME => true,
            &Type::TIMESTAMP => true,
            &Type::TIMESTAMPTZ => true,
            &Type::UUID => true,
            &Type::JSON | &Type::JSONB => true,
            _ if ty.name().ends_with("[]") => true, // Accept array types
            _ => false,                             // Other types not handled
        }
    }

    postgres_types::to_sql_checked!();
}

/// Convert Python object to PostgreSQL value
pub fn py_to_postgres_value(py: Python, obj: &PyObject) -> PyResult<PostgresValue> {
    // Handle None/NULL first
    if obj.is_none(py) {
        return Ok(PostgresValue::Null);
    }

    // Try different Python types
    if let Ok(b) = obj.downcast::<PyBool>(py) {
        return Ok(PostgresValue::Bool(b.is_true()));
    }

    if let Ok(i) = obj.downcast::<PyInt>(py) {
        let value = i.extract::<i64>()?;
        // Try to fit into smaller types if possible
        if value >= i16::MIN as i64 && value <= i16::MAX as i64 {
            Ok(PostgresValue::Int16(value as i16))
        } else if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            Ok(PostgresValue::Int32(value as i32))
        } else {
            Ok(PostgresValue::Int64(value))
        }
    } else if let Ok(f) = obj.downcast::<PyFloat>(py) {
        let value = f.value();
        Ok(PostgresValue::Float64(value))
    } else if let Ok(s) = obj.downcast::<PyString>(py) {
        let value = s.extract::<String>()?;
        Ok(PostgresValue::String(value))
    } else if let Ok(b) = obj.downcast::<PyBytes>(py) {
        let value = b.as_bytes().to_vec();
        Ok(PostgresValue::Bytes(value))
    } else if obj.as_ref(py).get_type().name()? == "datetime" {
        // Handle datetime objects
        let timestamp = obj.call_method0(py, "timestamp")?;
        let timestamp_float: f64 = timestamp.extract(py)?;
        let datetime = DateTime::from_timestamp(timestamp_float as i64, 0)
            .ok_or_else(|| type_conversion_error("valid timestamp", "datetime"))?;
        Ok(PostgresValue::TimestampTz(datetime))
    } else if let Ok(d) = obj.downcast::<PyDict>(py) {
        // Convert dict to JSON
        let json_module = py.import("json")?;
        let json_str = json_module.call_method1("dumps", (d,))?;
        let json_str = json_str.extract::<String>()?;
        let json_value: JsonValue = serde_json::from_str(&json_str)
            .map_err(|_| type_conversion_error("valid JSON", "dict"))?;
        Ok(PostgresValue::Json(json_value))
    } else if let Ok(l) = obj.downcast::<PyList>(py) {
        // Handle arrays
        let mut array_values = Vec::new();
        for item in l.iter() {
            array_values.push(py_to_postgres_value(py, &item.into())?);
        }
        Ok(PostgresValue::Array(array_values))
    } else {
        // Try to extract as string for unknown types
        if let Ok(s) = obj.as_ref(py).str() {
            let value = s.extract::<String>()?;
            Ok(PostgresValue::String(value))
        } else {
            Err(type_conversion_error(
                "supported PostgreSQL type",
                &obj.as_ref(py).get_type().name()?,
            ))
        }
    }
}

/// Convert PostgreSQL row value to Python object
pub fn postgres_to_py(
    py: Python,
    row: &Row,
    column_idx: usize,
    col_type: &Type,
) -> PyResult<PyObject> {
    match col_type {
        &Type::BOOL => {
            let value: bool = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(value.to_object(py))
        }
        &Type::INT2 => {
            let value: i16 = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(value.to_object(py))
        }
        &Type::INT4 => {
            let value: i32 = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(value.to_object(py))
        }
        &Type::INT8 => {
            let value: i64 = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(value.to_object(py))
        }
        &Type::FLOAT4 => {
            let value: f32 = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(value.to_object(py))
        }
        &Type::FLOAT8 => {
            let value: f64 = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(value.to_object(py))
        }
        &Type::VARCHAR | &Type::TEXT | &Type::BPCHAR => {
            let value: String = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(value.to_object(py))
        }
        &Type::BYTEA => {
            let value: Vec<u8> = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            Ok(PyBytes::new(py, &value).to_object(py))
        }
        &Type::DATE => {
            let value: NaiveDate = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            // Convert to Python date
            let py_date = py.import("datetime")?.getattr("date")?;
            Ok(py_date
                .call((value.year(), value.month(), value.day()), None)?
                .into())
        }
        &Type::TIME => {
            let value: NaiveTime = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            // Convert to Python time
            let py_time = py.import("datetime")?.getattr("time")?;
            Ok(py_time
                .call(
                    (
                        value.hour(),
                        value.minute(),
                        value.second(),
                        value.nanosecond() / 1000,
                    ),
                    None,
                )?
                .into())
        }
        &Type::TIMESTAMP => {
            let value: NaiveDateTime = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            // Convert to Python datetime
            let py_datetime = py.import("datetime")?.getattr("datetime")?;
            Ok(py_datetime
                .call(
                    (
                        value.year(),
                        value.month(),
                        value.day(),
                        value.hour(),
                        value.minute(),
                        value.second(),
                        value.nanosecond() / 1000,
                    ),
                    None,
                )?
                .into())
        }
        &Type::TIMESTAMPTZ => {
            let value: DateTime<Utc> = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            // Convert to Python datetime with UTC timezone
            let py_datetime = py.import("datetime")?.getattr("datetime")?;
            let utc_tz = py.import("datetime")?.getattr("timezone")?.getattr("utc")?;
            Ok(py_datetime
                .call(
                    (
                        value.year(),
                        value.month(),
                        value.day(),
                        value.hour(),
                        value.minute(),
                        value.second(),
                        value.timestamp_subsec_micros(),
                        utc_tz,
                    ),
                    None,
                )?
                .into())
        }
        &Type::UUID => {
            let value: Uuid = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            // Convert to Python UUID
            let py_uuid = py.import("uuid")?.getattr("UUID")?;
            Ok(py_uuid.call((value.to_string(),), None)?.into())
        }
        &Type::JSON | &Type::JSONB => {
            let value: JsonValue = row
                .try_get(column_idx)
                .map_err(crate::error::map_db_error)?;
            // Convert JSON to Python dict/list
            let json_str = serde_json::to_string(&value)
                .map_err(|_| DataError::new_err("Failed to serialize JSON"))?;
            let json_module = py.import("json")?;
            Ok(json_module.call_method1("loads", (json_str,))?.into())
        }
        _ => {
            // For unsupported types, try to get as string
            // This handles INET, CIDR, and other types not explicitly supported
            if let Ok(value) = row.try_get::<_, String>(column_idx) {
                Ok(value.to_object(py))
            } else if let Ok(value) = row.try_get::<_, i32>(column_idx) {
                // Try as integer (for port numbers, etc.)
                Ok(value.to_object(py))
            } else if let Ok(value) = row.try_get::<_, i64>(column_idx) {
                // Try as big integer
                Ok(value.to_object(py))
            } else {
                // Last resort - return the type name as a string
                Ok(format!("Unsupported type: {}", col_type.name()).to_object(py))
            }
        }
    }
}

/// Convert a vector of Python objects to PostgreSQL values for parameterized queries
pub fn py_objects_to_postgres_values(
    py: Python,
    params: &[PyObject],
) -> PyResult<Vec<PostgresValue>> {
    params
        .iter()
        .map(|obj| py_to_postgres_value(py, obj))
        .collect()
}
