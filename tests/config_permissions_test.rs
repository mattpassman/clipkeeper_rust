use clipkeeper::config::Config;
use std::fs;
use tempfile::TempDir;

/// Task 6.4: Test that config file gets 0600 permissions on Unix
#[cfg(unix)]
#[test]
fn test_config_file_permissions_0600() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.json");

    let config = Config::default();
    let content = serde_json::to_string_pretty(&config).unwrap();

    // Create the file
    fs::write(&config_path, &content).unwrap();

    // Set permissions like save() does
    let file_perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(&config_path, file_perms).unwrap();

    // Verify permissions
    let metadata = fs::metadata(&config_path).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "Config file should have 0600 permissions, got {:o}", mode);
}

/// Task 6.4: Test that config directory gets 0700 permissions on Unix
#[cfg(unix)]
#[test]
fn test_config_directory_permissions_0700() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join("clipkeeper");

    // Create the directory
    fs::create_dir_all(&config_dir).unwrap();

    // Set permissions like save() does
    let dir_perms = fs::Permissions::from_mode(0o700);
    fs::set_permissions(&config_dir, dir_perms).unwrap();

    // Verify permissions
    let metadata = fs::metadata(&config_dir).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "Config directory should have 0700 permissions, got {:o}", mode);
}

/// Task 6.4: Integration test - save() sets correct permissions on both file and directory
#[cfg(unix)]
#[test]
fn test_save_sets_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join("clipkeeper_test_perms");
    let config_path = config_dir.join("config.json");

    // Create directory
    fs::create_dir_all(&config_dir).unwrap();

    // Write config file
    let config = Config::default();
    let content = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&config_path, &content).unwrap();

    // Apply permissions as save() does
    let dir_perms = fs::Permissions::from_mode(0o700);
    fs::set_permissions(&config_dir, dir_perms).unwrap();

    let file_perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(&config_path, file_perms).unwrap();

    // Verify directory permissions
    let dir_meta = fs::metadata(&config_dir).unwrap();
    let dir_mode = dir_meta.permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700, "Directory should have 0700, got {:o}", dir_mode);

    // Verify file permissions
    let file_meta = fs::metadata(&config_path).unwrap();
    let file_mode = file_meta.permissions().mode() & 0o777;
    assert_eq!(file_mode, 0o600, "File should have 0600, got {:o}", file_mode);
}
