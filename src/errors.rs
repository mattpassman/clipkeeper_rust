use thiserror::Error;

// Re-export anyhow types for convenience
pub use anyhow::{Context, Result};

/// Main result type - use anyhow::Result for most cases
/// Only use specific error types (DatabaseError, ClipboardError) when needed
pub type ClipKeeperResult<T> = anyhow::Result<T>;

/// Database-related errors
/// Keep these specific for database operations
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Database is not open")]
    NotOpen,

    #[error("Entry not found: {0}")]
    EntryNotFound(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Clipboard-related errors
/// Keep these specific for clipboard operations
#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("Clipboard access denied")]
    AccessDenied,

    #[error("Clipboard is unavailable")]
    Unavailable,

    #[error("Arboard error: {0}")]
    Arboard(String),
}

// Conversion from arboard errors
impl From<arboard::Error> for ClipboardError {
    fn from(err: arboard::Error) -> Self {
        ClipboardError::Arboard(err.to_string())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // Test error message formatting
    #[test]
    fn test_database_error_display() {
        let err = DatabaseError::NotOpen;
        assert_eq!(err.to_string(), "Database is not open");

        let err = DatabaseError::EntryNotFound("abc123".to_string());
        assert_eq!(err.to_string(), "Entry not found: abc123");
    }

    #[test]
    fn test_database_error_entry_not_found_with_various_ids() {
        let test_cases = vec![
            ("abc123", "Entry not found: abc123"),
            ("", "Entry not found: "),
            ("very-long-uuid-12345678-1234-1234-1234-123456789012", 
             "Entry not found: very-long-uuid-12345678-1234-1234-1234-123456789012"),
        ];

        for (id, expected) in test_cases {
            let err = DatabaseError::EntryNotFound(id.to_string());
            assert_eq!(err.to_string(), expected);
        }
    }

    #[test]
    fn test_clipboard_error_display() {
        let err = ClipboardError::AccessDenied;
        assert_eq!(err.to_string(), "Clipboard access denied");

        let err = ClipboardError::Unavailable;
        assert_eq!(err.to_string(), "Clipboard is unavailable");
    }

    #[test]
    fn test_clipboard_error_arboard_message() {
        let err = ClipboardError::Arboard("Custom arboard error".to_string());
        assert_eq!(err.to_string(), "Arboard error: Custom arboard error");
    }

    // Test error conversion and propagation
    #[test]
    fn test_database_error_to_anyhow() {
        // Test that DatabaseError can be converted to anyhow::Error
        let db_err = DatabaseError::NotOpen;
        let result: Result<()> = Err(db_err.into());
        assert!(result.is_err());
        
        let err_msg = format!("{}", result.unwrap_err());
        assert_eq!(err_msg, "Database is not open");
    }

    #[test]
    fn test_clipboard_error_to_anyhow() {
        // Test that ClipboardError can be converted to anyhow::Error
        let clip_err = ClipboardError::AccessDenied;
        let result: Result<()> = Err(clip_err.into());
        assert!(result.is_err());
        
        let err_msg = format!("{}", result.unwrap_err());
        assert_eq!(err_msg, "Clipboard access denied");
    }

    #[test]
    fn test_error_propagation_with_question_mark() {
        fn database_operation() -> Result<()> {
            Err(DatabaseError::NotOpen)?
        }

        fn clipboard_operation() -> Result<()> {
            Err(ClipboardError::Unavailable)?
        }

        // Test that ? operator works for error propagation
        let db_result = database_operation();
        assert!(db_result.is_err());
        assert!(db_result.unwrap_err().to_string().contains("Database is not open"));

        let clip_result = clipboard_operation();
        assert!(clip_result.is_err());
        assert!(clip_result.unwrap_err().to_string().contains("Clipboard is unavailable"));
    }

    #[test]
    fn test_anyhow_context_adds_information() {
        fn failing_operation() -> Result<()> {
            Err(DatabaseError::NotOpen)?
        }

        let result = failing_operation().context("Failed to query database");
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(err_msg.contains("Failed to query database"));
        assert!(err_msg.contains("Database is not open"));
    }

    #[test]
    fn test_nested_context() {
        fn inner_operation() -> Result<()> {
            Err(DatabaseError::EntryNotFound("test-id".to_string()))?
        }

        fn middle_operation() -> Result<()> {
            inner_operation().context("Failed to retrieve entry")?;
            Ok(())
        }

        fn outer_operation() -> Result<()> {
            middle_operation().context("Database query failed")?;
            Ok(())
        }

        let result = outer_operation();
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        
        // Should contain all context layers
        assert!(err_msg.contains("Database query failed"));
        assert!(err_msg.contains("Failed to retrieve entry"));
        assert!(err_msg.contains("Entry not found: test-id"));
    }

    #[test]
    fn test_rusqlite_error_conversion() {
        // Test that rusqlite errors convert to DatabaseError
        let sqlite_err = rusqlite::Error::InvalidQuery;
        let db_err: DatabaseError = sqlite_err.into();
        
        assert!(db_err.to_string().contains("SQLite error"));
    }

    #[test]
    fn test_arboard_error_conversion() {
        // Test the From implementation for arboard::Error
        // We can't easily create real arboard errors, so we test the string conversion
        let arboard_msg = "Test arboard error message";
        let clip_err = ClipboardError::Arboard(arboard_msg.to_string());
        
        assert_eq!(clip_err.to_string(), format!("Arboard error: {}", arboard_msg));
    }

    #[test]
    fn test_error_chain_with_multiple_types() {
        fn operation_with_multiple_errors(use_db_error: bool) -> Result<()> {
            if use_db_error {
                Err(DatabaseError::NotOpen)?
            } else {
                Err(ClipboardError::AccessDenied)?
            }
        }

        // Test database error path
        let db_result = operation_with_multiple_errors(true);
        assert!(db_result.is_err());
        assert!(db_result.unwrap_err().to_string().contains("Database is not open"));

        // Test clipboard error path
        let clip_result = operation_with_multiple_errors(false);
        assert!(clip_result.is_err());
        assert!(clip_result.unwrap_err().to_string().contains("Clipboard access denied"));
    }

    #[test]
    fn test_error_downcast() {
        // Test that we can downcast anyhow::Error back to specific types
        let db_err = DatabaseError::NotOpen;
        let anyhow_err: anyhow::Error = db_err.into();
        
        // Check if we can identify the error type
        let is_db_error = anyhow_err.downcast_ref::<DatabaseError>().is_some();
        assert!(is_db_error);
    }

    #[test]
    fn test_clipkeeper_result_type_alias() {
        // Test that ClipKeeperResult works as expected
        fn operation_returning_clipkeeper_result() -> ClipKeeperResult<String> {
            Ok("success".to_string())
        }

        fn failing_operation_returning_clipkeeper_result() -> ClipKeeperResult<String> {
            Err(DatabaseError::NotOpen)?
        }

        let success = operation_returning_clipkeeper_result();
        assert!(success.is_ok());
        assert_eq!(success.unwrap(), "success");

        let failure = failing_operation_returning_clipkeeper_result();
        assert!(failure.is_err());
    }
}
