# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2024-10-01

### Added
- Initial release of pypg-driver
- High-performance PostgreSQL driver built with Rust and PyO3
- Synchronous API with async I/O under the hood using tokio-postgres
- Complete DB-API 2.0 compliance
- Full PostgreSQL type system support including:
  - Basic types (INTEGER, TEXT, BOOLEAN, etc.)
  - Date/time types with chrono integration
  - JSON/JSONB support
  - UUID support
  - Array types
  - Network types (INET, CIDR)
- Connection class with methods:
  - `query()` - Execute SELECT queries
  - `query_one()` - Get single row results
  - `execute()` - Execute INSERT/UPDATE/DELETE statements
  - `execute_batch()` - Bulk operations
  - `prepare()` - Prepared statements
  - `ping()` - Health checks
  - `info()` - Connection information
  - `begin()` - Transaction management
  - `close()` - Connection cleanup
  - `is_closed()` - Connection status
- Row class with dict-like interface
- Transaction class with automatic rollback
- Comprehensive error handling with PostgreSQL-specific exceptions
- Memory-safe implementation with Rust's ownership system
- Zero external Python dependencies
- Pre-built wheels for easy installation

### Performance
- Native Rust performance with zero-copy operations
- Tokio async I/O for excellent network performance
- Optimized binary protocol parsing
- Efficient type conversions between PostgreSQL and Python

### Documentation
- Complete API reference
- Usage examples and best practices
- Integration guides for popular libraries (Pandas, FastAPI, etc.)
- Performance comparisons with other PostgreSQL drivers
- Migration guides from psycopg2 and asyncpg

[0.1.0]: https://github.com/magi8101/pypg-driver/releases/tag/v0.1.0