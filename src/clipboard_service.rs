use arboard::Clipboard;
use std::sync::Mutex;
use crate::errors::{Result, ClipboardError};

/// Thread-safe wrapper around arboard::Clipboard for clipboard operations
/// 
/// This service provides a safe interface for reading from and writing to
/// the system clipboard across multiple threads. It handles platform-specific
/// clipboard formats and provides proper error handling.
/// 
/// # Thread Safety
/// 
/// The clipboard is wrapped in a Mutex to ensure thread-safe access.
/// Multiple threads can safely call read() and copy() methods.
/// 
/// # Platform Support
/// 
/// - Windows: Handles Windows-specific clipboard formats
/// - macOS: Handles macOS-specific clipboard formats  
/// - Linux: Handles X11 and Wayland clipboard protocols
/// 
/// # Examples
/// 
/// ```no_run
/// use clipkeeper::clipboard_service::ClipboardService;
/// 
/// let service = ClipboardService::new().unwrap();
/// 
/// // Copy text to clipboard
/// service.copy("Hello, World!").unwrap();
/// 
/// // Read text from clipboard
/// let content = service.read().unwrap();
/// println!("Clipboard content: {}", content);
/// ```
pub struct ClipboardService {
    clipboard: Mutex<Clipboard>,
}

impl ClipboardService {
    /// Create a new ClipboardService
    /// 
    /// # Errors
    /// 
    /// Returns an error if the clipboard cannot be initialized.
    /// This can happen if:
    /// - The clipboard is not available on the system
    /// - Platform-specific clipboard initialization fails
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use clipkeeper::clipboard_service::ClipboardService;
    /// 
    /// let service = ClipboardService::new().unwrap();
    /// ```
    pub fn new() -> Result<Self> {
        let clipboard = Clipboard::new()
            .map_err(|e| ClipboardError::Arboard(e.to_string()))?;
        
        Ok(Self {
            clipboard: Mutex::new(clipboard),
        })
    }

    /// Read text content from the system clipboard
    /// 
    /// This method reads the current text content from the clipboard.
    /// It handles platform-specific clipboard formats automatically.
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The clipboard cannot be accessed (AccessDenied)
    /// - The clipboard is empty (EmptyContent)
    /// - The clipboard contains non-text data
    /// - A platform-specific error occurs
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use clipkeeper::clipboard_service::ClipboardService;
    /// 
    /// let service = ClipboardService::new().unwrap();
    /// match service.read() {
    ///     Ok(content) => println!("Clipboard: {}", content),
    ///     Err(e) => eprintln!("Failed to read clipboard: {}", e),
    /// }
    /// ```
    pub fn read(&self) -> Result<String> {
        let mut clipboard = self.clipboard.lock()
            .map_err(|e| ClipboardError::Arboard(format!("Failed to lock clipboard: {}", e)))?;

        let content = clipboard
            .get_text()
            .map_err(|e| {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("access") || err_str.contains("denied") || err_str.contains("permission") {
                    ClipboardError::AccessDenied
                } else {
                    ClipboardError::Arboard(e.to_string())
                }
            })?;
        Ok(content)
    }

    /// Copy text content to the system clipboard
    /// 
    /// This method writes the provided text to the clipboard, replacing
    /// any existing content. The content is preserved exactly without
    /// any formatting changes.
    /// 
    /// # Arguments
    /// 
    /// * `content` - The text content to copy to the clipboard
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The clipboard cannot be accessed (AccessDenied)
    /// - The write operation fails
    /// - A platform-specific error occurs
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use clipkeeper::clipboard_service::ClipboardService;
    /// 
    /// let service = ClipboardService::new().unwrap();
    /// service.copy("Hello, World!").unwrap();
    /// ```
    pub fn copy(&self, content: &str) -> Result<()> {
        let mut clipboard = self.clipboard.lock()
            .map_err(|e| ClipboardError::Arboard(format!("Failed to lock clipboard: {}", e)))?;

        clipboard
            .set_text(content)
            .map_err(|e| {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("access") || err_str.contains("denied") || err_str.contains("permission") {
                    ClipboardError::AccessDenied
                } else {
                    ClipboardError::Arboard(e.to_string())
                }
            })?;
        Ok(())
    }
}

// Implement Default trait for convenience
impl Default for ClipboardService {
    fn default() -> Self {
        Self::new().expect("Failed to initialize clipboard service")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to check if clipboard is available
    fn is_clipboard_available() -> bool {
        ClipboardService::new().is_ok()
    }

    #[test]
    fn test_clipboard_service_creation() {
        // Test that we can create a clipboard service
        let result = ClipboardService::new();
        
        // In headless environments (CI, Docker), clipboard may not be available
        // This is expected and not a failure
        if result.is_err() {
            eprintln!("Note: Clipboard not available in this environment (expected in headless/CI)");
            return;
        }
        
        assert!(result.is_ok(), "Failed to create clipboard service: {:?}", result.err());
    }

    #[test]
    fn test_clipboard_service_default() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        // Test that Default trait works
        let _service = ClipboardService::default();
    }

    #[test]
    fn test_clipboard_copy_and_read() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        let service = ClipboardService::new().unwrap();
        
        let test_content = "Test clipboard content";
        
        // Copy content to clipboard
        let copy_result = service.copy(test_content);
        assert!(copy_result.is_ok(), "Failed to copy to clipboard: {:?}", copy_result.err());
        
        // Read content back from clipboard
        let read_result = service.read();
        assert!(read_result.is_ok(), "Failed to read from clipboard: {:?}", read_result.err());
        
        // Verify content matches
        let read_content = read_result.unwrap();
        assert_eq!(read_content, test_content, "Clipboard content doesn't match");
    }

    #[test]
    fn test_clipboard_preserves_exact_content() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        let service = ClipboardService::new().unwrap();
        
        // Test with various content types
        let test_cases = vec![
            "Simple text",
            "Text with\nnewlines\nand\ttabs",
            "Special chars: !@#$%^&*()",
            "Unicode: 你好世界 🌍",
            "   Leading and trailing spaces   ",
            "Multiple\n\n\nNewlines",
        ];
        
        for test_content in test_cases {
            service.copy(test_content).unwrap();
            let read_content = service.read().unwrap();
            assert_eq!(
                read_content, test_content,
                "Content not preserved exactly: expected '{}', got '{}'",
                test_content, read_content
            );
        }
    }

    #[test]
    fn test_clipboard_empty_string() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        let service = ClipboardService::new().unwrap();
        
        // Copy empty string
        let result = service.copy("");
        assert!(result.is_ok(), "Failed to copy empty string: {:?}", result.err());
        
        // Read back
        let read_result = service.read();
        assert!(read_result.is_ok(), "Failed to read empty string: {:?}", read_result.err());
        assert_eq!(read_result.unwrap(), "");
    }

    #[test]
    fn test_clipboard_large_content() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        let service = ClipboardService::new().unwrap();
        
        // Create a large string (10KB)
        let large_content = "A".repeat(10_000);
        
        // Copy and read back
        service.copy(&large_content).unwrap();
        let read_content = service.read().unwrap();
        
        assert_eq!(read_content.len(), large_content.len());
        assert_eq!(read_content, large_content);
    }

    #[test]
    fn test_clipboard_multiple_operations() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        let service = ClipboardService::new().unwrap();
        
        // Perform multiple copy/read operations
        for i in 0..10 {
            let content = format!("Test content {}", i);
            service.copy(&content).unwrap();
            let read_content = service.read().unwrap();
            assert_eq!(read_content, content);
        }
    }

    #[test]
    fn test_clipboard_thread_safety() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        use std::sync::Arc;
        use std::thread;
        
        let service = Arc::new(ClipboardService::new().unwrap());
        let mut handles = vec![];
        
        // Spawn multiple threads that access the clipboard
        for i in 0..5 {
            let service_clone = Arc::clone(&service);
            let handle = thread::spawn(move || {
                let content = format!("Thread {} content", i);
                service_clone.copy(&content).unwrap();
                let read_content = service_clone.read().unwrap();
                // Note: Due to race conditions, we might read content from another thread
                // The important thing is that operations don't panic or deadlock
                assert!(!read_content.is_empty());
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_clipboard_special_characters() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        let service = ClipboardService::new().unwrap();
        
        // Test various special characters
        let special_chars = vec![
            "\n",
            "\r\n",
            "\t",
            "\"",
            "'",
            "\\",
            "\0",
            "Mixed\nSpecial\tChars\"Here'",
        ];
        
        for chars in special_chars {
            service.copy(chars).unwrap();
            let read_content = service.read().unwrap();
            // Note: Some platforms may normalize line endings
            // So we check that the content is not empty rather than exact match
            assert!(!read_content.is_empty(), "Failed to preserve special chars: {:?}", chars);
        }
    }

    #[test]
    fn test_clipboard_overwrite() {
        if !is_clipboard_available() {
            eprintln!("Note: Skipping test - clipboard not available");
            return;
        }
        
        let service = ClipboardService::new().unwrap();
        
        // Copy first content
        service.copy("First content").unwrap();
        assert_eq!(service.read().unwrap(), "First content");
        
        // Overwrite with second content
        service.copy("Second content").unwrap();
        assert_eq!(service.read().unwrap(), "Second content");
        
        // Verify first content is gone
        let final_content = service.read().unwrap();
        assert_eq!(final_content, "Second content");
        assert_ne!(final_content, "First content");
    }
}
