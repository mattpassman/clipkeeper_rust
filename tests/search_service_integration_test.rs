use clipkeeper::history_store::HistoryStore;
use clipkeeper::search_service::{SearchService, SearchOptions};
use tempfile::tempdir;

#[test]
fn test_search_service_integration() {
    // Create a temporary database
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    
    // Create HistoryStore and add some test entries
    let store = HistoryStore::new(&db_path).unwrap();
    let shared_store = HistoryStore::new_shared(&db_path).unwrap();
    
    // Add test entries
    store.save("Hello world", "text").unwrap();
    store.save("Rust programming language", "text").unwrap();
    store.save("Python code example", "code").unwrap();
    store.save("JavaScript function", "code").unwrap();
    
    // Create SearchService
    let search_service = SearchService::new(shared_store);
    
    // Test 1: Search with single keyword
    let options = SearchOptions {
        limit: 10,
        content_type: None,
        since: None,
    };
    let results = search_service.search("Rust", options.clone()).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Rust"));
    assert_eq!(results[0].content_type, "text");
    assert!(!results[0].preview.is_empty());
    assert!(!results[0].relative_time.is_empty());
    
    // Test 2: Search with multiple keywords (AND logic)
    let results = search_service.search("code example", options.clone()).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Python"));
    
    // Test 3: Search with content type filter
    let options_with_filter = SearchOptions {
        limit: 10,
        content_type: Some("code".to_string()),
        since: None,
    };
    let results = search_service.search("", options_with_filter).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.content_type == "code"));
    
    // Test 4: Empty query returns recent entries
    let results = search_service.search("", options.clone()).unwrap();
    assert_eq!(results.len(), 4);
    
    // Test 5: Verify results are ordered by timestamp descending
    // (most recent first)
    assert!(results[0].timestamp >= results[1].timestamp);
    assert!(results[1].timestamp >= results[2].timestamp);
    
    // Test 6: Verify preview is created correctly
    let long_content = "a".repeat(150);
    store.save(&long_content, "text").unwrap();
    let results = search_service.search("", options.clone()).unwrap();
    assert!(results.len() > 0);
    let long_result = results.iter().find(|r| r.content.len() > 100);
    assert!(long_result.is_some(), "No result with content length > 100 found");
    let long_result = long_result.unwrap();
    assert!(long_result.preview.len() <= 103); // 100 chars + "..."
    assert!(long_result.preview.ends_with("..."));
    
    // Test 7: Verify relative time formatting
    assert!(results[0].relative_time == "Just now" || results[0].relative_time.contains("ago"));
}

#[test]
fn test_search_service_with_limit() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    
    let store = HistoryStore::new(&db_path).unwrap();
    let shared_store = HistoryStore::new_shared(&db_path).unwrap();
    
    // Add 10 entries
    for i in 0..10 {
        store.save(&format!("Entry {}", i), "text").unwrap();
    }
    
    let search_service = SearchService::new(shared_store);
    
    // Test with limit of 5
    let options = SearchOptions {
        limit: 5,
        content_type: None,
        since: None,
    };
    let results = search_service.search("", options).unwrap();
    assert_eq!(results.len(), 5);
}

#[test]
fn test_search_service_parse_query() {
    // Test parse_query static method
    let keywords = SearchService::parse_query("hello world test");
    assert_eq!(keywords, vec!["hello", "world", "test"]);
    
    let keywords = SearchService::parse_query("  extra   spaces  ");
    assert_eq!(keywords, vec!["extra", "spaces"]);
    
    let keywords = SearchService::parse_query("");
    assert_eq!(keywords.len(), 0);
}

#[test]
fn test_search_service_format_relative_time() {
    use chrono::Utc;
    
    let now = Utc::now().timestamp_millis();
    
    // Just now
    assert_eq!(SearchService::format_relative_time(now - 30_000), "Just now");
    
    // Minutes
    assert_eq!(SearchService::format_relative_time(now - 60_000), "1 min ago");
    assert_eq!(SearchService::format_relative_time(now - 120_000), "2 mins ago");
    
    // Hours
    assert_eq!(SearchService::format_relative_time(now - 3_600_000), "1 hour ago");
    assert_eq!(SearchService::format_relative_time(now - 7_200_000), "2 hours ago");
    
    // Days
    assert_eq!(SearchService::format_relative_time(now - 86_400_000), "1 day ago");
    assert_eq!(SearchService::format_relative_time(now - 172_800_000), "2 days ago");
    
    // Absolute format for older entries
    let old_timestamp = now - 604_800_000; // 7 days ago
    let result = SearchService::format_relative_time(old_timestamp);
    assert!(result.contains("-")); // Should contain date separator
    assert!(result.contains(":")); // Should contain time separator
}
