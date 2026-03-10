use clipkeeper::config::{Config, PrivacyConfig, RetentionConfig, MonitoringConfig, StorageConfig, SearchConfig};
use std::path::PathBuf;

/// Test that validates retention.days >= 0 (Requirement 20.1)
/// Note: u32 type ensures this is always true, but we test the validation logic
#[test]
fn test_validate_retention_days_non_negative() {
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec![],
        },
        retention: RetentionConfig { days: 0 }, // 0 is valid (unlimited retention)
        monitoring: MonitoringConfig { poll_interval: 500, auto_start: false, enabled: true },
        storage: StorageConfig {
            data_dir: PathBuf::from("/tmp/test"),
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    let errors = config.validate();
    // Should not have errors about retention.days since 0 is valid
    assert!(!errors.iter().any(|e| e.contains("retention.days")));
}

/// Test that validates monitoring.pollInterval > 0 (Requirement 20.2)
#[test]
fn test_validate_poll_interval_positive() {
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec![],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig { poll_interval: 0, auto_start: false, enabled: true }, // Invalid: must be > 0
        storage: StorageConfig {
            data_dir: PathBuf::from("/tmp/test"),
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    let errors = config.validate();
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("monitoring.poll_interval") && e.contains("greater than 0")));
}

#[test]
fn test_validate_poll_interval_valid() {
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec![],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig { poll_interval: 500, auto_start: false, enabled: true }, // Valid
        storage: StorageConfig {
            data_dir: PathBuf::from("/tmp/test"),
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    let errors = config.validate();
    // Should not have errors about poll_interval
    assert!(!errors.iter().any(|e| e.contains("monitoring.poll_interval")));
}

/// Test that validates privacy.enabled is boolean (Requirement 20.3)
/// Note: This is enforced by the type system, but we test JSON validation
#[test]
fn test_validate_privacy_enabled_boolean_from_json() {
    use serde_json::json;
    
    // Valid boolean
    let errors = Config::validate_value("privacy.enabled", &json!(true));
    assert!(errors.is_empty());
    
    let errors = Config::validate_value("privacy.enabled", &json!(false));
    assert!(errors.is_empty());
    
    // Invalid: string
    let errors = Config::validate_value("privacy.enabled", &json!("true"));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("privacy.enabled") && e.contains("boolean")));
    
    // Invalid: number
    let errors = Config::validate_value("privacy.enabled", &json!(1));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("privacy.enabled") && e.contains("boolean")));
}

/// Test that validates storage.dataDir is valid path (Requirement 20.4)
#[test]
fn test_validate_data_dir_absolute_path() {
    // Invalid: relative path
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec![],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig { poll_interval: 500, auto_start: false, enabled: true },
        storage: StorageConfig {
            data_dir: PathBuf::from("relative/path"), // Invalid: not absolute
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    let errors = config.validate();
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("storage.data_dir") && e.contains("absolute path")));
}

#[test]
fn test_validate_data_dir_valid() {
    // Valid: absolute path
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec![],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig { poll_interval: 500, auto_start: false, enabled: true },
        storage: StorageConfig {
            data_dir: PathBuf::from("/tmp/test"), // Valid: absolute
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    let errors = config.validate();
    // Should not have errors about data_dir
    assert!(!errors.iter().any(|e| e.contains("storage.data_dir")));
}

/// Test that all validation errors are returned together (Requirement 20.5)
#[test]
fn test_validate_returns_all_errors() {
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec![],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig { poll_interval: 0, auto_start: false, enabled: true }, // Error 1: must be > 0
        storage: StorageConfig {
            data_dir: PathBuf::from("relative/path"), // Error 2: must be absolute
            db_path: Some(PathBuf::from("relative/db.db")), // Error 3: must be absolute
            log_path: PathBuf::from("relative/log.txt"), // Error 4: must be absolute
        },
        search: SearchConfig { default_limit: 0 }, // Error 5: must be > 0
    };

    let errors = config.validate();
    
    // Should have multiple errors
    assert!(errors.len() >= 4, "Expected at least 4 errors, got {}: {:?}", errors.len(), errors);
    
    // Check that all expected errors are present
    assert!(errors.iter().any(|e| e.contains("monitoring.poll_interval")));
    assert!(errors.iter().any(|e| e.contains("storage.data_dir")));
    assert!(errors.iter().any(|e| e.contains("storage.db_path")));
    assert!(errors.iter().any(|e| e.contains("storage.log_path")));
}

/// Test validate_value for retention.days with various inputs
#[test]
fn test_validate_value_retention_days() {
    use serde_json::json;
    
    // Valid: non-negative integers
    let errors = Config::validate_value("retention.days", &json!(0));
    assert!(errors.is_empty());
    
    let errors = Config::validate_value("retention.days", &json!(30));
    assert!(errors.is_empty());
    
    let errors = Config::validate_value("retention.days", &json!(365));
    assert!(errors.is_empty());
    
    // Invalid: negative number
    let errors = Config::validate_value("retention.days", &json!(-1));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("retention.days") && e.contains("non-negative")));
    
    // Invalid: float
    let errors = Config::validate_value("retention.days", &json!(30.5));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("retention.days") && e.contains("integer")));
    
    // Invalid: string
    let errors = Config::validate_value("retention.days", &json!("30"));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("retention.days")));
}

/// Test validate_value for monitoring.poll_interval with various inputs
#[test]
fn test_validate_value_poll_interval() {
    use serde_json::json;
    
    // Valid: positive integers
    let errors = Config::validate_value("monitoring.poll_interval", &json!(1));
    assert!(errors.is_empty());
    
    let errors = Config::validate_value("monitoring.poll_interval", &json!(500));
    assert!(errors.is_empty());
    
    // Invalid: zero
    let errors = Config::validate_value("monitoring.poll_interval", &json!(0));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("monitoring.poll_interval") && e.contains("positive")));
    
    // Invalid: negative
    let errors = Config::validate_value("monitoring.poll_interval", &json!(-100));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("monitoring.poll_interval") && e.contains("positive")));
    
    // Invalid: float
    let errors = Config::validate_value("monitoring.poll_interval", &json!(500.5));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("monitoring.poll_interval") && e.contains("integer")));
}

/// Test validate_value for storage.data_dir with various inputs
#[test]
fn test_validate_value_data_dir() {
    use serde_json::json;
    
    // Valid: absolute path
    let errors = Config::validate_value("storage.data_dir", &json!("/tmp/test"));
    assert!(errors.is_empty());
    
    // Invalid: relative path
    let errors = Config::validate_value("storage.data_dir", &json!("relative/path"));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("storage.data_dir") && e.contains("absolute")));
    
    // Invalid: not a string
    let errors = Config::validate_value("storage.data_dir", &json!(123));
    assert!(!errors.is_empty());
    assert!(errors.iter().any(|e| e.contains("storage.data_dir")));
}

/// Test that valid configuration passes all validation
#[test]
fn test_validate_valid_config() {
    let config = Config::default();
    let errors = config.validate();
    assert!(errors.is_empty(), "Default config should be valid, but got errors: {:?}", errors);
}

/// Test validation with custom db_path
#[test]
fn test_validate_custom_db_path() {
    let mut config = Config::default();
    
    // Valid: absolute path
    config.storage.db_path = Some(PathBuf::from("/custom/path/db.db"));
    let errors = config.validate();
    assert!(!errors.iter().any(|e| e.contains("storage.db_path")));
    
    // Invalid: relative path
    config.storage.db_path = Some(PathBuf::from("relative/db.db"));
    let errors = config.validate();
    assert!(errors.iter().any(|e| e.contains("storage.db_path") && e.contains("absolute")));
}

/// Test validation with search.default_limit
#[test]
fn test_validate_search_default_limit() {
    let mut config = Config::default();
    
    // Valid: positive number
    config.search.default_limit = 100;
    let errors = config.validate();
    assert!(!errors.iter().any(|e| e.contains("search.default_limit")));
    
    // Invalid: zero
    config.search.default_limit = 0;
    let errors = config.validate();
    assert!(errors.iter().any(|e| e.contains("search.default_limit") && e.contains("greater than 0")));
}
