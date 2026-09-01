use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::PyErr;

// Base Database Error - follows DB-API 2.0 specification (PEP 249)
create_exception!(PostPyro, DatabaseError, PyException);
create_exception!(PostPyro, InterfaceError, DatabaseError);
create_exception!(PostPyro, DataError, DatabaseError);
create_exception!(PostPyro, OperationalError, DatabaseError);
create_exception!(PostPyro, IntegrityError, DatabaseError);
create_exception!(PostPyro, InternalError, DatabaseError);
create_exception!(PostPyro, ProgrammingError, DatabaseError);
create_exception!(PostPyro, NotSupportedError, DatabaseError);

pub fn type_conversion_error(expected: &str, actual: &str) -> PyErr {
    DataError::new_err(format!(
        "Type conversion error: expected {}, got {}",
        expected, actual
    ))
}

pub fn invalid_connection_string_error(details: &str) -> PyErr {
    InterfaceError::new_err(format!("Invalid connection string: {}", details))
}

pub fn transaction_completed_error() -> PyErr {
    ProgrammingError::new_err("Transaction is already committed or rolled back")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Classify a PostgreSQL SQLSTATE code into an error category. Pure
/// function so it's unit-testable without a live connection or the GIL.
fn classify_sqlstate(code: &str) -> PostgreSQLErrorClass {
    match code {
        c if c.starts_with("08") => PostgreSQLErrorClass::ConnectionIssue,
        c if c.starts_with("42") => PostgreSQLErrorClass::SyntaxError,
        c if c.starts_with("23") => PostgreSQLErrorClass::ConstraintViolation,
        c if c.starts_with("22") => PostgreSQLErrorClass::DataTypeIssue,
        c if c.starts_with("53") || c.starts_with("54") => {
            PostgreSQLErrorClass::InsufficientResources
        }
        c if c.starts_with("58") || c == "XX000" => PostgreSQLErrorClass::SystemError,
        c if c.starts_with("0A") => PostgreSQLErrorClass::UnsupportedFeature,
        _ => PostgreSQLErrorClass::GenericDatabase,
    }
}

fn suggestion_for(class: PostgreSQLErrorClass, sqlstate: &str) -> &'static str {
    match class {
        PostgreSQLErrorClass::ConnectionIssue => {
            "Check network connectivity, server status, and connection parameters"
        }
        PostgreSQLErrorClass::SyntaxError => {
            "Verify SQL syntax, table/column names, and parameter placeholders"
        }
        PostgreSQLErrorClass::ConstraintViolation => match sqlstate {
            "23505" => "Duplicate key violation - ensure unique values",
            "23503" => "Foreign key constraint violation - check referenced values",
            "23502" => "NOT NULL constraint violation - provide required values",
            "23514" => "CHECK constraint violation - verify data meets constraints",
            _ => "Check data integrity constraints",
        },
        PostgreSQLErrorClass::DataTypeIssue => {
            "Verify data types and format - check parameter types and values"
        }
        PostgreSQLErrorClass::InsufficientResources => {
            "Database server resources exhausted - contact administrator"
        }
        PostgreSQLErrorClass::SystemError => {
            "Internal database error - check server logs and contact administrator"
        }
        PostgreSQLErrorClass::UnsupportedFeature => "Feature not available in this PostgreSQL version",
        PostgreSQLErrorClass::GenericDatabase => "Check query and database configuration",
    }
}

fn map_database_error(db_err: &dyn sqlx::error::DatabaseError) -> PyErr {
    let message = db_err.message();
    let (class, enhanced) = match db_err.code() {
        Some(code) => {
            let class = classify_sqlstate(&code);
            let suggestion = suggestion_for(class, &code);
            (
                class,
                format!("{} (SQLSTATE: {})\nSuggestion: {}", message, code, suggestion),
            )
        }
        None => (PostgreSQLErrorClass::GenericDatabase, message.to_string()),
    };

    match class {
        PostgreSQLErrorClass::ConnectionIssue | PostgreSQLErrorClass::InsufficientResources => {
            OperationalError::new_err(enhanced)
        }
        PostgreSQLErrorClass::SyntaxError => ProgrammingError::new_err(enhanced),
        PostgreSQLErrorClass::ConstraintViolation => IntegrityError::new_err(enhanced),
        PostgreSQLErrorClass::DataTypeIssue => DataError::new_err(enhanced),
        PostgreSQLErrorClass::SystemError => InternalError::new_err(enhanced),
        PostgreSQLErrorClass::UnsupportedFeature => NotSupportedError::new_err(enhanced),
        PostgreSQLErrorClass::GenericDatabase => DatabaseError::new_err(enhanced),
    }
}

/// Map a sqlx error to the DB-API 2.0 exception hierarchy.
pub fn map_db_error(error: sqlx::Error) -> PyErr {
    match error {
        sqlx::Error::Database(db_err) => map_database_error(db_err.as_ref()),
        sqlx::Error::RowNotFound => {
            ProgrammingError::new_err("Query returned no rows, expected exactly one")
        }
        sqlx::Error::PoolTimedOut => {
            OperationalError::new_err("Timed out waiting for a connection from the pool")
        }
        sqlx::Error::PoolClosed => OperationalError::new_err("Connection pool is closed"),
        sqlx::Error::WorkerCrashed => InternalError::new_err("Database worker task crashed"),
        sqlx::Error::Io(e) => OperationalError::new_err(format!("I/O error: {}", e)),
        sqlx::Error::Tls(e) => OperationalError::new_err(format!("TLS error: {}", e)),
        sqlx::Error::Protocol(msg) => InternalError::new_err(format!("Protocol error: {}", msg)),
        sqlx::Error::Configuration(e) => {
            InterfaceError::new_err(format!("Invalid configuration: {}", e))
        }
        sqlx::Error::ColumnNotFound(name) => {
            ProgrammingError::new_err(format!("Column '{}' not found", name))
        }
        sqlx::Error::ColumnIndexOutOfBounds { index, len } => ProgrammingError::new_err(format!(
            "Column index {} out of bounds (row has {} columns)",
            index, len
        )),
        sqlx::Error::ColumnDecode { index, source } => {
            DataError::new_err(format!("Failed to decode column {}: {}", index, source))
        }
        sqlx::Error::Decode(e) => DataError::new_err(format!("Decode error: {}", e)),
        sqlx::Error::Encode(e) => DataError::new_err(format!("Encode error: {}", e)),
        sqlx::Error::TypeNotFound { type_name } => {
            NotSupportedError::new_err(format!("Type '{}' not found", type_name))
        }
        other => DatabaseError::new_err(format!("Database error: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_connection_sqlstate() {
        assert_eq!(classify_sqlstate("08006"), PostgreSQLErrorClass::ConnectionIssue);
    }

    #[test]
    fn classifies_unique_violation_sqlstate() {
        assert_eq!(classify_sqlstate("23505"), PostgreSQLErrorClass::ConstraintViolation);
    }

    #[test]
    fn classifies_syntax_error_sqlstate() {
        assert_eq!(classify_sqlstate("42601"), PostgreSQLErrorClass::SyntaxError);
    }

    #[test]
    fn classifies_unknown_sqlstate_as_generic() {
        assert_eq!(classify_sqlstate("99999"), PostgreSQLErrorClass::GenericDatabase);
    }

    #[test]
    fn classifies_insufficient_resources() {
        assert_eq!(classify_sqlstate("53200"), PostgreSQLErrorClass::InsufficientResources);
        assert_eq!(classify_sqlstate("54000"), PostgreSQLErrorClass::InsufficientResources);
    }
}
