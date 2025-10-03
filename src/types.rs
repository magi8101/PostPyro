use compact_str::CompactString;
use lru::LruCache;
use once_cell::sync::Lazy;
use postgres_types::ToSql;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString};
use pyo3::{IntoPy, PyObject, PyResult, Python};
use smallvec::SmallVec;
use std::sync::Mutex;

// String cache for common database values
static STRING_CACHE: Lazy<Mutex<LruCache<String, CompactString>>> =
    Lazy::new(|| Mutex::new(LruCache::new(std::num::NonZeroUsize::new(1000).unwrap())));

/// Intern frequently used strings for memory efficiency
fn intern_string(s: String) -> CompactString {
    let mut cache = STRING_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(&s) {
        cached.clone()
    } else {
        let compact = CompactString::new(&s);
        cache.put(s, compact.clone());
        compact
    }
}

/// High-performance PostgreSQL value type with proper binary protocol support
#[derive(Debug, Clone)]
pub enum PostgresValue {
    Null,
    Bool(bool),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    String(String),
}

impl ToSql for PostgresValue {
    fn to_sql(
        &self,
        ty: &postgres_types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            PostgresValue::Null => Ok(postgres_types::IsNull::Yes),
            PostgresValue::Bool(v) => v.to_sql(ty, out),
            PostgresValue::Int16(v) => v.to_sql(ty, out),
            PostgresValue::Int32(v) => v.to_sql(ty, out),
            PostgresValue::Int64(v) => v.to_sql(ty, out),
            PostgresValue::Float32(v) => v.to_sql(ty, out),
            PostgresValue::Float64(v) => v.to_sql(ty, out),
            PostgresValue::String(v) => v.to_sql(ty, out),
        }
    }

    fn accepts(ty: &postgres_types::Type) -> bool {
        matches!(
            *ty,
            postgres_types::Type::BOOL
                | postgres_types::Type::INT2
                | postgres_types::Type::INT4
                | postgres_types::Type::INT8
                | postgres_types::Type::FLOAT4
                | postgres_types::Type::FLOAT8
                | postgres_types::Type::NUMERIC
                | postgres_types::Type::TEXT
                | postgres_types::Type::VARCHAR
                | postgres_types::Type::CHAR
                | postgres_types::Type::BPCHAR
        )
    }

    postgres_types::to_sql_checked!();
}

/// Convert Python object to PostgresValue with proper type handling
pub fn py_to_postgres_value(py: Python, obj: &PyObject) -> PyResult<PostgresValue> {
    let obj_ref = obj.as_ref(py);

    // Fast path: check None first
    if obj.is_none(py) {
        return Ok(PostgresValue::Null);
    }

    // Booleans - use native bool type
    if let Ok(b) = obj_ref.downcast::<PyBool>() {
        return Ok(PostgresValue::Bool(b.extract()?));
    }

    // Integers - use appropriate size
    if let Ok(i) = obj_ref.downcast::<PyInt>() {
        let val = i.extract::<i64>()?;
        return if val >= i16::MIN as i64 && val <= i16::MAX as i64 {
            Ok(PostgresValue::Int16(val as i16))
        } else if val >= i32::MIN as i64 && val <= i32::MAX as i64 {
            Ok(PostgresValue::Int32(val as i32))
        } else {
            Ok(PostgresValue::Int64(val))
        };
    }

    // Floats - use native float types
    if let Ok(f) = obj_ref.downcast::<PyFloat>() {
        let val = f.value();
        // Use f32 if precision allows, otherwise f64
        return Ok(PostgresValue::Float64(val));
    }

    // Strings
    if let Ok(s) = obj_ref.downcast::<PyString>() {
        return Ok(PostgresValue::String(s.extract()?));
    }

    // Fallback: convert to string representation
    let s = obj_ref.str()?.extract::<String>()?;
    Ok(PostgresValue::String(s))
}

/// High-performance PostgreSQL to Python conversion with type specialization
pub fn postgres_to_py(
    py: Python,
    row: &tokio_postgres::Row,
    idx: usize,
    col_type: &postgres_types::Type,
) -> PyResult<PyObject> {
    // Type-specialized conversion for performance
    match *col_type {
        postgres_types::Type::INT2 => match row.try_get::<_, Option<i16>>(idx) {
            Ok(Some(i)) => Ok(i.into_py(py)),
            Ok(None) => Ok(py.None()),
            Err(_) => Ok(py.None()),
        },
        postgres_types::Type::INT4 => match row.try_get::<_, Option<i32>>(idx) {
            Ok(Some(i)) => Ok(i.into_py(py)),
            Ok(None) => Ok(py.None()),
            Err(_) => Ok(py.None()),
        },
        postgres_types::Type::INT8 => match row.try_get::<_, Option<i64>>(idx) {
            Ok(Some(i)) => Ok(i.into_py(py)),
            Ok(None) => Ok(py.None()),
            Err(_) => Ok(py.None()),
        },
        postgres_types::Type::FLOAT4 => match row.try_get::<_, Option<f32>>(idx) {
            Ok(Some(f)) => Ok(f.into_py(py)),
            Ok(None) => Ok(py.None()),
            Err(_) => Ok(py.None()),
        },
        postgres_types::Type::FLOAT8 => match row.try_get::<_, Option<f64>>(idx) {
            Ok(Some(f)) => Ok(f.into_py(py)),
            Ok(None) => Ok(py.None()),
            Err(_) => Ok(py.None()),
        },
        postgres_types::Type::BOOL => match row.try_get::<_, Option<bool>>(idx) {
            Ok(Some(b)) => Ok(b.into_py(py)),
            Ok(None) => Ok(py.None()),
            Err(_) => Ok(py.None()),
        },
        postgres_types::Type::TEXT
        | postgres_types::Type::VARCHAR
        | postgres_types::Type::CHAR
        | postgres_types::Type::BPCHAR => match row.try_get::<_, Option<String>>(idx) {
            Ok(Some(s)) => {
                let interned = intern_string(s);
                Ok(interned.as_str().into_py(py))
            }
            Ok(None) => Ok(py.None()),
            Err(_) => Ok(py.None()),
        },
        _ => {
            // Generic fallback for other types
            if let Ok(Some(s)) = row.try_get::<_, Option<String>>(idx) {
                Ok(s.into_py(py))
            } else {
                Ok(py.None())
            }
        }
    }
}

/// Convert Python objects to Box<dyn ToSql> with proper type handling
pub fn py_objects_to_postgres_values(
    py: Python,
    objects: &[PyObject],
) -> PyResult<Vec<Box<dyn postgres_types::ToSql + Sync + Send>>> {
    let mut values: Vec<Box<dyn postgres_types::ToSql + Sync + Send>> =
        Vec::with_capacity(objects.len());

    for obj in objects {
        let obj_ref = obj.as_ref(py);

        if obj.is_none(py) {
            values.push(Box::new(None::<String>));
        } else if let Ok(b) = obj_ref.downcast::<PyBool>() {
            // Use native boolean type
            let bool_val: bool = b.extract()?;
            values.push(Box::new(bool_val));
        } else if let Ok(i) = obj_ref.downcast::<PyInt>() {
            // Use appropriate native integer type
            let val = i.extract::<i64>()?;
            if val >= i32::MIN as i64 && val <= i32::MAX as i64 {
                values.push(Box::new(val as i32));
            } else {
                values.push(Box::new(val));
            }
        } else if let Ok(f) = obj_ref.downcast::<PyFloat>() {
            // Use f32 for PostgreSQL REAL type compatibility
            let val = f.value() as f32;
            values.push(Box::new(val));
        } else if let Ok(s) = obj_ref.downcast::<PyString>() {
            let s: String = s.extract()?;
            values.push(Box::new(s));
        } else {
            let s = obj_ref.str()?.extract::<String>()?;
            values.push(Box::new(s));
        }
    }
    Ok(values)
}

/// High-performance batch conversion using SmallVec
pub fn py_objects_to_postgres_values_fast(
    py: Python,
    objects: &[PyObject],
) -> PyResult<SmallVec<[PostgresValue; 8]>> {
    let mut values = SmallVec::with_capacity(objects.len());
    for obj in objects {
        values.push(py_to_postgres_value(py, obj)?);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_value_sizes() {
        let size = std::mem::size_of::<PostgresValue>();
        println!("PostgresValue size: {} bytes", size);
        assert!(size <= 32);
    }

    #[test]
    fn test_bool_conversion() {
        let bool_true = PostgresValue::Bool(true);
        let bool_false = PostgresValue::Bool(false);

        match bool_true {
            PostgresValue::Bool(b) => assert_eq!(b, true),
            _ => panic!("Expected Bool variant"),
        }

        match bool_false {
            PostgresValue::Bool(b) => assert_eq!(b, false),
            _ => panic!("Expected Bool variant"),
        }
    }
}
