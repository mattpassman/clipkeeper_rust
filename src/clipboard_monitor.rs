use arboard::Clipboard;
use std::hash::{BuildHasher, Hasher};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use crate::errors::{Result, ClipboardError};

/// Event emitted when clipboard content changes
#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    pub content: String,
    pub timestamp: i64,
}

/// Monitors system clipboard for changes and emits events
pub struct ClipboardMonitor {
    poll_interval: Duration,
    tx: mpsc::Sender<ClipboardEvent>,
    shutdown_rx: mpsc::Receiver<()>,
}

impl ClipboardMonitor {
    /// Create a new clipboard monitor
    /// 
    /// # Arguments
    /// * `poll_interval` - Duration between clipboard checks
    /// * `tx` - Channel sender for clipboard change events
    /// * `shutdown_rx` - Channel receiver for shutdown signal
    pub fn new(
        poll_interval: Duration,
        tx: mpsc::Sender<ClipboardEvent>,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            poll_interval,
            tx,
            shutdown_rx,
        }
    }

    /// Run the clipboard monitor in the current thread
    /// 
    /// This method polls the clipboard at regular intervals and emits events
    /// when content changes. It runs until a shutdown signal is received.
    pub fn run(self) -> Result<()> {
        tracing::info!(component = "ClipboardMonitor", "Starting clipboard monitor");
        
        let mut last_state: Option<(usize, u64)> = None;
        
        loop {
            // Check for shutdown signal (non-blocking)
            match self.shutdown_rx.try_recv() {
                Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::info!(component = "ClipboardMonitor", "Shutdown signal received, stopping monitor");
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No shutdown signal, continue monitoring
                }
            }

            // Check clipboard for changes
            match self.check_clipboard(&mut last_state) {
                Ok(Some(event)) => {
                    tracing::info!(
                        component = "ClipboardMonitor",
                        content_length = event.content.len(),
                        timestamp = event.timestamp,
                        "Clipboard change detected"
                    );
                    
                    // Send event through channel
                    if let Err(e) = self.tx.send(event) {
                        tracing::error!(
                            component = "ClipboardMonitor",
                            error = %e,
                            "Failed to send clipboard event"
                        );
                        break;
                    }
                }
                Ok(None) => {
                    // No change detected
                }
                Err(e) => {
                    tracing::error!(
                        component = "ClipboardMonitor",
                        error = %e,
                        "Clipboard monitor error"
                    );
                }
            }

            // Sleep until next poll
            thread::sleep(self.poll_interval);
        }

        tracing::info!(component = "ClipboardMonitor", "Clipboard monitor stopped");
        Ok(())
    }

    /// Check clipboard for changes
    /// 
    /// Returns Some(ClipboardEvent) if content changed, None otherwise.
    /// Short-circuits on content length before hashing for performance.
    fn check_clipboard(&self, last_state: &mut Option<(usize, u64)>) -> Result<Option<ClipboardEvent>> {
        // Try to read clipboard with retry logic
        let content = match self.read_clipboard_with_retry() {
            Ok(content) => content,
            Err(e) => {
                // Log error but don't propagate - continue monitoring
                tracing::warn!(
                    component = "ClipboardMonitor",
                    error = %e,
                    "Failed to read clipboard after retry"
                );
                return Ok(None);
            }
        };

        let len = content.len();

        // Fast path: if length matches previous, compute hash to confirm
        let changed = match *last_state {
            Some((prev_len, prev_hash)) => {
                if len != prev_len {
                    true
                } else {
                    Self::calculate_hash(&content) != prev_hash
                }
            }
            None => true,
        };

        if changed {
            let hash = Self::calculate_hash(&content);
            *last_state = Some((len, hash));
            
            let event = ClipboardEvent {
                content,
                timestamp: crate::time_utils::now_millis(),
            };
            
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    /// Read clipboard with retry logic
    /// 
    /// Retries once after 100ms delay if access is denied
    fn read_clipboard_with_retry(&self) -> Result<String> {
        match self.read_clipboard() {
            Ok(content) => Ok(content),
            Err(ClipboardError::AccessDenied) => {
                // Retry once after 100ms delay
                tracing::debug!(
                    component = "ClipboardMonitor",
                    "Clipboard access denied, retrying after 100ms"
                );
                thread::sleep(Duration::from_millis(100));
                self.read_clipboard().map_err(|e| e.into())
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Read current clipboard content
    fn read_clipboard(&self) -> std::result::Result<String, ClipboardError> {
        let mut clipboard = Clipboard::new()
            .map_err(|e| ClipboardError::Arboard(e.to_string()))?;

        clipboard
            .get_text()
            .map_err(|e| {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("access") || err_str.contains("denied") || err_str.contains("permission") {
                    ClipboardError::AccessDenied
                } else {
                    ClipboardError::Arboard(e.to_string())
                }
            })
    }

    /// Calculate fast non-cryptographic hash of content using ahash.
    /// Used only for change detection, not security.
    fn calculate_hash(content: &str) -> u64 {
        let build_hasher = ahash::RandomState::with_seeds(0, 0, 0, 0);
        let mut hasher = build_hasher.build_hasher();
        hasher.write(content.as_bytes());
        hasher.finish()
    }
}

/// Spawn clipboard monitor as a background thread
/// 
/// # Arguments
/// * `poll_interval` - Duration between clipboard checks
/// * `tx` - Channel sender for clipboard change events
/// * `shutdown_rx` - Channel receiver for shutdown signal
/// 
/// # Returns
/// JoinHandle for the spawned thread
pub fn spawn_monitor(
    poll_interval: Duration,
    tx: mpsc::Sender<ClipboardEvent>,
    shutdown_rx: mpsc::Receiver<()>,
) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let monitor = ClipboardMonitor::new(poll_interval, tx, shutdown_rx);
        monitor.run()
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn test_calculate_hash() {
        let content1 = "Hello, World!";
        let content2 = "Hello, World!";
        let content3 = "Different content";

        let hash1 = ClipboardMonitor::calculate_hash(content1);
        let hash2 = ClipboardMonitor::calculate_hash(content2);
        let hash3 = ClipboardMonitor::calculate_hash(content3);

        // Same content should produce same hash
        assert_eq!(hash1, hash2);
        
        // Different content should produce different hash
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_clipboard_event_creation() {
        let content = "Test content".to_string();
        let timestamp = crate::time_utils::now_millis();
        
        let event = ClipboardEvent {
            content: content.clone(),
            timestamp,
        };

        assert_eq!(event.content, content);
        assert_eq!(event.timestamp, timestamp);
    }

    #[test]
    fn test_monitor_creation() {
        let (tx, _rx) = mpsc::channel();
        let (_shutdown_tx, shutdown_rx) = mpsc::channel();
        let poll_interval = Duration::from_millis(500);

        let monitor = ClipboardMonitor::new(poll_interval, tx, shutdown_rx);

        assert_eq!(monitor.poll_interval, poll_interval);
    }

    #[test]
    fn test_monitor_shutdown_signal() {
        let (tx, rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let poll_interval = Duration::from_millis(100);

        // Spawn monitor in background thread
        let handle = std::thread::spawn(move || {
            let monitor = ClipboardMonitor::new(poll_interval, tx, shutdown_rx);
            monitor.run()
        });

        // Give monitor time to start
        std::thread::sleep(Duration::from_millis(50));

        // Send shutdown signal
        shutdown_tx.send(()).expect("Failed to send shutdown signal");

        // Wait for monitor to stop
        let result = handle.join().expect("Monitor thread panicked");
        
        // Monitor should stop gracefully
        assert!(result.is_ok());

        // Channel should be closed (no more events)
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_spawn_monitor() {
        let (tx, _rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let poll_interval = Duration::from_millis(100);

        // Spawn monitor
        let handle = spawn_monitor(poll_interval, tx, shutdown_rx);

        // Give monitor time to start
        std::thread::sleep(Duration::from_millis(50));

        // Send shutdown signal
        shutdown_tx.send(()).expect("Failed to send shutdown signal");

        // Wait for monitor to stop
        let result = handle.join().expect("Monitor thread panicked");
        
        // Monitor should stop gracefully
        assert!(result.is_ok());
    }

    #[test]
    fn test_hash_change_detection() {
        let content1 = "First content";
        let content2 = "Second content";
        let content3 = "First content"; // Same as content1

        let hash1 = ClipboardMonitor::calculate_hash(content1);
        let hash2 = ClipboardMonitor::calculate_hash(content2);
        let hash3 = ClipboardMonitor::calculate_hash(content3);

        // Different content should have different hashes
        assert_ne!(hash1, hash2);
        
        // Same content should have same hash (deterministic)
        assert_eq!(hash1, hash3);
    }

    #[test]
    fn test_clipboard_event_timestamp() {
        let before = crate::time_utils::now_millis();
        
        let event = ClipboardEvent {
            content: "Test".to_string(),
            timestamp: crate::time_utils::now_millis(),
        };
        
        let after = crate::time_utils::now_millis();

        // Timestamp should be between before and after
        assert!(event.timestamp >= before);
        assert!(event.timestamp <= after);
    }

    #[test]
    fn test_monitor_poll_interval() {
        let (tx, _rx) = mpsc::channel();
        let (_shutdown_tx, shutdown_rx) = mpsc::channel();
        
        let poll_interval_100ms = Duration::from_millis(100);
        let monitor_100 = ClipboardMonitor::new(poll_interval_100ms, tx.clone(), shutdown_rx);
        assert_eq!(monitor_100.poll_interval, Duration::from_millis(100));

        let (_shutdown_tx2, shutdown_rx2) = mpsc::channel();
        let poll_interval_500ms = Duration::from_millis(500);
        let monitor_500 = ClipboardMonitor::new(poll_interval_500ms, tx, shutdown_rx2);
        assert_eq!(monitor_500.poll_interval, Duration::from_millis(500));
    }
}
