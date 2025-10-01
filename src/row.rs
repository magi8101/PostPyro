use std::collections::HashMap;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};
use tokio_postgres::Row as TokioRow;

use crate::types::postgres_to_py;

/// Represents a single row from a query result
#[pyclass]
pub struct Row {
    /// Column values as Python objects
    values: Vec<PyObject>,
    /// Column names for name-based access
    column_names: Vec<String>,
    /// Mapping from column name to index for fast lookup
    name_to_index: HashMap<String, usize>,
}

#[pymethods]
impl Row {
    /// Get the number of columns in this row
    fn __len__(&self) -> usize {
        self.values.len()
    }

    /// Get a column value by index or name
    ///
    /// Args:
    ///     key: Column index (int) or column name (str)
    ///
    /// Returns:
    ///     Column value
    ///
    /// Raises:
    ///     IndexError: If index is out of range
    ///     KeyError: If column name doesn't exist
    fn __getitem__(&self, py: Python, key: PyObject) -> PyResult<PyObject> {
        if let Ok(index) = key.downcast::<pyo3::types::PyInt>(py) {
            let idx = index.extract::<usize>()?;
            self.get_by_index(idx)
        } else if let Ok(name) = key.downcast::<PyString>(py) {
            let name_str = name.extract::<String>()?;
            self.get_by_name(&name_str)
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "Row index must be int or str"
            ))
        }
    }

    /// Get a column value with a default if not found
    ///
    /// Args:
    ///     key: Column index (int) or column name (str)
    ///     default: Default value to return if key not found
    ///
    /// Returns:
    ///     Column value or default
    #[pyo3(signature = (key, default=None))]
    fn get(&self, py: Python, key: PyObject, default: Option<PyObject>) -> PyResult<PyObject> {
        match self.__getitem__(py, key) {
            Ok(value) => Ok(value),
            Err(_) => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    /// Get all column names
    ///
    /// Returns:
    ///     list: List of column names
    fn keys(&self, py: Python) -> PyResult<PyObject> {
        let list = PyList::new(py, self.column_names.iter().map(|s| s.to_object(py)));
        Ok(list.to_object(py))
    }

    /// Get all column values
    ///
    /// Returns:
    ///     list: List of column values
    fn values(&self, py: Python) -> PyResult<PyObject> {
        let list = PyList::new(py, &self.values);
        Ok(list.to_object(py))
    }

    /// Get all column name-value pairs
    ///
    /// Returns:
    ///     list: List of (name, value) tuples
    fn items(&self, py: Python) -> PyResult<PyObject> {
        let list = PyList::empty(py);
        for (name, value) in self.column_names.iter().zip(&self.values) {
            let tuple = PyTuple::new(py, &[name.to_object(py), value.clone()]);
            list.append(tuple)?;
        }
        Ok(list.to_object(py))
    }

    /// Iterate over column values
    fn __iter__(&self, py: Python) -> PyResult<PyObject> {
        self.values(py)
    }

    /// String representation for debugging
    fn __repr__(&self, py: Python) -> PyResult<String> {
        let mut repr = "Row(".to_string();
        for (i, (name, value)) in self.column_names.iter().zip(&self.values).enumerate() {
            if i > 0 {
                repr.push_str(", ");
            }
            let value_repr = value.as_ref(py).repr()?.extract::<String>()?;
            repr.push_str(&format!("{}={}", name, value_repr));
        }
        repr.push(')');
        Ok(repr)
    }

    /// Convert row to a Python dictionary
    ///
    /// Returns:
    ///     dict: Dictionary mapping column names to values
    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (name, value) in self.column_names.iter().zip(&self.values) {
            dict.set_item(name, value)?;
        }
        Ok(dict.to_object(py))
    }
}

impl Row {
    /// Create a Row from a tokio_postgres::Row
    ///
    /// This extracts all data immediately to avoid lifetime issues
    pub fn from_tokio_row(py: Python, row: &TokioRow) -> PyResult<Py<Self>> {
        let mut values = Vec::new();
        let mut column_names = Vec::new();
        let mut name_to_index = HashMap::new();

        let columns = row.columns();
        for (idx, column) in columns.iter().enumerate() {
            let column_name = column.name().to_string();
            column_names.push(column_name.clone());
            name_to_index.insert(column_name, idx);

            // Convert the PostgreSQL value to Python
            let py_value = postgres_to_py(py, row, idx, column.type_())?;
            values.push(py_value);
        }

        Ok(Py::new(py, Self {
            values,
            column_names,
            name_to_index,
        })?)
    }

    /// Get value by column index
    fn get_by_index(&self, index: usize) -> PyResult<PyObject> {
        self.values.get(index).cloned().ok_or_else(|| {
            pyo3::exceptions::PyIndexError::new_err(format!(
                "Column index {} is out of range (0..{})",
                index,
                self.values.len()
            ))
        })
    }

    /// Get value by column name (case-insensitive)
    fn get_by_name(&self, name: &str) -> PyResult<PyObject> {
        // First try exact match
        if let Some(&index) = self.name_to_index.get(name) {
            return self.get_by_index(index);
        }

        // Try case-insensitive match (PostgreSQL lowercases unquoted identifiers)
        let name_lower = name.to_lowercase();
        for (col_name, &index) in &self.name_to_index {
            if col_name.to_lowercase() == name_lower {
                return self.get_by_index(index);
            }
        }

        Err(pyo3::exceptions::PyKeyError::new_err(format!(
            "Column '{}' not found. Available columns: {:?}",
            name, self.column_names
        )))
    }
}