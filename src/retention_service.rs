use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::config::Config;
use crate::errors::Result;
use crate::history_store::HistoryStore;

/// Background service that periodically cleans up old clipboard entries
/// based on the configured retention period.
///
/// # Requirements
/// - 6.1: Schedule cleanup operations to run every hour
/// - 6.2: Check retention.days configuration value
/// - 6.3: Skip cleanup when retention.days is 0 (unlimited)
/// - 6.4: Delete entries older than the specified number of days
/// - 6.5: Log the count of deleted entries
/// - 6.6: Cancel all scheduled cleanup operations on stop
pub struct RetentionService {
    history_store: Arc<Mutex<HistoryStore>>,
    config: Arc<Mutex<Config>>,
    interval: Duration,
    shutdown_rx: mpsc::Receiver<()>,
}

impl RetentionService {
    /// Create a new RetentionService.
    ///
    /// # Arguments
    /// * `history_store` - Shared history store for database operations
    /// * `config` - Shared configuration for reading retention.days
    /// * `shutdown_rx` - Channel receiver for shutdown signal
    pub fn new(
        history_store: Arc<Mutex<HistoryStore>>,
        config: Arc<Mutex<Config>>,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            history_store,
            config,
            interval: Duration::from_secs(3600), // 1 hour
            shutdown_rx,
        }
    }

    /// Run the retention service in the current thread.
    ///
    /// This method loops with hourly sleeps, checking for shutdown signals
    /// between cleanup cycles. On each cycle it reads retention.days from
    /// config and deletes entries older than that threshold.
    pub fn run(self) -> Result<()> {
        crate::log_component_action!(
            "RetentionService",
            "Starting retention service",
            interval_secs = self.interval.as_secs()
        );

        loop {
            // Check for shutdown signal (non-blocking)
            match self.shutdown_rx.try_recv() {
                Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                    crate::log_component_action!(
                        "RetentionService",
                        "Shutdown signal received, stopping retention service"
                    );
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No shutdown signal, continue
                }
            }

            // Run cleanup
            match self.run_cleanup() {
                Ok(deleted) => {
                    if deleted > 0 {
                        crate::log_component_action!(
                            "RetentionService",
                            "Cleanup completed",
                            entries_deleted = deleted
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        component = "RetentionService",
                        error = %e,
                        "Retention cleanup failed"
                    );
                }
            }

            // Sleep for the interval, checking for shutdown periodically
            // Break the sleep into smaller chunks so we can respond to shutdown faster
            let check_interval = Duration::from_secs(1);
            let total_checks = self.interval.as_secs();
            for _ in 0..total_checks {
                match self.shutdown_rx.try_recv() {
                    Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                        crate::log_component_action!(
                            "RetentionService",
                            "Shutdown signal received during sleep, stopping retention service"
                        );
                        return Ok(());
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                thread::sleep(check_interval);
            }
        }

        crate::log_component_action!(
            "RetentionService",
            "Retention service stopped"
        );
        Ok(())
    }

    /// Run a single cleanup cycle.
    ///
    /// Reads retention.days from config. If 0, skips cleanup (unlimited retention).
    /// Otherwise, calls HistoryStore::cleanup_old_entries with the configured days.
    ///
    /// Returns the number of deleted entries (0 if skipped).
    fn run_cleanup(&self) -> Result<usize> {
        let retention_days = {
            let config = self.config.lock().map_err(|e| {
                anyhow::anyhow!("Failed to lock config: {}", e)
            })?;
            config.retention.days
        };

        // Skip cleanup if retention.days = 0 (unlimited)
        if retention_days == 0 {
            crate::log_component_action!(
                "RetentionService",
                "Skipping cleanup, retention is unlimited (days=0)"
            );
            return Ok(0);
        }

        crate::log_component_action!(
            "RetentionService",
            "Running cleanup",
            retention_days = retention_days
        );

        let store = self.history_store.lock().map_err(|e| {
            anyhow::anyhow!("Failed to lock history store: {}", e)
        })?;

        let deleted = store.cleanup_old_entries(retention_days)?;

        crate::log_component_action!(
            "RetentionService",
            "Entries deleted during cleanup",
            count = deleted,
            retention_days = retention_days
        );

        Ok(deleted)
    }
}

/// Spawn the retention service as a background thread.
///
/// # Arguments
/// * `history_store` - Shared history store
/// * `config` - Shared configuration
/// * `shutdown_rx` - Channel receiver for shutdown signal
///
/// # Returns
/// A JoinHandle for the spawned thread.
pub fn spawn_retention_service(
    history_store: Arc<Mutex<HistoryStore>>,
    config: Arc<Mutex<Config>>,
    shutdown_rx: mpsc::Receiver<()>,
) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let service = RetentionService::new(history_store, config, shutdown_rx);
        service.run()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_classifier::ContentType;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_store(dir: &Path) -> HistoryStore {
        let db_path = dir.join("test.db");
        HistoryStore::new(&db_path).expect("Failed to create test store")
    }

    fn create_test_config(retention_days: u32) -> Config {
        let mut config = Config::default();
        config.retention.days = retention_days;
        config
    }

    /// Insert an entry with a specific timestamp directly via SQL.
    /// This is needed for testing cleanup of old entries since
    /// HistoryStore::save auto-generates the current timestamp.
    fn insert_entry_with_timestamp(db_path: &Path, content: &str, timestamp: i64) {
        let conn = rusqlite::Connection::open(db_path).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let metadata = r#"{"language":null,"confidence":1.0,"character_count":10,"word_count":2}"#;
        conn.execute(
            "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            rusqlite::params![id, content, "text", timestamp, metadata],
        ).unwrap();
    }

    /// Count entries via a separate connection to avoid locking issues.
    fn count_entries(db_path: &Path) -> usize {
        let conn = rusqlite::Connection::open(db_path).unwrap();
        conn.query_row("SELECT COUNT(*) FROM clipboard_entries", [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn test_retention_service_creation() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(Mutex::new(create_test_store(tmp.path())));
        let config = Arc::new(Mutex::new(create_test_config(30)));
        let (_tx, rx) = mpsc::channel();

        let service = RetentionService::new(store, config, rx);
        assert_eq!(service.interval, Duration::from_secs(3600));
    }

    #[test]
    fn test_cleanup_skips_when_unlimited() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(Mutex::new(create_test_store(tmp.path())));
        let config = Arc::new(Mutex::new(create_test_config(0))); // unlimited
        let (_tx, rx) = mpsc::channel();

        let service = RetentionService::new(store, config, rx);
        let deleted = service.run_cleanup().unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_cleanup_runs_when_days_configured() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(Mutex::new(create_test_store(tmp.path())));
        let config = Arc::new(Mutex::new(create_test_config(30)));
        let (_tx, rx) = mpsc::channel();

        let service = RetentionService::new(store, config, rx);
        // No entries to delete, but should run without error
        let deleted = service.run_cleanup().unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_cleanup_deletes_old_entries() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Insert an old entry (100 days ago) directly via SQL
        let old_timestamp = chrono::Utc::now().timestamp_millis() - (100 * 24 * 60 * 60 * 1000);
        insert_entry_with_timestamp(&db_path, "old entry", old_timestamp);

        // Insert a recent entry via the normal API
        store.save("recent entry", ContentType::Text).unwrap();

        assert_eq!(count_entries(&db_path), 2);

        let store = Arc::new(Mutex::new(store));
        let config = Arc::new(Mutex::new(create_test_config(30))); // 30 days retention
        let (_tx, rx) = mpsc::channel();

        let service = RetentionService::new(Arc::clone(&store), config, rx);
        let deleted = service.run_cleanup().unwrap();

        assert_eq!(deleted, 1); // Only the old entry should be deleted
        assert_eq!(count_entries(&db_path), 1);
    }

    #[test]
    fn test_cleanup_keeps_entries_within_retention() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Insert entries that are within the retention period (5 days ago, retention = 30)
        let recent_timestamp = chrono::Utc::now().timestamp_millis() - (5 * 24 * 60 * 60 * 1000);
        insert_entry_with_timestamp(&db_path, "recent enough", recent_timestamp);
        store.save("brand new", ContentType::Text).unwrap();

        assert_eq!(count_entries(&db_path), 2);

        let store = Arc::new(Mutex::new(store));
        let config = Arc::new(Mutex::new(create_test_config(30)));
        let (_tx, rx) = mpsc::channel();

        let service = RetentionService::new(Arc::clone(&store), config, rx);
        let deleted = service.run_cleanup().unwrap();

        assert_eq!(deleted, 0); // Nothing should be deleted
        assert_eq!(count_entries(&db_path), 2);
    }

    #[test]
    fn test_graceful_shutdown() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(Mutex::new(create_test_store(tmp.path())));
        let config = Arc::new(Mutex::new(create_test_config(30)));
        let (tx, rx) = mpsc::channel();

        // Send shutdown signal immediately
        tx.send(()).unwrap();

        let service = RetentionService::new(store, config, rx);
        let result = service.run();
        assert!(result.is_ok());
    }

    #[test]
    fn test_spawn_retention_service_and_shutdown() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(Mutex::new(create_test_store(tmp.path())));
        let config = Arc::new(Mutex::new(create_test_config(30)));
        let (tx, rx) = mpsc::channel();

        let handle = spawn_retention_service(store, config, rx);

        // Send shutdown signal
        tx.send(()).unwrap();

        // Thread should complete
        let result = handle.join().expect("Thread panicked");
        assert!(result.is_ok());
    }

    #[test]
    fn test_shutdown_via_dropped_sender() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(Mutex::new(create_test_store(tmp.path())));
        let config = Arc::new(Mutex::new(create_test_config(30)));
        let (tx, rx) = mpsc::channel();

        // Drop the sender to trigger Disconnected
        drop(tx);

        let service = RetentionService::new(store, config, rx);
        let result = service.run();
        assert!(result.is_ok());
    }
}
