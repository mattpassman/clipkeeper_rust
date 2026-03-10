use clipkeeper::config::{Config, PrivacyConfig, RetentionConfig, MonitoringConfig, StorageConfig, SearchConfig};
use std::path::PathBuf;

fn make_test_config() -> Config {
    Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec!["pat1".to_string(), "pat2".to_string()],
            custom_patterns: vec!["custom1".to_string()],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig {
            poll_interval: 500,
            auto_start: true,
            enabled: false,
        },
        storage: StorageConfig {
            data_dir: PathBuf::from("/tmp/data"),
            db_path: Some(PathBuf::from("/tmp/data/test.db")),
            log_path: PathBuf::from("/tmp/data/test.log"),
        },
        search: SearchConfig { default_limit: 75 },
    }
}

// --- Task 6.3: Test get for all nested keys ---

#[test]
fn test_get_version() {
    let config = make_test_config();
    assert_eq!(config.get("version").unwrap(), "1.0");
}

#[test]
fn test_get_retention_days() {
    let config = make_test_config();
    assert_eq!(config.get("retention.days").unwrap(), "30");
}

#[test]
fn test_get_monitoring_poll_interval() {
    let config = make_test_config();
    assert_eq!(config.get("monitoring.poll_interval").unwrap(), "500");
}

#[test]
fn test_get_monitoring_poll_interval_camel_case() {
    let config = make_test_config();
    assert_eq!(config.get("monitoring.pollInterval").unwrap(), "500");
}

#[test]
fn test_get_monitoring_auto_start() {
    let config = make_test_config();
    assert_eq!(config.get("monitoring.auto_start").unwrap(), "true");
}

#[test]
fn test_get_monitoring_auto_start_camel_case() {
    let config = make_test_config();
    assert_eq!(config.get("monitoring.autoStart").unwrap(), "true");
}

#[test]
fn test_get_monitoring_enabled() {
    let config = make_test_config();
    assert_eq!(config.get("monitoring.enabled").unwrap(), "false");
}

#[test]
fn test_get_privacy_enabled() {
    let config = make_test_config();
    assert_eq!(config.get("privacy.enabled").unwrap(), "true");
}

#[test]
fn test_get_privacy_patterns() {
    let config = make_test_config();
    assert_eq!(config.get("privacy.patterns").unwrap(), "pat1,pat2");
}

#[test]
fn test_get_privacy_custom_patterns() {
    let config = make_test_config();
    assert_eq!(config.get("privacy.custom_patterns").unwrap(), "custom1");
}

#[test]
fn test_get_privacy_custom_patterns_camel_case() {
    let config = make_test_config();
    assert_eq!(config.get("privacy.customPatterns").unwrap(), "custom1");
}

#[test]
fn test_get_storage_data_dir() {
    let config = make_test_config();
    assert_eq!(config.get("storage.data_dir").unwrap(), "/tmp/data");
}

#[test]
fn test_get_storage_data_dir_camel_case() {
    let config = make_test_config();
    assert_eq!(config.get("storage.dataDir").unwrap(), "/tmp/data");
}

#[test]
fn test_get_storage_db_path() {
    let config = make_test_config();
    assert_eq!(config.get("storage.db_path").unwrap(), "/tmp/data/test.db");
}

#[test]
fn test_get_storage_db_path_camel_case() {
    let config = make_test_config();
    assert_eq!(config.get("storage.dbPath").unwrap(), "/tmp/data/test.db");
}

#[test]
fn test_get_storage_log_path() {
    let config = make_test_config();
    assert_eq!(config.get("storage.log_path").unwrap(), "/tmp/data/test.log");
}

#[test]
fn test_get_storage_log_path_camel_case() {
    let config = make_test_config();
    assert_eq!(config.get("storage.logPath").unwrap(), "/tmp/data/test.log");
}

#[test]
fn test_get_search_default_limit() {
    let config = make_test_config();
    assert_eq!(config.get("search.default_limit").unwrap(), "75");
}

#[test]
fn test_get_search_default_limit_camel_case() {
    let config = make_test_config();
    assert_eq!(config.get("search.defaultLimit").unwrap(), "75");
}

// --- Task 6.3: Test descriptive error messages for unknown keys ---

#[test]
fn test_get_unknown_key_error_message() {
    let config = make_test_config();
    let err = config.get("nonexistent.key").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown configuration key: 'nonexistent.key'"), "Got: {}", msg);
    assert!(msg.contains("Valid keys are:"), "Got: {}", msg);
    assert!(msg.contains("retention.days"), "Got: {}", msg);
}

#[test]
fn test_set_unknown_key_error_message() {
    let mut config = make_test_config();
    let err = config.set("nonexistent.key", "value").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown configuration key: 'nonexistent.key'"), "Got: {}", msg);
    assert!(msg.contains("Valid keys are:"), "Got: {}", msg);
}

// --- Task 6.3: Test set for all nested keys ---

#[test]
fn test_set_version() {
    let mut config = make_test_config();
    config.set("version", "2.0").unwrap();
    assert_eq!(config.version, "2.0");
}

#[test]
fn test_set_monitoring_auto_start() {
    let mut config = make_test_config();
    config.set("monitoring.auto_start", "false").unwrap();
    assert!(!config.monitoring.auto_start);
}

#[test]
fn test_set_monitoring_auto_start_camel_case() {
    let mut config = make_test_config();
    config.set("monitoring.autoStart", "true").unwrap();
    assert!(config.monitoring.auto_start);
}

#[test]
fn test_set_monitoring_enabled() {
    let mut config = make_test_config();
    config.set("monitoring.enabled", "true").unwrap();
    assert!(config.monitoring.enabled);
}

#[test]
fn test_set_privacy_patterns() {
    let mut config = make_test_config();
    config.set("privacy.patterns", "a,b,c").unwrap();
    assert_eq!(config.privacy.patterns, vec!["a", "b", "c"]);
}

#[test]
fn test_set_storage_data_dir() {
    let mut config = make_test_config();
    config.set("storage.data_dir", "/new/path").unwrap();
    assert_eq!(config.storage.data_dir, PathBuf::from("/new/path"));
}

#[test]
fn test_set_storage_data_dir_rejects_relative() {
    let mut config = make_test_config();
    let err = config.set("storage.data_dir", "relative/path").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("absolute path"), "Got: {}", msg);
}

#[test]
fn test_set_storage_log_path() {
    let mut config = make_test_config();
    config.set("storage.log_path", "/new/log.txt").unwrap();
    assert_eq!(config.storage.log_path, PathBuf::from("/new/log.txt"));
}

#[test]
fn test_set_monitoring_poll_interval_rejects_zero() {
    let mut config = make_test_config();
    let err = config.set("monitoring.poll_interval", "0").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Must be greater than 0"), "Got: {}", msg);
}

#[test]
fn test_set_search_default_limit_rejects_zero() {
    let mut config = make_test_config();
    let err = config.set("search.default_limit", "0").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Must be greater than 0"), "Got: {}", msg);
}

#[test]
fn test_set_descriptive_error_for_invalid_number() {
    let mut config = make_test_config();
    let err = config.set("retention.days", "abc").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Invalid value for retention.days"), "Got: {}", msg);
    assert!(msg.contains("abc"), "Got: {}", msg);
}

#[test]
fn test_set_descriptive_error_for_invalid_boolean() {
    let mut config = make_test_config();
    let err = config.set("privacy.enabled", "yes").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Invalid value for privacy.enabled"), "Got: {}", msg);
    assert!(msg.contains("boolean"), "Got: {}", msg);
}
