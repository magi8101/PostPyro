use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::PyErr;
use tokio_postgres::Error as PgError;

// Base Database Error - follows DB-API 2.0 specification (PEP 249)
create_exception!(pypg_driver, DatabaseError, PyException);
create_exception!(pypg_driver, InterfaceError, DatabaseError);
create_exception!(pypg_driver, DataError, DatabaseError);
create_exception!(pypg_driver, OperationalError, DatabaseError);
create_exception!(pypg_driver, IntegrityError, DatabaseError);
create_exception!(pypg_driver, InternalError, DatabaseError);
create_exception!(pypg_driver, ProgrammingError, DatabaseError);
create_exception!(pypg_driver, NotSupportedError, DatabaseError);

/// Map PostgreSQL errors to appropriate Python DB-API 2.0 exceptions
pub fn map_db_error(error: PgError) -> PyErr {
    map_db_error_enhanced(error)
}

/// Create a type conversion error for when Python types can't be converted to PostgreSQL types
pub fn type_conversion_error(expected: &str, actual: &str) -> PyErr {
    DataError::new_err(format!(
        "Type conversion error: expected {}, got {}",
        expected, actual
    ))
}

/// Create an error for invalid connection strings
pub fn invalid_connection_string_error(details: &str) -> PyErr {
    InterfaceError::new_err(format!("Invalid connection string: {}", details))
}

/// Create an error for when a connection is closed but an operation is attempted
pub fn connection_closed_error() -> PyErr {
    InterfaceError::new_err("Connection is closed")
}

/// Create an error for when a transaction is completed but operations are attempted
pub fn transaction_completed_error() -> PyErr {
    ProgrammingError::new_err("Transaction is already committed or rolled back")
}

/// Create an error for unsupported operations
pub fn not_supported_error(feature: &str) -> PyErr {
    NotSupportedError::new_err(format!("Feature not supported: {}", feature))
}

/// Enhanced error mapping based on PostgreSQL error codes
pub fn map_db_error_enhanced(error: PgError) -> PyErr {
    use tokio_postgres::error::SqlState;

    // Try to get the SQL state code for more specific error mapping
    if let Some(db_error) = error.as_db_error() {
        match db_error.code() {
            // Constraint violation errors
            &SqlState::UNIQUE_VIOLATION
            | &SqlState::FOREIGN_KEY_VIOLATION
            | &SqlState::CHECK_VIOLATION
            | &SqlState::NOT_NULL_VIOLATION => {
                IntegrityError::new_err(format!("Constraint violation: {}", error))
            }

            // Syntax errors and invalid names
            &SqlState::SYNTAX_ERROR
            | &SqlState::UNDEFINED_COLUMN
            | &SqlState::UNDEFINED_TABLE
            | &SqlState::UNDEFINED_FUNCTION => {
                ProgrammingError::new_err(format!("SQL error: {}", error))
            }

            // Data type errors
            &SqlState::INVALID_TEXT_REPRESENTATION
            | &SqlState::NUMERIC_VALUE_OUT_OF_RANGE
            | &SqlState::DATETIME_FIELD_OVERFLOW => {
                DataError::new_err(format!("Data conversion error: {}", error))
            }

            // Connection and operational errors
            &SqlState::CONNECTION_EXCEPTION
            | &SqlState::CONNECTION_DOES_NOT_EXIST
            | &SqlState::CONNECTION_FAILURE => {
                OperationalError::new_err(format!("Connection error: {}", error))
            }

            // Internal PostgreSQL errors
            &SqlState::INTERNAL_ERROR | &SqlState::DATA_CORRUPTED => {
                InternalError::new_err(format!("Internal database error: {}", error))
            }

            // Feature not supported
            &SqlState::FEATURE_NOT_SUPPORTED => {
                NotSupportedError::new_err(format!("Feature not supported: {}", error))
            }

            // Default to DatabaseError for unmapped codes
            _ => DatabaseError::new_err(format!("Database error: {}", error)),
        }
    } else {
        // Non-database errors (connection issues, etc.)
        OperationalError::new_err(format!("Operational error: {}", error))
    }
}
