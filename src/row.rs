use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList};
use sqlx::postgres::PgRow;
use sqlx::{Column, Row as SqlxRow};

use crate::types::pg_value_to_py;

/// A single result row with real name- and index-based column access.
#[pyclass(frozen)]
pub struct Row {
    names: Vec<String>,
    values: Vec<PyObject>,
}

#[pymethods]
impl Row {
    fn __len__(&self) -> usize {
        self.values.len()
    }

    fn __getitem__(&self, py: Python, key: &PyAny) -> PyResult<PyObject> {
        // `bool` is a subclass of `int` in Python, so `key.extract::<usize>()`
        // would silently accept `row[True]` as `row[1]`. Reject it explicitly
        // rather than let it fall through to whichever branch happens to
        // extract it.
        if key.is_instance_of::<PyBool>() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Row indices must be an int or str, not bool",
            ));
        }
        if let Ok(idx) = key.extract::<usize>() {
            self.values
                .get(idx)
                .map(|v| v.clone_ref(py))
                .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("Row index out of range"))
        } else if let Ok(name) = key.extract::<&str>() {
            self.get_by_name(py, name)
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!("Column '{}' not found", name)))
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "Row indices must be an int or str",
            ))
        }
    }

    fn __iter__(&self, py: Python) -> PyResult<PyObject> {
        let list = PyList::new(py, self.values.iter().map(|v| v.clone_ref(py)));
        list.call_method0("__iter__").map(|it| it.to_object(py))
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let mut parts = Vec::with_capacity(self.names.len());
        for (name, value) in self.names.iter().zip(self.values.iter()) {
            parts.push(format!("{}={}", name, value.as_ref(py).repr()?));
        }
        Ok(format!("Row({})", parts.join(", ")))
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, py: Python, key: &PyAny, default: Option<PyObject>) -> PyResult<PyObject> {
        if key.is_instance_of::<PyBool>() {
            return Err(pyo3::exceptions::PyTypeError::new_err("key must be an int or str, not bool"));
        }
        let found = if let Ok(idx) = key.extract::<usize>() {
            self.values.get(idx).map(|v| v.clone_ref(py))
        } else if let Ok(name) = key.extract::<&str>() {
            self.get_by_name(py, name)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err("key must be an int or str"));
        };
        Ok(found.unwrap_or_else(|| default.unwrap_or_else(|| py.None())))
    }

    fn keys(&self) -> Vec<String> {
        self.names.clone()
    }

    fn values(&self, py: Python) -> Vec<PyObject> {
        self.values.iter().map(|v| v.clone_ref(py)).collect()
    }

    fn items(&self, py: Python) -> Vec<(String, PyObject)> {
        self.names
            .iter()
            .cloned()
            .zip(self.values.iter().map(|v| v.clone_ref(py)))
            .collect()
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (name, value) in self.names.iter().zip(self.values.iter()) {
            dict.set_item(name, value.clone_ref(py))?;
        }
        Ok(dict.to_object(py))
    }
}

impl Row {
    fn get_by_name(&self, py: Python, name: &str) -> Option<PyObject> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|idx| self.values[idx].clone_ref(py))
    }

    pub fn from_pg_row(py: Python, row: &PgRow) -> PyResult<Self> {
        let columns = row.columns();
        let mut names = Vec::with_capacity(columns.len());
        let mut values = Vec::with_capacity(columns.len());
        for (idx, column) in columns.iter().enumerate() {
            names.push(column.name().to_string());
            values.push(pg_value_to_py(py, row, idx)?);
        }
        Ok(Row { names, values })
    }
}
