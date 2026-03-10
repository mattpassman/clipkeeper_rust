use clipkeeper::config::{Config, PrivacyConfig, RetentionConfig, MonitoringConfig, StorageConfig, SearchConfig};
use std::path::PathBuf;

#[test]
fn test_privacy_custom_patterns_field() {
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec!["pattern1".to_string(), "pattern2".to_string()],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig { poll_interval: 500, auto_start: false, enabled: true },
        storage: StorageConfig {
            data_dir: PathBuf::from("/tmp/test"),
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    assert_eq!(config.privacy.custom_patterns.len(), 2);
    assert_eq!(config.privacy.custom_patterns[0], "pattern1");
    assert_eq!(config.privacy.custom_patterns[1], "pattern2");
}

#[test]
fn test_storage_db_path_optional_with_default() {
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
            data_dir: PathBuf::from("/tmp/test"),
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    // When db_path is None, get_db_path should return data_dir/clipkeeper.db
    let db_path = config.storage.get_db_path();
    assert_eq!(db_path, PathBuf::from("/tmp/test/clipkeeper.db"));
}

#[test]
fn test_storage_db_path_optional_with_custom() {
    let custom_path = PathBuf::from("/custom/path/database.db");
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
            data_dir: PathBuf::from("/tmp/test"),
            db_path: Some(custom_path.clone()),
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 50 },
    };

    // When db_path is Some, get_db_path should return the custom path
    let db_path = config.storage.get_db_path();
    assert_eq!(db_path, custom_path);
}

#[test]
fn test_search_default_limit_field() {
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
            data_dir: PathBuf::from("/tmp/test"),
            db_path: None,
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 100 },
    };

    assert_eq!(config.search.default_limit, 100);
}

#[test]
fn test_default_config_has_all_fields() {
    let config = Config::default();

    // Check privacy has custom_patterns
    assert!(config.privacy.custom_patterns.is_empty());

    // Check storage db_path is None (will default to data_dir/clipkeeper.db)
    assert!(config.storage.db_path.is_none());
    let db_path = config.storage.get_db_path();
    assert!(db_path.ends_with("clipkeeper.db"));

    // Check search has default_limit
    assert_eq!(config.search.default_limit, 50);
}

#[test]
fn test_config_get_new_fields() {
    let mut config = Config::default();
    config.privacy.custom_patterns = vec!["test1".to_string(), "test2".to_string()];
    config.search.default_limit = 75;

    // Test getting custom_patterns
    let patterns = config.get("privacy.custom_patterns").unwrap();
    assert_eq!(patterns, "test1,test2");

    // Test getting db_path
    let db_path = config.get("storage.db_path").unwrap();
    assert!(db_path.ends_with("clipkeeper.db"));

    // Test getting default_limit
    let limit = config.get("search.default_limit").unwrap();
    assert_eq!(limit, "75");
}

#[test]
fn test_config_set_new_fields() {
    let mut config = Config::default();

    // Test setting custom_patterns
    config.set("privacy.custom_patterns", "pattern1,pattern2,pattern3").unwrap();
    assert_eq!(config.privacy.custom_patterns.len(), 3);
    assert_eq!(config.privacy.custom_patterns[0], "pattern1");
    assert_eq!(config.privacy.custom_patterns[1], "pattern2");
    assert_eq!(config.privacy.custom_patterns[2], "pattern3");

    // Test setting db_path
    config.set("storage.db_path", "/custom/db.db").unwrap();
    assert_eq!(config.storage.db_path, Some(PathBuf::from("/custom/db.db")));

    // Test setting default_limit
    config.set("search.default_limit", "200").unwrap();
    assert_eq!(config.search.default_limit, 200);
}

#[test]
fn test_config_serialization_with_new_fields() {
    let config = Config {
        version: "1.0".to_string(),
        privacy: PrivacyConfig {
            enabled: true,
            patterns: vec![],
            custom_patterns: vec!["custom1".to_string()],
        },
        retention: RetentionConfig { days: 30 },
        monitoring: MonitoringConfig { poll_interval: 500, auto_start: false, enabled: true },
        storage: StorageConfig {
            data_dir: PathBuf::from("/tmp/test"),
            db_path: Some(PathBuf::from("/custom/db.db")),
            log_path: PathBuf::from("/tmp/test/log.txt"),
        },
        search: SearchConfig { default_limit: 100 },
    };

    // Serialize to JSON
    let json = serde_json::to_string(&config).unwrap();

    // Deserialize back
    let deserialized: Config = serde_json::from_str(&json).unwrap();

    // Verify all fields
    assert_eq!(deserialized.privacy.custom_patterns.len(), 1);
    assert_eq!(deserialized.privacy.custom_patterns[0], "custom1");
    assert_eq!(deserialized.storage.db_path, Some(PathBuf::from("/custom/db.db")));
    assert_eq!(deserialized.search.default_limit, 100);
}
