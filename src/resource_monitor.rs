use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sysinfo::{Pid, System};

use crate::errors::Result;
use crate::history_store::HistoryStore;

/// Maximum number of metric samples to keep in memory.
const MAX_METRICS_HISTORY: usize = 100;

/// Default collection interval in seconds.
const DEFAULT_INTERVAL_SECS: u64 = 60;

/// A single snapshot of resource metrics.
///
/// # Requirements
/// - 15.1: Collect metrics at configurable intervals (default 60 seconds)
/// - 15.2: Record memory usage (RSS)
/// - 15.3: Record CPU usage percentage
/// - 15.4: Record database size and entry count
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Unix timestamp in milliseconds when metrics were collected.
    pub timestamp: i64,
    /// Resident Set Size (RSS) memory in bytes.
    pub memory_rss_bytes: u64,
    /// CPU usage as a percentage (0.0 - 100.0+).
    pub cpu_usage_percent: f32,
    /// Database file size in bytes.
    pub database_size_bytes: u64,
    /// Total number of clipboard entries in the database.
    pub entry_count: usize,
}

/// Shared metrics storage accessible from multiple threads.
pub type SharedMetrics = Arc<Mutex<VecDeque<Metrics>>>;

/// ResourceMonitor tracks memory, CPU, database size, and entry count.
///
/// It runs as a background thread, collecting metrics at a configurable
/// interval and storing the last 100 samples in memory.
///
/// # Requirements
/// - 15.1: Collect metrics at configurable intervals (default 60 seconds)
/// - 15.2: Record memory usage (RSS)
/// - 15.3: Record CPU usage percentage
/// - 15.4: Record database size and entry count
/// - 15.5: Display current resource usage statistics
/// - 15.6: Cease all metrics collection on stop
pub struct ResourceMonitor {
    interval: Duration,
    history_store: Arc<Mutex<HistoryStore>>,
    db_path: PathBuf,
    metrics_path: Option<PathBuf>,
    metrics: SharedMetrics,
    shutdown_rx: mpsc::Receiver<()>,
    start_time: std::time::Instant,
}

impl ResourceMonitor {
    /// Create a new ResourceMonitor.
    ///
    /// # Arguments
    /// * `history_store` - Shared history store for entry count queries
    /// * `db_path` - Path to the SQLite database file (for size measurement)
    /// * `metrics` - Shared metrics storage for retrieval by other components
    /// * `shutdown_rx` - Channel receiver for shutdown signal
    pub fn new(
        history_store: Arc<Mutex<HistoryStore>>,
        db_path: PathBuf,
        metrics: SharedMetrics,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            interval: Duration::from_secs(DEFAULT_INTERVAL_SECS),
            history_store,
            db_path: db_path.clone(),
            metrics_path: db_path.parent().map(|p| p.join("metrics.log")),
            metrics,
            shutdown_rx,
            start_time: std::time::Instant::now(),
        }
    }

    /// Create a new ResourceMonitor with a custom collection interval.
    pub fn with_interval(
        history_store: Arc<Mutex<HistoryStore>>,
        db_path: PathBuf,
        metrics: SharedMetrics,
        shutdown_rx: mpsc::Receiver<()>,
        interval: Duration,
    ) -> Self {
        Self {
            interval,
            history_store,
            db_path: db_path.clone(),
            metrics_path: db_path.parent().map(|p| p.join("metrics.log")),
            metrics,
            shutdown_rx,
            start_time: std::time::Instant::now(),
        }
    }

    /// Run the resource monitor in the current thread.
    ///
    /// Collects metrics immediately on start, then every `interval` seconds.
    /// Checks for shutdown signals between collection cycles.
    pub fn run(self) -> Result<()> {
        crate::log_component_action!(
            "ResourceMonitor",
            "Starting resource monitor",
            interval_secs = self.interval.as_secs()
        );

        let mut sys = System::new();
        let pid = Pid::from_u32(std::process::id());

        // Collect initial metrics
        self.collect_and_store(&mut sys, pid);

        loop {
            // Sleep in 1-second chunks so we can respond to shutdown quickly
            let total_checks = self.interval.as_secs();
            for _ in 0..total_checks {
                match self.shutdown_rx.try_recv() {
                    Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                        crate::log_component_action!(
                            "ResourceMonitor",
                            "Shutdown signal received, stopping resource monitor"
                        );
                        return Ok(());
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                thread::sleep(Duration::from_secs(1));
            }

            // Collect metrics after the interval
            self.collect_and_store(&mut sys, pid);
        }
    }

    /// Collect a single metrics snapshot and store it.
    fn collect_and_store(&self, sys: &mut System, pid: Pid) {
        match self.collect_metrics(sys, pid) {
            Ok(metrics) => {
                // Write to metrics log file (like JS version)
                if let Some(ref metrics_path) = self.metrics_path {
                    let uptime_secs = self.start_time.elapsed().as_secs();
                    let datetime = chrono::Utc::now().to_rfc3339();
                    let log_entry = serde_json::json!({
                        "timestamp": metrics.timestamp,
                        "datetime": datetime,
                        "uptime_secs": uptime_secs,
                        "memory_rss_mb": (metrics.memory_rss_bytes as f64 / 1024.0 / 1024.0 * 100.0).round() / 100.0,
                        "cpu_usage_percent": (metrics.cpu_usage_percent * 100.0).round() / 100.0,
                        "database_size_kb": (metrics.database_size_bytes as f64 / 1024.0 * 100.0).round() / 100.0,
                        "entry_count": metrics.entry_count,
                        "system": {
                            "platform": std::env::consts::OS,
                            "arch": std::env::consts::ARCH,
                        }
                    });
                    if let Ok(line) = serde_json::to_string(&log_entry) {
                        use std::io::Write;
                        if let Ok(mut file) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(metrics_path)
                        {
                            let _ = writeln!(file, "{}", line);
                        }
                    }
                }

                let mut history = match self.metrics.lock() {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::error!(
                            component = "ResourceMonitor",
                            error = %e,
                            "Failed to lock metrics storage"
                        );
                        return;
                    }
                };

                // Maintain max history size
                if history.len() >= MAX_METRICS_HISTORY {
                    history.pop_front();
                }
                history.push_back(metrics);
            }
            Err(e) => {
                tracing::error!(
                    component = "ResourceMonitor",
                    error = %e,
                    "Failed to collect metrics"
                );
            }
        }
    }

    /// Collect current resource metrics.
    fn collect_metrics(&self, sys: &mut System, pid: Pid) -> Result<Metrics> {
        // Refresh process-specific information
        sys.refresh_process(pid);

        let (memory_rss_bytes, cpu_usage_percent) = if let Some(process) = sys.process(pid) {
            (process.memory(), process.cpu_usage())
        } else {
            (0, 0.0)
        };

        // Get database file size
        let database_size_bytes = std::fs::metadata(&self.db_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // Get entry count from history store
        let entry_count = match self.history_store.lock() {
            Ok(store) => store.get_statistics().map(|s| s.total).unwrap_or(0),
            Err(_) => 0,
        };

        let timestamp = chrono::Utc::now().timestamp_millis();

        Ok(Metrics {
            timestamp,
            memory_rss_bytes,
            cpu_usage_percent,
            database_size_bytes,
            entry_count,
        })
    }
}

/// Get the current (most recent) metrics snapshot.
///
/// # Requirements
/// - 15.5: Display current resource usage statistics
pub fn get_current_metrics(metrics: &SharedMetrics) -> Option<Metrics> {
    metrics
        .lock()
        .ok()
        .and_then(|history| history.back().cloned())
}

/// Get the full metrics history (up to the last 100 samples).
///
/// # Requirements
/// - 15.1-15.5: Metrics collection and retrieval
pub fn get_metrics_history(metrics: &SharedMetrics) -> Vec<Metrics> {
    metrics
        .lock()
        .map(|history| history.iter().cloned().collect())
        .unwrap_or_default()
}

/// Create a new empty shared metrics storage.
pub fn new_shared_metrics() -> SharedMetrics {
    Arc::new(Mutex::new(VecDeque::with_capacity(MAX_METRICS_HISTORY)))
}

/// Spawn the resource monitor as a background thread.
///
/// # Arguments
/// * `history_store` - Shared history store
/// * `db_path` - Path to the database file
/// * `metrics` - Shared metrics storage
/// * `shutdown_rx` - Channel receiver for shutdown signal
///
/// # Returns
/// A JoinHandle for the spawned thread.
pub fn spawn_resource_monitor(
    history_store: Arc<Mutex<HistoryStore>>,
    db_path: PathBuf,
    metrics: SharedMetrics,
    shutdown_rx: mpsc::Receiver<()>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let monitor = ResourceMonitor::new(history_store, db_path, metrics, shutdown_rx);
        if let Err(e) = monitor.run() {
            tracing::error!(
                component = "ResourceMonitor",
                error = %e,
                "Resource monitor thread exited with error"
            );
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_store::HistoryStore;
    use tempfile::TempDir;

    fn create_test_store(dir: &std::path::Path) -> HistoryStore {
        let db_path = dir.join("test.db");
        HistoryStore::new(&db_path).unwrap()
    }

    #[test]
    fn test_new_shared_metrics_is_empty() {
        let metrics = new_shared_metrics();
        let history = metrics.lock().unwrap();
        assert!(history.is_empty());
        assert_eq!(history.capacity(), MAX_METRICS_HISTORY);
    }

    #[test]
    fn test_get_current_metrics_empty() {
        let metrics = new_shared_metrics();
        assert!(get_current_metrics(&metrics).is_none());
    }

    #[test]
    fn test_get_current_metrics_returns_latest() {
        let metrics = new_shared_metrics();
        {
            let mut history = metrics.lock().unwrap();
            history.push_back(Metrics {
                timestamp: 1000,
                memory_rss_bytes: 100,
                cpu_usage_percent: 1.0,
                database_size_bytes: 200,
                entry_count: 5,
            });
            history.push_back(Metrics {
                timestamp: 2000,
                memory_rss_bytes: 150,
                cpu_usage_percent: 2.5,
                database_size_bytes: 300,
                entry_count: 10,
            });
        }

        let current = get_current_metrics(&metrics).unwrap();
        assert_eq!(current.timestamp, 2000);
        assert_eq!(current.memory_rss_bytes, 150);
        assert_eq!(current.entry_count, 10);
    }

    #[test]
    fn test_get_metrics_history_empty() {
        let metrics = new_shared_metrics();
        let history = get_metrics_history(&metrics);
        assert!(history.is_empty());
    }

    #[test]
    fn test_get_metrics_history_returns_all() {
        let metrics = new_shared_metrics();
        {
            let mut history = metrics.lock().unwrap();
            for i in 0..5 {
                history.push_back(Metrics {
                    timestamp: i * 1000,
                    memory_rss_bytes: 100 + i as u64,
                    cpu_usage_percent: i as f32,
                    database_size_bytes: 200,
                    entry_count: i as usize,
                });
            }
        }

        let history = get_metrics_history(&metrics);
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].timestamp, 0);
        assert_eq!(history[4].timestamp, 4000);
    }

    #[test]
    fn test_metrics_history_max_size() {
        let metrics = new_shared_metrics();
        {
            let mut history = metrics.lock().unwrap();
            for i in 0..150 {
                if history.len() >= MAX_METRICS_HISTORY {
                    history.pop_front();
                }
                history.push_back(Metrics {
                    timestamp: i * 1000,
                    memory_rss_bytes: 100,
                    cpu_usage_percent: 1.0,
                    database_size_bytes: 200,
                    entry_count: 0,
                });
            }
        }

        let history = get_metrics_history(&metrics);
        assert_eq!(history.len(), MAX_METRICS_HISTORY);
        // Should contain the last 100 samples (50..150)
        assert_eq!(history[0].timestamp, 50 * 1000);
        assert_eq!(history[99].timestamp, 149 * 1000);
    }

    #[test]
    fn test_resource_monitor_creation() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(temp_dir.path());
        let db_path = temp_dir.path().join("test.db");
        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (_tx, rx) = mpsc::channel();

        let monitor = ResourceMonitor::new(
            shared_store,
            db_path.clone(),
            metrics,
            rx,
        );

        assert_eq!(monitor.interval, Duration::from_secs(DEFAULT_INTERVAL_SECS));
        assert_eq!(monitor.db_path, db_path);
    }

    #[test]
    fn test_resource_monitor_with_custom_interval() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(temp_dir.path());
        let db_path = temp_dir.path().join("test.db");
        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (_tx, rx) = mpsc::channel();

        let monitor = ResourceMonitor::with_interval(
            shared_store,
            db_path,
            metrics,
            rx,
            Duration::from_secs(30),
        );

        assert_eq!(monitor.interval, Duration::from_secs(30));
    }

    #[test]
    fn test_collect_metrics() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(temp_dir.path());
        let db_path = temp_dir.path().join("test.db");

        // Save some entries so entry_count > 0
        store.save("hello world", "text").unwrap();
        store.save("https://example.com", "url").unwrap();

        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (_tx, rx) = mpsc::channel();

        let monitor = ResourceMonitor::new(
            shared_store,
            db_path,
            metrics,
            rx,
        );

        let mut sys = System::new();
        let pid = Pid::from_u32(std::process::id());
        let result = monitor.collect_metrics(&mut sys, pid).unwrap();

        assert!(result.timestamp > 0);
        // RSS should be non-zero for a running process
        // (may be 0 on some CI environments, so we just check it doesn't error)
        assert!(result.database_size_bytes > 0);
        assert_eq!(result.entry_count, 2);
    }

    #[test]
    fn test_collect_and_store_adds_to_history() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(temp_dir.path());
        let db_path = temp_dir.path().join("test.db");
        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (_tx, rx) = mpsc::channel();

        let monitor = ResourceMonitor::new(
            shared_store,
            db_path,
            Arc::clone(&metrics),
            rx,
        );

        let mut sys = System::new();
        let pid = Pid::from_u32(std::process::id());

        monitor.collect_and_store(&mut sys, pid);

        let history = get_metrics_history(&metrics);
        assert_eq!(history.len(), 1);
        assert!(history[0].timestamp > 0);
    }

    #[test]
    fn test_graceful_shutdown() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(temp_dir.path());
        let db_path = temp_dir.path().join("test.db");
        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (tx, rx) = mpsc::channel();

        let handle = spawn_resource_monitor(
            shared_store,
            db_path,
            Arc::clone(&metrics),
            rx,
        );

        // Give it a moment to start and collect initial metrics
        thread::sleep(Duration::from_millis(200));

        // Send shutdown signal
        tx.send(()).unwrap();

        // Thread should exit promptly
        handle.join().expect("Resource monitor thread panicked");

        // Should have at least the initial metrics sample
        let history = get_metrics_history(&metrics);
        assert!(!history.is_empty(), "Should have collected at least one sample");
    }

    #[test]
    fn test_shutdown_via_dropped_sender() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(temp_dir.path());
        let db_path = temp_dir.path().join("test.db");
        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (tx, rx) = mpsc::channel();

        let handle = spawn_resource_monitor(
            shared_store,
            db_path,
            Arc::clone(&metrics),
            rx,
        );

        // Give it a moment to start
        thread::sleep(Duration::from_millis(200));

        // Drop the sender - this disconnects the channel and triggers shutdown
        drop(tx);

        // Thread should exit promptly
        handle.join().expect("Resource monitor thread panicked");
    }

    #[test]
    fn test_database_size_for_nonexistent_path() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(temp_dir.path());
        let nonexistent_path = temp_dir.path().join("nonexistent.db");
        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (_tx, rx) = mpsc::channel();

        let monitor = ResourceMonitor::new(
            shared_store,
            nonexistent_path,
            metrics,
            rx,
        );

        let mut sys = System::new();
        let pid = Pid::from_u32(std::process::id());
        let result = monitor.collect_metrics(&mut sys, pid).unwrap();

        // Should gracefully return 0 for nonexistent file
        assert_eq!(result.database_size_bytes, 0);
    }
}
