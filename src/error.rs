use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::PyErr;
use tokio_postgres::Error as PgError;

// Base Database Error - follows DB-API 2.0 specification (PEP 249)
create_exception!(PostPyro, DatabaseError, PyException);
create_exception!(PostPyro, InterfaceError, DatabaseError);
create_exception!(PostPyro, DataError, DatabaseError);
create_exception!(PostPyro, OperationalError, DatabaseError);
create_exception!(PostPyro, IntegrityError, DatabaseError);
create_exception!(PostPyro, InternalError, DatabaseError);
create_exception!(PostPyro, ProgrammingError, DatabaseError);
create_exception!(PostPyro, NotSupportedError, DatabaseError);

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

/// Map PostgreSQL error to appropriate Python exception
pub fn map_db_error_enhanced(error: PgError) -> PyErr {
    use std::time::Instant;
    let start_time = Instant::now();

    let (error_class, detailed_message) = analyze_postgresql_error(&error);
    let processing_time = start_time.elapsed();

    // Add performance metrics to error for debugging
    let enhanced_message = if processing_time > std::time::Duration::from_micros(100) {
        format!(
            "{} [Error processing: {:?}]",
            detailed_message, processing_time
        )
    } else {
        detailed_message
    };

    match error_class {
        PostgreSQLErrorClass::ConnectionIssue => OperationalError::new_err(enhanced_message),
        PostgreSQLErrorClass::SyntaxError => ProgrammingError::new_err(enhanced_message),
        PostgreSQLErrorClass::ConstraintViolation => IntegrityError::new_err(enhanced_message),
        PostgreSQLErrorClass::DataTypeIssue => DataError::new_err(enhanced_message),
        PostgreSQLErrorClass::InsufficientResources => OperationalError::new_err(enhanced_message),
        PostgreSQLErrorClass::SystemError => InternalError::new_err(enhanced_message),
        PostgreSQLErrorClass::UnsupportedFeature => NotSupportedError::new_err(enhanced_message),
        PostgreSQLErrorClass::GenericDatabase => DatabaseError::new_err(enhanced_message),
    }
}

/// PostgreSQL error classification
#[derive(Debug, Clone, PartialEq)]
enum PostgreSQLErrorClass {
    ConnectionIssue,
    SyntaxError,
    ConstraintViolation,
    DataTypeIssue,
    InsufficientResources,
    SystemError,
    UnsupportedFeature,
    GenericDatabase,
}

/// Analyze PostgreSQL error using SQLSTATE codes
#[inline]
fn analyze_postgresql_error(error: &PgError) -> (PostgreSQLErrorClass, String) {
    let base_message = error.to_string();

    // Extract SQLSTATE code for precise error classification
    let error_code = error.code();
    let error_class = if let Some(code) = error_code {
        match code.code() {
            // Connection exceptions (08xxx)
            code if code.starts_with("08") => PostgreSQLErrorClass::ConnectionIssue,

            // Syntax error or access rule violation (42xxx)
            code if code.starts_with("42") => PostgreSQLErrorClass::SyntaxError,

            // Integrity constraint violation (23xxx)
            code if code.starts_with("23") => PostgreSQLErrorClass::ConstraintViolation,

            // Invalid data type (22xxx)
            code if code.starts_with("22") => PostgreSQLErrorClass::DataTypeIssue,

            // Insufficient resources (53xxx, 54xxx)
            code if code.starts_with("53") || code.starts_with("54") => {
                PostgreSQLErrorClass::InsufficientResources
            }

            // System error (58xxx, XX000)
            code if code.starts_with("58") || code == "XX000" => PostgreSQLErrorClass::SystemError,

            // Feature not supported (0Axxx)
            code if code.starts_with("0A") => PostgreSQLErrorClass::UnsupportedFeature,

            _ => PostgreSQLErrorClass::GenericDatabase,
        }
    } else {
        PostgreSQLErrorClass::GenericDatabase
    };

    // Enhanced message with context
    let detailed_message = if let Some(code) = error_code {
        let severity = get_error_severity(&error_class);
        let suggestion = get_error_suggestion(&error_class, code.code());
        format!(
            "[{}] {} (SQLSTATE: {}){}",
            severity,
            base_message,
            code.code(),
            if !suggestion.is_empty() {
                format!("\nSuggestion: {}", suggestion)
            } else {
                String::new()
            }
        )
    } else {
        base_message
    };

    (error_class, detailed_message)
}

/// Get human-readable severity level
fn get_error_severity(class: &PostgreSQLErrorClass) -> &'static str {
    match class {
        PostgreSQLErrorClass::SystemError | PostgreSQLErrorClass::InsufficientResources => {
            "CRITICAL"
        }
        PostgreSQLErrorClass::ConnectionIssue => "ERROR",
        PostgreSQLErrorClass::ConstraintViolation | PostgreSQLErrorClass::SyntaxError => "ERROR",
        PostgreSQLErrorClass::DataTypeIssue => "WARNING",
        PostgreSQLErrorClass::UnsupportedFeature => "INFO",
        PostgreSQLErrorClass::GenericDatabase => "ERROR",
    }
}

/// Provide contextual suggestions for error resolution
fn get_error_suggestion(class: &PostgreSQLErrorClass, sqlstate: &str) -> String {
    match class {
        PostgreSQLErrorClass::ConnectionIssue => {
            "Check network connectivity, server status, and connection parameters".to_string()
        }
        PostgreSQLErrorClass::SyntaxError => {
            "Verify SQL syntax, table/column names, and parameter placeholders".to_string()
        }
        PostgreSQLErrorClass::ConstraintViolation => match sqlstate {
            "23505" => "Duplicate key violation - ensure unique values".to_string(),
            "23503" => "Foreign key constraint violation - check referenced values".to_string(),
            "23502" => "NOT NULL constraint violation - provide required values".to_string(),
            "23514" => "CHECK constraint violation - verify data meets constraints".to_string(),
            _ => "Check data integrity constraints".to_string(),
        },
        PostgreSQLErrorClass::DataTypeIssue => {
            "Verify data types and format - check parameter types and values".to_string()
        }
        PostgreSQLErrorClass::InsufficientResources => {
            "Database server resources exhausted - contact administrator".to_string()
        }
        PostgreSQLErrorClass::SystemError => {
            "Internal database error - check server logs and contact administrator".to_string()
        }
        PostgreSQLErrorClass::UnsupportedFeature => {
            "Feature not available in this PostgreSQL version".to_string()
        }
        PostgreSQLErrorClass::GenericDatabase => {
            "Check query and database configuration".to_string()
        }
    }
}

/// Original simple mapping function for backwards compatibility
#[allow(dead_code)]
fn map_db_error_simple(error: PgError) -> PyErr {
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
