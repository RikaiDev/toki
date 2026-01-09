//! Database helper functions for safe type conversions.

use chrono::{DateTime, Utc};
use rusqlite::types::Type;

/// Parse a UUID string from database, returning a rusqlite error on failure.
pub fn parse_uuid(s: &str) -> rusqlite::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(e))
    })
}

/// Parse an RFC3339 datetime string from database, returning a rusqlite error on failure.
pub fn parse_datetime(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(e)))
}
