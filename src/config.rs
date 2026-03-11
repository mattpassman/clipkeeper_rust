use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use directories::BaseDirs;
use crate::errors::{Context, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub privacy: PrivacyConfig,
    pub retention: RetentionConfig,
    pub monitoring: MonitoringConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub search: SearchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub enabled: bool,
    pub patterns: Vec<String>,
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    pub days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub poll_interval: u64,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_monitoring_enabled")]
    pub enabled: bool,
    /// Maximum metrics.log file size in KB before rotation (default 1024 = 1 MB).
    #[serde(default = "default_max_metrics_log_kb")]
    pub max_metrics_log_kb: u64,
}

fn default_max_metrics_log_kb() -> u64 {
    1024
}

fn default_monitoring_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    #[serde(default)]
    pub db_path: Option<PathBuf>,
    pub log_path: PathBuf,
}

impl StorageConfig {
    /// Get the database path, using default if not specified
    pub fn get_db_path(&self) -> PathBuf {
        self.db_path.clone().unwrap_or_else(|| {
            self.data_dir.join("clipkeeper.db")
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default = "default_search_limit")]
    pub default_limit: u32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self { default_limit: 50 }
    }
}

fn default_search_limit() -> u32 {
    50
}

impl Config {
    #[tracing::instrument(name = "config_load", skip_all)]
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path();
        
        if config_path.exists() {
            crate::log_component_action!(
                "ConfigurationManager",
                "Loading configuration",
                config_path = %config_path.display()
            );
            
            let content = fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            let config: Config = serde_json::from_str(&content)
                .context("Failed to parse config file")?;
            
            crate::log_component_action!(
                "ConfigurationManager",
                "Configuration loaded successfully",
                retention_days = config.retention.days,
                poll_interval = config.monitoring.poll_interval,
                privacy_enabled = config.privacy.enabled
            );
            
            Ok(config)
        } else {
            crate::log_component_action!(
                "ConfigurationManager",
                "Configuration file not found, creating default",
                config_path = %config_path.display()
            );
            
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }
    
    #[tracing::instrument(name = "config_save", skip(self))]
    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path();
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
            
            // Requirement 28.2: Set config directory to 0700 on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let dir_perms = fs::Permissions::from_mode(0o700);
                fs::set_permissions(parent, dir_perms)
                    .context("Failed to set config directory permissions to 0700")?;
            }
        }
        
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(&config_path, &content)
            .context("Failed to write config file")?;
        
        // Requirement 28.3: Set config file to 0600 on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file_perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&config_path, file_perms)
                .context("Failed to set config file permissions to 0600")?;
        }
        
        crate::log_component_action!(
            "ConfigurationManager",
            "Configuration saved",
            config_path = %config_path.display()
        );
        
        Ok(())
    }
    
    pub fn get_config_path() -> PathBuf {
        let base_dirs = BaseDirs::new().unwrap_or_else(|| panic!("Failed to get base directories"));
        
        if cfg!(target_os = "windows") {
            base_dirs.config_dir()
                .join("clipkeeper")
                .join("config.json")
        } else {
            base_dirs.home_dir()
                .join(".config")
                .join("clipkeeper")
                .join("config.json")
        }
    }
    
    #[tracing::instrument(name = "config_get", skip(self), fields(key = %key))]
    pub fn get(&self, key: &str) -> Result<String> {
        match key {
            "version" => Ok(self.version.clone()),
            "retention.days" => Ok(self.retention.days.to_string()),
            "monitoring.poll_interval" | "monitoring.pollInterval" => {
                Ok(self.monitoring.poll_interval.to_string())
            }
            "monitoring.auto_start" | "monitoring.autoStart" => {
                Ok(self.monitoring.auto_start.to_string())
            }
            "monitoring.enabled" => Ok(self.monitoring.enabled.to_string()),
            "privacy.enabled" => Ok(self.privacy.enabled.to_string()),
            "privacy.patterns" => Ok(self.privacy.patterns.join(",")),
            "privacy.custom_patterns" | "privacy.customPatterns" => {
                Ok(self.privacy.custom_patterns.join(","))
            }
            "storage.data_dir" | "storage.dataDir" => {
                Ok(self.storage.data_dir.display().to_string())
            }
            "storage.db_path" | "storage.dbPath" => {
                Ok(self.storage.get_db_path().display().to_string())
            }
            "storage.log_path" | "storage.logPath" => {
                Ok(self.storage.log_path.display().to_string())
            }
            "search.default_limit" | "search.defaultLimit" => {
                Ok(self.search.default_limit.to_string())
            }
            _ => {
                let valid_keys = [
                    "version",
                    "retention.days",
                    "monitoring.poll_interval",
                    "monitoring.auto_start",
                    "monitoring.enabled",
                    "privacy.enabled",
                    "privacy.patterns",
                    "privacy.custom_patterns",
                    "storage.data_dir",
                    "storage.db_path",
                    "storage.log_path",
                    "search.default_limit",
                ];
                anyhow::bail!(
                    "Unknown configuration key: '{}'. Valid keys are: {}",
                    key,
                    valid_keys.join(", ")
                )
            }
        }
    }
    
    #[tracing::instrument(name = "config_set", skip(self), fields(key = %key, value = %value))]
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        crate::log_component_action!(
            "ConfigurationManager",
            "Setting configuration value",
            key = key,
            value = value
        );
        
        match key {
            "version" => {
                self.version = value.to_string();
            }
            "retention.days" => {
                self.retention.days = value.parse()
                    .with_context(|| format!(
                        "Invalid value for retention.days: '{}'. Expected a non-negative integer (e.g., 0 for unlimited, 30 for 30 days)",
                        value
                    ))?;
            }
            "monitoring.poll_interval" | "monitoring.pollInterval" => {
                let parsed: u64 = value.parse()
                    .with_context(|| format!(
                        "Invalid value for monitoring.poll_interval: '{}'. Expected a positive integer in milliseconds (e.g., 500)",
                        value
                    ))?;
                if parsed == 0 {
                    anyhow::bail!(
                        "Invalid value for monitoring.poll_interval: '{}'. Must be greater than 0",
                        value
                    );
                }
                self.monitoring.poll_interval = parsed;
            }
            "monitoring.auto_start" | "monitoring.autoStart" => {
                self.monitoring.auto_start = value.parse()
                    .with_context(|| format!(
                        "Invalid value for monitoring.auto_start: '{}'. Expected a boolean (true or false)",
                        value
                    ))?;
            }
            "monitoring.enabled" => {
                self.monitoring.enabled = value.parse()
                    .with_context(|| format!(
                        "Invalid value for monitoring.enabled: '{}'. Expected a boolean (true or false)",
                        value
                    ))?;
            }
            "privacy.enabled" => {
                self.privacy.enabled = value.parse()
                    .with_context(|| format!(
                        "Invalid value for privacy.enabled: '{}'. Expected a boolean (true or false)",
                        value
                    ))?;
            }
            "privacy.patterns" => {
                self.privacy.patterns = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "privacy.custom_patterns" | "privacy.customPatterns" => {
                self.privacy.custom_patterns = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "storage.data_dir" | "storage.dataDir" => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    anyhow::bail!(
                        "Invalid value for storage.data_dir: '{}'. Must be an absolute path",
                        value
                    );
                }
                self.storage.data_dir = path;
            }
            "storage.db_path" | "storage.dbPath" => {
                self.storage.db_path = Some(PathBuf::from(value));
            }
            "storage.log_path" | "storage.logPath" => {
                self.storage.log_path = PathBuf::from(value);
            }
            "search.default_limit" | "search.defaultLimit" => {
                let parsed: u32 = value.parse()
                    .with_context(|| format!(
                        "Invalid value for search.default_limit: '{}'. Expected a positive integer",
                        value
                    ))?;
                if parsed == 0 {
                    anyhow::bail!(
                        "Invalid value for search.default_limit: '{}'. Must be greater than 0",
                        value
                    );
                }
                self.search.default_limit = parsed;
            }
            _ => {
                let valid_keys = [
                    "version",
                    "retention.days",
                    "monitoring.poll_interval",
                    "monitoring.auto_start",
                    "monitoring.enabled",
                    "privacy.enabled",
                    "privacy.patterns",
                    "privacy.custom_patterns",
                    "storage.data_dir",
                    "storage.db_path",
                    "storage.log_path",
                    "search.default_limit",
                ];
                anyhow::bail!(
                    "Unknown configuration key: '{}'. Valid keys are: {}",
                    key,
                    valid_keys.join(", ")
                );
            }
        }
        Ok(())
    }
    
    /// Validate all configuration values
    /// Returns a Vec of validation error messages (empty if valid)
    #[tracing::instrument(name = "config_validate", skip(self))]
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        
        // Requirement 20.1: Validate retention.days >= 0
        // Note: u32 is always non-negative, so this is automatically satisfied
        // But we document it for clarity
        
        // Requirement 20.2: Validate monitoring.pollInterval > 0
        if self.monitoring.poll_interval == 0 {
            errors.push("monitoring.poll_interval must be greater than 0".to_string());
        }
        
        // Requirement 20.3: Validate privacy.enabled is boolean
        // Note: This is enforced by the type system (bool), so no runtime check needed
        
        // Requirement 20.4: Validate storage.dataDir is valid path
        // Check if the path is absolute or can be created
        if !self.storage.data_dir.is_absolute() {
            errors.push(format!(
                "storage.data_dir must be an absolute path, got: {}",
                self.storage.data_dir.display()
            ));
        }
        
        // Additional validation: Check if parent directory exists or can be created
        if let Some(parent) = self.storage.data_dir.parent() {
            if !parent.exists() && parent != std::path::Path::new("") {
                // Try to check if we can potentially create it
                // We don't actually create it here, just validate the path structure
                if parent.components().count() == 0 {
                    errors.push(format!(
                        "storage.data_dir has invalid parent directory: {}",
                        self.storage.data_dir.display()
                    ));
                }
            }
        }
        
        // Validate db_path if specified
        if let Some(ref db_path) = self.storage.db_path {
            if !db_path.is_absolute() {
                errors.push(format!(
                    "storage.db_path must be an absolute path if specified, got: {}",
                    db_path.display()
                ));
            }
        }
        
        // Validate log_path
        if !self.storage.log_path.is_absolute() {
            errors.push(format!(
                "storage.log_path must be an absolute path, got: {}",
                self.storage.log_path.display()
            ));
        }
        
        // Validate search.default_limit is reasonable
        if self.search.default_limit == 0 {
            errors.push("search.default_limit must be greater than 0".to_string());
        }
        
        if !errors.is_empty() {
            crate::log_component_action!(
                "ConfigurationManager",
                "Configuration validation failed",
                error_count = errors.len(),
                errors = ?errors
            );
        } else {
            crate::log_component_action!(
                "ConfigurationManager",
                "Configuration validation passed"
            );
        }
        
        errors
    }
}

impl Config {
    /// Validate a raw JSON value for a given configuration key.
    /// This is useful for validating values before they are set,
    /// especially when parsing from JSON where type information matters.
    /// Returns a Vec of validation error messages (empty if valid).
    pub fn validate_value(key: &str, value: &serde_json::Value) -> Vec<String> {
        let mut errors = Vec::new();
        
        match key {
            "retention.days" => {
                match value {
                    serde_json::Value::Number(n) => {
                        if let Some(v) = n.as_i64() {
                            if v < 0 {
                                errors.push(format!(
                                    "retention.days must be a non-negative integer, got: {}",
                                    v
                                ));
                            }
                        } else if let Some(v) = n.as_f64() {
                            if v < 0.0 || v.fract() != 0.0 {
                                errors.push(format!(
                                    "retention.days must be a non-negative integer, got: {}",
                                    v
                                ));
                            }
                        }
                    }
                    _ => {
                        errors.push(format!(
                            "retention.days must be a non-negative integer, got: {}",
                            value
                        ));
                    }
                }
            }
            "monitoring.poll_interval" | "monitoring.pollInterval" => {
                match value {
                    serde_json::Value::Number(n) => {
                        if let Some(v) = n.as_i64() {
                            if v <= 0 {
                                errors.push(format!(
                                    "monitoring.poll_interval must be a positive integer, got: {}",
                                    v
                                ));
                            }
                        } else if let Some(v) = n.as_f64() {
                            if v <= 0.0 || v.fract() != 0.0 {
                                errors.push(format!(
                                    "monitoring.poll_interval must be a positive integer, got: {}",
                                    v
                                ));
                            }
                        }
                    }
                    _ => {
                        errors.push(format!(
                            "monitoring.poll_interval must be a positive integer, got: {}",
                            value
                        ));
                    }
                }
            }
            "privacy.enabled" => {
                if !value.is_boolean() {
                    errors.push(format!(
                        "privacy.enabled must be a boolean, got: {}",
                        value
                    ));
                }
            }
            "storage.data_dir" | "storage.dataDir" => {
                match value.as_str() {
                    Some(path_str) => {
                        let path = std::path::Path::new(path_str);
                        if !path.is_absolute() {
                            errors.push(format!(
                                "storage.data_dir must be an absolute path, got: {}",
                                path_str
                            ));
                        }
                    }
                    None => {
                        errors.push(format!(
                            "storage.data_dir must be a valid path string, got: {}",
                            value
                        ));
                    }
                }
            }
            _ => {} // Unknown keys are not validated here
        }
        
        errors
    }
}

impl Default for Config {
    fn default() -> Self {
        let base_dirs = BaseDirs::new().unwrap_or_else(|| panic!("Failed to get base directories"));
        
        let data_dir = if cfg!(target_os = "windows") {
            base_dirs.data_local_dir()
                .join("clipkeeper")
        } else if cfg!(target_os = "macos") {
            base_dirs.home_dir()
                .join("Library")
                .join("Application Support")
                .join("clipkeeper")
        } else {
            base_dirs.home_dir()
                .join(".local")
                .join("share")
                .join("clipkeeper")
        };
        
        let log_path = data_dir.join("clipkeeper.log");
        
        Config {
            version: "1.0".to_string(),
            privacy: PrivacyConfig {
                enabled: true,
                patterns: vec![],
                custom_patterns: vec![],
            },
            retention: RetentionConfig {
                days: 30,
            },
            monitoring: MonitoringConfig {
                poll_interval: 500,
                auto_start: false,
                enabled: true,
                max_metrics_log_kb: 1024,
            },
            storage: StorageConfig {
                data_dir,
                db_path: None, // Will default to data_dir/clipkeeper.db
                log_path,
            },
            search: SearchConfig {
                default_limit: 50,
            },
        }
    }
}
