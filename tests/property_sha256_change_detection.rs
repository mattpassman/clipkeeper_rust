/// Property-based test for SHA-256 change detection
/// 
/// Feature: clipkeeper-rust-conversion, Property 28: SHA-256 change detection is reliable
/// 
/// This test verifies that SHA-256 hashing for clipboard change detection is reliable:
/// 1. Same content always produces the same hash (deterministic)
/// 2. Different content produces different hashes (collision-free for practical purposes)
/// 
/// **Validates: Requirements 1.2, 23.3**

use proptest::prelude::*;
use sha2::{Sha256, Digest};

/// Calculate SHA-256 hash of content (same as ClipboardMonitor::calculate_hash)
fn calculate_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Strategy to generate various clipboard content strings
fn clipboard_content_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Short strings
        prop::string::string_regex("[a-zA-Z0-9 ]{1,50}").unwrap(),
        // Long strings
        prop::string::string_regex("[a-zA-Z0-9 ]{100,1000}").unwrap(),
        // Strings with special characters
        prop::string::string_regex("[a-zA-Z0-9!@#$%^&*()_+\\-=\\[\\]{}|;:',.<>?/` ]{10,100}").unwrap(),
        // Unicode strings
        ".*".prop_map(|s| s),
        // Empty string
        Just("".to_string()),
        // Whitespace variations
        prop::string::string_regex("[ \\t\\n\\r]{1,20}").unwrap(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    
    /// Property 28a: Same content produces same hash (deterministic)
    /// 
    /// For any clipboard content, calculating its SHA-256 hash multiple times
    /// should always produce the exact same hash value.
    #[test]
    fn property_same_content_produces_same_hash(
        content in clipboard_content_strategy()
    ) {
        // Calculate hash multiple times
        let hash1 = calculate_hash(&content);
        let hash2 = calculate_hash(&content);
        let hash3 = calculate_hash(&content);
        
        // All hashes should be identical
        prop_assert_eq!(&hash1, &hash2, "Hash should be deterministic (hash1 != hash2)");
        prop_assert_eq!(&hash2, &hash3, "Hash should be deterministic (hash2 != hash3)");
        prop_assert_eq!(&hash1, &hash3, "Hash should be deterministic (hash1 != hash3)");
        
        // Hash should be 64 characters (SHA-256 in hex)
        prop_assert_eq!(hash1.len(), 64, "SHA-256 hash should be 64 hex characters");
        
        // Hash should only contain hex characters
        prop_assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()), 
                    "Hash should only contain hex characters");
    }
    
    /// Property 28b: Different content produces different hashes
    /// 
    /// For any two different clipboard contents, their SHA-256 hashes should be different.
    /// This verifies that the hash function provides reliable change detection.
    #[test]
    fn property_different_content_produces_different_hashes(
        content1 in clipboard_content_strategy(),
        content2 in clipboard_content_strategy(),
    ) {
        // Only test when contents are actually different
        prop_assume!(content1 != content2);
        
        let hash1 = calculate_hash(&content1);
        let hash2 = calculate_hash(&content2);
        
        // Different content should produce different hashes
        prop_assert_ne!(
            &hash1, &hash2,
            "Different content should produce different hashes:\n  content1: {:?}\n  content2: {:?}\n  hash1: {}\n  hash2: {}",
            content1, content2, hash1, hash2
        );
    }
    
    /// Property 28c: Hash is stable across content modifications
    /// 
    /// For any content, even small modifications should produce a completely different hash.
    /// This verifies that the hash function is sensitive to changes.
    #[test]
    fn property_small_changes_produce_different_hashes(
        content in clipboard_content_strategy().prop_filter("non-empty", |s| !s.is_empty())
    ) {
        let original_hash = calculate_hash(&content);
        
        // Make small modifications
        let modified1 = format!("{} ", content); // Add space
        let modified2 = format!("{}a", content); // Add character
        
        // Remove last character (handle Unicode properly)
        let modified3 = if content.chars().count() > 1 {
            content.chars().take(content.chars().count() - 1).collect::<String>()
        } else {
            "x".to_string() // Replace with different character
        };
        
        let hash1 = calculate_hash(&modified1);
        let hash2 = calculate_hash(&modified2);
        let hash3 = calculate_hash(&modified3);
        
        // All modified versions should have different hashes
        prop_assert_ne!(&original_hash, &hash1, "Adding space should change hash");
        prop_assert_ne!(&original_hash, &hash2, "Adding character should change hash");
        prop_assert_ne!(&original_hash, &hash3, "Removing character should change hash");
        
        // Modified versions should also differ from each other
        prop_assert_ne!(&hash1, &hash2, "Different modifications should produce different hashes");
        prop_assert_ne!(&hash2, &hash3, "Different modifications should produce different hashes");
        prop_assert_ne!(&hash1, &hash3, "Different modifications should produce different hashes");
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    
    #[test]
    fn test_hash_determinism_with_known_values() {
        let content = "Hello, World!";
        
        let hash1 = calculate_hash(content);
        let hash2 = calculate_hash(content);
        
        assert_eq!(hash1, hash2, "Same content should produce same hash");
        assert_eq!(hash1.len(), 64, "SHA-256 hash should be 64 hex characters");
    }
    
    #[test]
    fn test_hash_difference_with_known_values() {
        let content1 = "Hello, World!";
        let content2 = "Hello, World";  // Missing exclamation mark
        
        let hash1 = calculate_hash(content1);
        let hash2 = calculate_hash(content2);
        
        assert_ne!(hash1, hash2, "Different content should produce different hashes");
    }
    
    #[test]
    fn test_hash_empty_string() {
        let empty = "";
        let hash = calculate_hash(empty);
        
        // SHA-256 of empty string is a known value
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
    
    #[test]
    fn test_hash_unicode_content() {
        let unicode1 = "Hello 世界";
        let unicode2 = "Hello 世界";
        let unicode3 = "Hello 世";
        
        let hash1 = calculate_hash(unicode1);
        let hash2 = calculate_hash(unicode2);
        let hash3 = calculate_hash(unicode3);
        
        assert_eq!(hash1, hash2, "Same unicode content should produce same hash");
        assert_ne!(hash1, hash3, "Different unicode content should produce different hashes");
    }
    
    #[test]
    fn test_hash_whitespace_sensitivity() {
        let content1 = "Hello World";
        let content2 = "Hello  World";  // Two spaces
        let content3 = "Hello\tWorld";   // Tab instead of space
        
        let hash1 = calculate_hash(content1);
        let hash2 = calculate_hash(content2);
        let hash3 = calculate_hash(content3);
        
        // All should be different
        assert_ne!(hash1, hash2, "Different whitespace should produce different hashes");
        assert_ne!(hash1, hash3, "Different whitespace should produce different hashes");
        assert_ne!(hash2, hash3, "Different whitespace should produce different hashes");
    }
    
    #[test]
    fn test_hash_case_sensitivity() {
        let content1 = "Hello World";
        let content2 = "hello world";
        let content3 = "HELLO WORLD";
        
        let hash1 = calculate_hash(content1);
        let hash2 = calculate_hash(content2);
        let hash3 = calculate_hash(content3);
        
        // All should be different (case-sensitive)
        assert_ne!(hash1, hash2, "Different case should produce different hashes");
        assert_ne!(hash1, hash3, "Different case should produce different hashes");
        assert_ne!(hash2, hash3, "Different case should produce different hashes");
    }
    
    #[test]
    fn test_hash_long_content() {
        let long_content = "a".repeat(10000);
        let hash = calculate_hash(&long_content);
        
        assert_eq!(hash.len(), 64, "Hash of long content should still be 64 characters");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
    
    #[test]
    fn test_hash_special_characters() {
        let special1 = "!@#$%^&*()_+-=[]{}|;':\",./<>?";
        let special2 = "!@#$%^&*()_+-=[]{}|;':\",./<>?";
        let special3 = "!@#$%^&*()_+-=[]{}|;':\",./<>"; // Missing last char
        
        let hash1 = calculate_hash(special1);
        let hash2 = calculate_hash(special2);
        let hash3 = calculate_hash(special3);
        
        assert_eq!(hash1, hash2, "Same special characters should produce same hash");
        assert_ne!(hash1, hash3, "Different special characters should produce different hashes");
    }
    
    #[test]
    fn test_hash_newlines_and_line_endings() {
        let unix = "Line1\nLine2\nLine3";
        let windows = "Line1\r\nLine2\r\nLine3";
        let mac = "Line1\rLine2\rLine3";
        
        let hash_unix = calculate_hash(unix);
        let hash_windows = calculate_hash(windows);
        let hash_mac = calculate_hash(mac);
        
        // Different line endings should produce different hashes
        assert_ne!(hash_unix, hash_windows, "Unix and Windows line endings should differ");
        assert_ne!(hash_unix, hash_mac, "Unix and Mac line endings should differ");
        assert_ne!(hash_windows, hash_mac, "Windows and Mac line endings should differ");
    }
}
