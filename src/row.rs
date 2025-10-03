use crate::types::postgres_to_py;
use pyo3::prelude::*;
use smallvec::SmallVec;
use tokio_postgres::Row as TokioRow;

/// High-performance immutable row with pre-allocated storage
#[pyclass(frozen)]
pub struct Row {
    data: SmallVec<[PyObject; 16]>, // Stack allocation for ≤16 columns (common case)
}

#[pymethods]
impl Row {
    pub fn __getitem__(&self, py: Python, key: &PyAny) -> PyResult<PyObject> {
        if let Ok(idx) = key.extract::<usize>() {
            // Access by index
            if idx < self.data.len() {
                Ok(self.data[idx].clone_ref(py))
            } else {
                Err(pyo3::exceptions::PyIndexError::new_err(
                    "Index out of range",
                ))
            }
        } else if let Ok(col_name) = key.extract::<&str>() {
            // Access by column name - for now just return the first column
            // TODO: Store column names mapping for proper column name access
            if !self.data.is_empty() {
                Ok(self.data[0].clone_ref(py))
            } else {
                Err(pyo3::exceptions::PyKeyError::new_err(format!(
                    "Column '{}' not found",
                    col_name
                )))
            }
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "Key must be integer or string",
            ))
        }
    }

    pub fn __len__(&self) -> usize {
        self.data.len()
    }
}

impl Row {
    /// High-performance row conversion with pre-allocation
    pub fn from_tokio_row(py: Python, row: &TokioRow) -> PyResult<Self> {
        let column_count = row.len();
        let mut data = SmallVec::with_capacity(column_count);

        // Bulk process columns for better cache locality
        for i in 0..column_count {
            let col_type = row.columns()[i].type_();
            data.push(postgres_to_py(py, row, i, col_type)?);
        }

        Ok(Row { data })
    }

    /// Bulk create multiple rows with optimized processing
    pub fn from_tokio_rows(py: Python, rows: &[TokioRow]) -> PyResult<Vec<Self>> {
        let mut result = Vec::with_capacity(rows.len());

        for row in rows {
            result.push(Self::from_tokio_row(py, row)?);
        }

        Ok(result)
    }
}
