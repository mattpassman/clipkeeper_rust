use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::errors::Result;
use crate::history_store::HistoryStore;

/// Maximum number of metric samples to keep in memory.
const MAX_METRICS_HISTORY: usize = 100;

/// Default collection interval in seconds.
const DEFAULT_INTERVAL_SECS: u64 = 300;

/// A single snapshot of resource metrics.
///
/// # Requirements
/// - 15.1: Collect metrics at configurable intervals (default 300 seconds)
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

// ---------------------------------------------------------------------------
// Lightweight /proc/self/stat reader (Linux only, no sysinfo dependency)
// ---------------------------------------------------------------------------

/// Read RSS (in bytes) and total CPU ticks from /proc/self/stat.
/// Returns (rss_bytes, utime + stime) or zeros on failure.
#[cfg(target_os = "linux")]
fn read_proc_self_stat() -> (u64, u64) {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return (0, 0);
    };
    // Fields are space-separated. comm (field 2) may contain spaces and is
    // wrapped in parens, so split after the closing ')'.
    let Some(rest) = stat.rfind(')').map(|i| &stat[i + 2..]) else {
        return (0, 0);
    };
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After the ')' and the space:
    //   index 0 = state, 1 = ppid, …, 11 = utime, 12 = stime, …, 21 = rss (pages)
    let utime: u64 = fields.get(11).and_then(|v| v.parse().ok()).unwrap_or(0);
    let stime: u64 = fields.get(12).and_then(|v| v.parse().ok()).unwrap_or(0);
    let rss_pages: u64 = fields.get(21).and_then(|v| v.parse().ok()).unwrap_or(0);
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
    (rss_pages * page_size, utime + stime)
}

/// Estimate CPU usage % between two snapshots.
#[cfg(target_os = "linux")]
fn cpu_percent_between(prev_ticks: u64, cur_ticks: u64, elapsed: Duration) -> f32 {
    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    if ticks_per_sec <= 0.0 || elapsed.as_secs_f64() <= 0.0 {
        return 0.0;
    }
    let delta_secs = (cur_ticks.saturating_sub(prev_ticks)) as f64 / ticks_per_sec;
    (delta_secs / elapsed.as_secs_f64() * 100.0) as f32
}

/// Read total and available system memory from /proc/meminfo (in bytes).
#[cfg(target_os = "linux")]
pub fn read_meminfo() -> (u64, u64) {
    let Ok(content) = std::fs::read_to_string("/proc/meminfo") else {
        return (0, 0);
    };
    let mut total: u64 = 0;
    let mut available: u64 = 0;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = parse_meminfo_kb(rest) * 1024;
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available = parse_meminfo_kb(rest) * 1024;
        }
        if total > 0 && available > 0 {
            break;
        }
    }
    (total, available)
}

#[cfg(target_os = "linux")]
fn parse_meminfo_kb(s: &str) -> u64 {
    s.trim().trim_end_matches("kB").trim().parse().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Non-Linux: use sysinfo crate for macOS / Windows
// ---------------------------------------------------------------------------

/// Read RSS (in bytes) and a dummy tick counter via sysinfo.
/// The tick value is not meaningful on non-Linux, so CPU % will be
/// derived from sysinfo's own `cpu_usage()` instead.
#[cfg(not(target_os = "linux"))]
fn read_proc_self_stat() -> (u64, u64) {
    use sysinfo::{Pid, System};
    let mut sys = System::new();
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_process(pid);
    let rss = sys.process(pid).map(|p| p.memory()).unwrap_or(0);
    (rss, 0)
}

/// On non-Linux we ignore the tick-based calculation and ask sysinfo directly.
#[cfg(not(target_os = "linux"))]
fn cpu_percent_between(_prev: u64, _cur: u64, _elapsed: Duration) -> f32 {
    use sysinfo::{Pid, System};
    let mut sys = System::new();
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_process(pid);
    sys.process(pid).map(|p| p.cpu_usage()).unwrap_or(0.0)
}

/// Read total and available system memory via sysinfo.
#[cfg(not(target_os = "linux"))]
pub fn read_meminfo() -> (u64, u64) {
    use sysinfo::System;
    let sys = System::new_all();
    (sys.total_memory(), sys.available_memory())
}

// ---------------------------------------------------------------------------
// ResourceMonitor
// ---------------------------------------------------------------------------

/// ResourceMonitor tracks memory, CPU, database size, and entry count.
///
/// On Linux it reads /proc/self/stat directly — no sysinfo crate needed.
/// On macOS and Windows the sysinfo crate is pulled in automatically via
/// `cfg(not(target_os = "linux"))` conditional compilation.
///
/// # Requirements
/// - 15.1: Collect metrics at configurable intervals (default 300 seconds)
/// - 15.2: Record memory usage (RSS)
/// - 15.3: Record CPU usage percentage
/// - 15.4: Record database size and entry count
/// - 15.5: Display current resource usage statistics
/// - 15.6: Cease all metrics collection on stop
pub struct ResourceMonitor {
    interval: Duration,
    entry_count: Arc<AtomicUsize>,
    db_path: PathBuf,
    metrics_path: Option<PathBuf>,
    metrics: SharedMetrics,
    shutdown_rx: mpsc::Receiver<()>,
    start_time: std::time::Instant,
}

impl ResourceMonitor {
    /// Create a new ResourceMonitor with the default 300s interval.
    pub fn new(
        history_store: Arc<Mutex<HistoryStore>>,
        db_path: PathBuf,
        metrics: SharedMetrics,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Self {
        let entry_count = history_store.lock()
            .map(|s| s.entry_count_handle())
            .unwrap_or_else(|_| Arc::new(AtomicUsize::new(0)));
        Self {
            interval: Duration::from_secs(DEFAULT_INTERVAL_SECS),
            entry_count,
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
        let entry_count = history_store.lock()
            .map(|s| s.entry_count_handle())
            .unwrap_or_else(|_| Arc::new(AtomicUsize::new(0)));
        Self {
            interval,
            entry_count,
            db_path: db_path.clone(),
            metrics_path: db_path.parent().map(|p| p.join("metrics.log")),
            metrics,
            shutdown_rx,
            start_time: std::time::Instant::now(),
        }
    }

    /// Run the resource monitor in the current thread.
    pub fn run(self) -> Result<()> {
        crate::log_component_action!(
            "ResourceMonitor",
            "Starting resource monitor",
            interval_secs = self.interval.as_secs()
        );

        let mut prev_ticks: u64 = 0;
        let mut prev_instant = std::time::Instant::now();

        // Collect initial metrics
        self.collect_and_store(&mut prev_ticks, &mut prev_instant);

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

            self.collect_and_store(&mut prev_ticks, &mut prev_instant);
        }
    }

    /// Collect a single metrics snapshot and store it.
    fn collect_and_store(&self, prev_ticks: &mut u64, prev_instant: &mut std::time::Instant) {
        match self.collect_metrics(prev_ticks, prev_instant) {
            Ok(metrics) => {
                // Write to metrics log file
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
    fn collect_metrics(
        &self,
        prev_ticks: &mut u64,
        prev_instant: &mut std::time::Instant,
    ) -> Result<Metrics> {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(*prev_instant);

        let (rss_bytes, cur_ticks) = read_proc_self_stat();
        let cpu = cpu_percent_between(*prev_ticks, cur_ticks, elapsed);

        *prev_ticks = cur_ticks;
        *prev_instant = now;

        let database_size_bytes = std::fs::metadata(&self.db_path)
            .map(|m| m.len())
            .unwrap_or(0);

        let entry_count = self.entry_count.load(Ordering::Relaxed);

        let timestamp = chrono::Utc::now().timestamp_millis();

        Ok(Metrics {
            timestamp,
            memory_rss_bytes: rss_bytes,
            cpu_usage_percent: cpu,
            database_size_bytes,
            entry_count,
        })
    }
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Get the current (most recent) metrics snapshot.
pub fn get_current_metrics(metrics: &SharedMetrics) -> Option<Metrics> {
    metrics
        .lock()
        .ok()
        .and_then(|history| history.back().cloned())
}

/// Get the full metrics history (up to the last 100 samples).
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

        store.save("hello world", "text").unwrap();
        store.save("https://example.com", "url").unwrap();

        let shared_store = Arc::new(Mutex::new(store));
        let metrics = new_shared_metrics();
        let (_tx, rx) = mpsc::channel();

        let monitor = ResourceMonitor::new(shared_store, db_path, metrics, rx);

        let mut prev_ticks: u64 = 0;
        let mut prev_instant = std::time::Instant::now();
        let result = monitor.collect_metrics(&mut prev_ticks, &mut prev_instant).unwrap();

        assert!(result.timestamp > 0);
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

        let monitor = ResourceMonitor::new(shared_store, db_path, Arc::clone(&metrics), rx);

        let mut prev_ticks: u64 = 0;
        let mut prev_instant = std::time::Instant::now();
        monitor.collect_and_store(&mut prev_ticks, &mut prev_instant);

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

        thread::sleep(Duration::from_millis(200));
        tx.send(()).unwrap();
        handle.join().expect("Resource monitor thread panicked");

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

        thread::sleep(Duration::from_millis(200));
        drop(tx);
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

        let monitor = ResourceMonitor::new(shared_store, nonexistent_path, metrics, rx);

        let mut prev_ticks: u64 = 0;
        let mut prev_instant = std::time::Instant::now();
        let result = monitor.collect_metrics(&mut prev_ticks, &mut prev_instant).unwrap();

        assert_eq!(result.database_size_bytes, 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_read_proc_self_stat() {
        let (rss, ticks) = read_proc_self_stat();
        // A running process should have non-zero RSS
        assert!(rss > 0, "RSS should be non-zero on Linux");
        // ticks may be very small for a short-lived test, but should not panic
        let _ = ticks;
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_read_meminfo() {
        let (total, available) = read_meminfo();
        assert!(total > 0, "Total memory should be non-zero");
        assert!(available > 0, "Available memory should be non-zero");
        assert!(available <= total, "Available should not exceed total");
    }
}
