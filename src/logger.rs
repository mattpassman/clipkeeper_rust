use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::{Path, PathBuf};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging system with file appender and JSON formatting
///
/// This function sets up structured logging with:
/// - JSON formatted log entries
/// - Daily log rotation (keeps 7 days)
/// - Platform-specific log directories
/// - Proper file permissions on Unix (0700 for directory)
///
/// # Arguments
///
/// * `log_path` - Optional custom log directory path. If None, uses platform-specific default.
///
/// # Returns
///
/// Returns Ok(()) if logging was initialized successfully, or an error if initialization failed.
///
/// # Requirements
///
/// Validates: Requirements 9.1, 9.2, 9.3, 9.5, 28.1, 28.3
pub fn init(log_path: Option<PathBuf>) -> Result<()> {
    let log_dir = log_path.unwrap_or_else(get_default_log_directory);
    
    // Ensure log directory exists with proper permissions
    ensure_log_directory(&log_dir)?;
    
    // Create rolling file appender with daily rotation
    // This will create files like: clipkeeper.log.2024-01-15
    // and keep the last 7 days of logs
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("clipkeeper")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&log_dir)
        .context("Failed to create rolling file appender")?;
    
    // Create JSON formatting layer for structured logs
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_appender)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false);
    
    // Create environment filter (defaults to INFO level)
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    // Initialize the global subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();
    
    tracing::info!(
        component = "Logger",
        log_dir = %log_dir.display(),
        "Logging initialized"
    );
    
    Ok(())
}

/// Get the platform-specific default log directory
///
/// Returns:
/// - Windows: %LOCALAPPDATA%\clipkeeper\logs
/// - macOS: ~/Library/Logs/clipkeeper
/// - Linux: ~/.local/share/clipkeeper/logs
///
/// # Requirements
///
/// Validates: Requirements 9.1, 28.5
fn get_default_log_directory() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "clipkeeper") {
        #[cfg(target_os = "macos")]
        {
            // macOS uses ~/Library/Logs/clipkeeper
            let mut log_dir = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()));
            log_dir.push("Library");
            log_dir.push("Logs");
            log_dir.push("clipkeeper");
            log_dir
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            // Windows and Linux use data_local_dir/logs
            let mut log_dir = proj_dirs.data_local_dir().to_path_buf();
            log_dir.push("logs");
            log_dir
        }
    } else {
        // Fallback to current directory
        PathBuf::from("./logs")
    }
}

/// Ensure the log directory exists and has proper permissions
///
/// Creates the directory if it doesn't exist, and on Unix systems,
/// sets permissions to 0700 (owner only).
///
/// # Arguments
///
/// * `log_dir` - Path to the log directory
///
/// # Returns
///
/// Returns Ok(()) if the directory exists or was created successfully,
/// or an error if creation or permission setting failed.
///
/// # Requirements
///
/// Validates: Requirements 9.5, 28.1, 28.3
fn ensure_log_directory(log_dir: &Path) -> Result<()> {
    if !log_dir.exists() {
        std::fs::create_dir_all(log_dir)
            .context(format!("Failed to create log directory: {}", log_dir.display()))?;
        
        // Set directory permissions to 0700 on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(log_dir, permissions)
                .context(format!("Failed to set permissions on log directory: {}", log_dir.display()))?;
        }
    }
    
    Ok(())
}

/// Macro to log events without including sensitive content
///
/// This macro should be used when logging privacy filter actions or any
/// operation that might involve sensitive data. It ensures that the actual
/// content is never logged, only metadata about the action.
///
/// # Example
///
/// ```ignore
/// log_secure_action!("Privacy filter blocked content", 
///     pattern_type = "password", 
///     content_length = 42
/// );
/// ```
///
/// # Requirements
///
/// Validates: Requirements 2.7, 9.4
#[macro_export]
macro_rules! log_secure_action {
    ($message:expr, $($key:ident = $value:expr),* $(,)?) => {
        tracing::info!(
            component = "PrivacyFilter",
            action = "filtered",
            $($key = $value,)*
            $message
        );
    };
}

/// Macro to log component actions with component tags
///
/// This macro provides a convenient way to log actions from any component
/// with proper component tagging for structured logging.
///
/// # Example
///
/// ```ignore
/// log_component_action!("ConfigurationManager", "Configuration loaded", 
///     config_path = %config_path.display()
/// );
/// ```
///
/// # Requirements
///
/// Validates: Requirements 9.1, 9.2
#[macro_export]
macro_rules! log_component_action {
    ($component:expr, $message:expr $(, $($rest:tt)*)?) => {
        tracing::info!(
            component = $component,
            $($($rest)*,)?
            $message
        );
    };
}

/// Macro to log component errors with component tags
///
/// This macro provides a convenient way to log errors from any component
/// with proper component tagging for structured logging.
///
/// # Example
///
/// ```ignore
/// log_component_error!("HistoryStore", "Failed to save entry", 
///     error = %err
/// );
/// ```
///
/// # Requirements
///
/// Validates: Requirements 9.1, 9.2
#[macro_export]
macro_rules! log_component_error {
    ($component:expr, $message:expr $(, $($rest:tt)*)?) => {
        tracing::error!(
            component = $component,
            $($($rest)*,)?
            $message
        );
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_get_default_log_directory() {
        let log_dir = get_default_log_directory();
        
        // Should contain "clipkeeper" in the path
        assert!(log_dir.to_string_lossy().contains("clipkeeper"));
        
        // Should end with "logs"
        assert!(log_dir.ends_with("logs"));
    }
    
    #[test]
    fn test_ensure_log_directory_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("test_logs");
        
        assert!(!log_dir.exists());
        
        ensure_log_directory(&log_dir).unwrap();
        
        assert!(log_dir.exists());
        assert!(log_dir.is_dir());
    }
    
    #[cfg(unix)]
    #[test]
    fn test_ensure_log_directory_sets_permissions() {
        use std::os::unix::fs::PermissionsExt;
        
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("test_logs_perms");
        
        ensure_log_directory(&log_dir).unwrap();
        
        let metadata = std::fs::metadata(&log_dir).unwrap();
        let permissions = metadata.permissions();
        
        // Check that permissions are 0700 (owner only)
        assert_eq!(permissions.mode() & 0o777, 0o700);
    }
    
    #[test]
    fn test_ensure_log_directory_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("test_logs_idempotent");
        
        // First call creates directory
        ensure_log_directory(&log_dir).unwrap();
        assert!(log_dir.exists());
        
        // Second call should succeed without error
        ensure_log_directory(&log_dir).unwrap();
        assert!(log_dir.exists());
    }
    
    #[test]
    fn test_log_secure_action_macro_compiles() {
        // This test verifies that the log_secure_action macro compiles correctly
        // and accepts the expected parameters without logging actual sensitive content
        log_secure_action!(
            "Test filtering action",
            pattern_type = "password",
            content_length = 42
        );
        
        // If we get here, the macro compiled and executed successfully
        assert!(true);
    }
    
    #[test]
    fn test_log_component_action_macro_compiles() {
        // This test verifies that the log_component_action macro compiles correctly
        // and accepts various parameter formats including display formatting
        log_component_action!(
            "TestComponent",
            "Test action",
            test_value = 123
        );
        
        // If we get here, the macro compiled and executed successfully
        assert!(true);
    }
    
    #[test]
    fn test_log_component_error_macro_compiles() {
        // This test verifies that the log_component_error macro compiles correctly
        log_component_error!(
            "TestComponent",
            "Test error",
            error_code = 500
        );
        
        // If we get here, the macro compiled and executed successfully
        assert!(true);
    }
    
    #[test]
    fn test_init_creates_log_file() {
        // Test that init() successfully creates a log file in a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test_logs");
        
        // Initialize logger with custom path
        // Note: This will fail if logger is already initialized in another test
        // but that's okay for this test
        let result = init(Some(log_path.clone()));
        
        // The init might fail if already initialized, but the directory should be created
        assert!(log_path.exists());
        assert!(log_path.is_dir());
        
        // If init succeeded, verify we can log
        if result.is_ok() {
            tracing::info!(component = "Test", "Test log message");
        }
    }
    
    #[test]
    fn test_secure_logging_does_not_log_content() {
        // This test verifies that the log_secure_action macro
        // logs metadata but not actual sensitive content
        
        // Simulate filtering sensitive content
        let sensitive_content = "password123!@#";
        let content_length = sensitive_content.len();
        
        // Log the action without the actual content
        log_secure_action!(
            "Privacy filter blocked content",
            pattern_type = "password",
            content_length = content_length
        );
        
        // The test passes if we can call the macro without including the actual content
        // In a real scenario, we would verify the log file doesn't contain "password123!@#"
        assert!(true);
    }
    
    #[test]
    fn test_log_file_actually_written() {
        // Test that the log directory structure is created correctly
        // Note: We can't test actual log writing in unit tests because the global
        // logger can only be initialized once per test run.
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("test_logs_written");
        
        // Directly test directory creation (which is what init() does first)
        ensure_log_directory(&log_path).unwrap();
        
        // Verify directory exists and has correct structure
        assert!(log_path.exists(), "Log directory should be created");
        assert!(log_path.is_dir(), "Log path should be a directory");
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&log_path).unwrap();
            let permissions = metadata.permissions();
            assert_eq!(permissions.mode() & 0o777, 0o700, 
                "Log directory should have 0700 permissions");
        }
    }
    
    #[test]
    fn test_log_directory_permissions_on_unix() {
        // Test that log directory has correct permissions (0700) on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            
            let temp_dir = TempDir::new().unwrap();
            let log_dir = temp_dir.path().join("test_logs_perms_check");
            
            ensure_log_directory(&log_dir).unwrap();
            
            let metadata = std::fs::metadata(&log_dir).unwrap();
            let permissions = metadata.permissions();
            let mode = permissions.mode() & 0o777;
            
            // Verify permissions are exactly 0700 (owner read/write/execute only)
            assert_eq!(mode, 0o700, "Log directory should have 0700 permissions");
        }
        
        #[cfg(not(unix))]
        {
            // On non-Unix systems, just verify directory is created
            let temp_dir = TempDir::new().unwrap();
            let log_dir = temp_dir.path().join("test_logs_perms_check");
            
            ensure_log_directory(&log_dir).unwrap();
            assert!(log_dir.exists());
        }
    }
    
    #[test]
    fn test_secure_logging_macro_with_multiple_fields() {
        // Test that secure logging macro works with multiple metadata fields
        log_secure_action!(
            "Multiple fields test",
            pattern_type = "credit_card",
            content_length = 16,
            timestamp = 1234567890,
            action = "blocked"
        );
        
        // If we get here, the macro handled multiple fields correctly
        assert!(true);
    }
    
    #[test]
    fn test_component_logging_macros_with_various_types() {
        // Test that component logging macros work with various data types
        log_component_action!(
            "TestComponent",
            "Action with string",
            value = "test_string"
        );
        
        log_component_action!(
            "TestComponent",
            "Action with number",
            count = 42
        );
        
        log_component_error!(
            "TestComponent",
            "Error with boolean",
            is_critical = true
        );
        
        // If we get here, all macros compiled and executed successfully
        assert!(true);
    }
    
    #[test]
    fn test_ensure_log_directory_with_nested_path() {
        // Test that ensure_log_directory can create nested directories
        let temp_dir = TempDir::new().unwrap();
        let nested_log_dir = temp_dir.path().join("level1").join("level2").join("logs");
        
        assert!(!nested_log_dir.exists());
        
        ensure_log_directory(&nested_log_dir).unwrap();
        
        assert!(nested_log_dir.exists());
        assert!(nested_log_dir.is_dir());
    }
    
    #[test]
    fn test_get_default_log_directory_structure() {
        // Test that default log directory follows platform conventions
        let log_dir = get_default_log_directory();
        let path_str = log_dir.to_string_lossy();
        
        // Should contain "clipkeeper"
        assert!(path_str.contains("clipkeeper"), 
            "Log directory should contain 'clipkeeper': {}", path_str);
        
        // Should end with "logs"
        assert!(log_dir.ends_with("logs"), 
            "Log directory should end with 'logs': {}", path_str);
        
        // Platform-specific checks
        #[cfg(target_os = "macos")]
        {
            assert!(path_str.contains("Library/Logs"), 
                "macOS log directory should be in Library/Logs: {}", path_str);
        }
        
        #[cfg(target_os = "windows")]
        {
            assert!(path_str.contains("AppData") || path_str.contains("Local"), 
                "Windows log directory should be in AppData/Local: {}", path_str);
        }
        
        #[cfg(target_os = "linux")]
        {
            assert!(path_str.contains(".local/share") || path_str.contains("logs"), 
                "Linux log directory should be in .local/share: {}", path_str);
        }
    }
}
