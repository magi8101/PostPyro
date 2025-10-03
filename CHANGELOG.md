# Changelog

All notable changes to PostPyro will be documented in this file.

## [1.0.0] - 2025-10-03

### 🎉 First Production Release!

### Added

- **Production-ready PostgreSQL driver** built with Rust + PyO3 + tokio-postgres
- **Complete parameter system** supporting all PostgreSQL types:
  - Strings, integers, floats, booleans
  - Automatic type conversion with binary protocol
  - Proper NULL handling and type casting
- **Full DB-API 2.0 compliance** with threadsafety level 2
- **Transaction support** with automatic commit/rollback
- **Error handling** with detailed PostgreSQL error mapping
- **Performance optimizations**:
  - LRU statement caching for prepared statements
  - Binary protocol communication
  - Zero-copy operations where possible

### Performance

- **Fastest simple queries** compared to industry standards
- Competitive parameterized query performance
- Thread-safe multi-connection support

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
