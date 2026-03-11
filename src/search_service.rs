use crate::content_classifier::ContentType;
use crate::errors::Result;
use crate::history_store::{ClipboardEntry, SharedHistoryStore};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// SearchService coordinates search operations with filtering and result formatting
pub struct SearchService {
    history_store: SharedHistoryStore,
}

/// Options for search operations
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub limit: usize,
    pub content_type: Option<String>,
    pub since: Option<String>,
}

/// Formatted search result with preview and relative time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: uuid::Uuid,
    pub content: String,
    pub content_type: ContentType,
    pub timestamp: i64,
    pub preview: String,
    pub relative_time: String,
}

impl SearchService {
    /// Create a new SearchService with a shared HistoryStore
    pub fn new(history_store: SharedHistoryStore) -> Self {
        Self { history_store }
    }

    /// Parse a search query into keywords by splitting on whitespace
    /// 
    /// # Requirements
    /// - Requirement 5.1: Parse query into keywords by splitting on whitespace
    /// - Requirement 18.1: Treat each word as a separate keyword
    pub fn parse_query(query: &str) -> Vec<String> {
        query
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }

    /// Search clipboard history with optional filters
    /// 
    /// # Requirements
    /// - Requirement 5.2: Perform FTS5 search with AND logic for multiple keywords
    /// - Requirement 5.3: Fall back to LIKE-based search when FTS5 unavailable
    /// - Requirement 5.6: Filter by content type
    /// - Requirement 5.7: Filter by date
    /// - Requirement 5.8: Respect result limit
    /// - Requirement 5.9: Return recent entries for empty query
    /// - Requirement 5.10: Order results by timestamp descending
    pub fn search(&self, query: &str, options: SearchOptions) -> Result<Vec<SearchResult>> {
        crate::log_component_action!(
            "SearchService",
            "Search initiated",
            query_length = query.len(),
            limit = options.limit,
            has_type_filter = options.content_type.is_some(),
            has_date_filter = options.since.is_some()
        );

        // Delegate to HistoryStore for actual search
        let store = self.history_store.lock().unwrap();
        let entries = store.search(
            query,
            options.limit,
            options.content_type.as_deref(),
            options.since.as_deref(),
        )?;

        // Format results with preview and relative time
        let results = Self::format_results(entries);

        crate::log_component_action!(
            "SearchService",
            "Search completed",
            results_count = results.len()
        );

        Ok(results)
    }

    /// Format entries into search results with preview and relative time
    /// 
    /// # Requirements
    /// - Requirement 5.4: Format results with id, content, contentType, timestamp, preview, relativeTime
    /// - Requirement 18.4: Include entry id, content preview, content type, and relative timestamp
    fn format_results(entries: Vec<ClipboardEntry>) -> Vec<SearchResult> {
        entries
            .into_iter()
            .map(|entry| SearchResult {
                id: entry.id,
                content: entry.content.clone(),
                content_type: entry.content_type,
                timestamp: entry.timestamp,
                preview: Self::create_preview(&entry.content, 100),
                relative_time: Self::format_relative_time(entry.timestamp),
            })
            .collect()
    }

    /// Create a preview of content with maximum length
    fn create_preview(content: &str, max_length: usize) -> String {
        if content.len() <= max_length {
            content.to_string()
        } else {
            format!("{}...", &content[..max_length])
        }
    }

    /// Format timestamp as relative time
    /// 
    /// # Requirements
    /// - Requirement 5.5: Use "Just now" for <1 min, "N min(s) ago" for <1 hour,
    ///   "N hour(s) ago" for <24 hours, "N day(s) ago" for <7 days, and absolute format for older
    pub fn format_relative_time(timestamp: i64) -> String {
        let now = Utc::now().timestamp_millis();
        let diff_ms = now - timestamp;
        let diff_secs = diff_ms / 1000;

        if diff_secs < 60 {
            "Just now".to_string()
        } else if diff_secs < 3600 {
            let mins = diff_secs / 60;
            if mins == 1 {
                "1 min ago".to_string()
            } else {
                format!("{} mins ago", mins)
            }
        } else if diff_secs < 86400 {
            let hours = diff_secs / 3600;
            if hours == 1 {
                "1 hour ago".to_string()
            } else {
                format!("{} hours ago", hours)
            }
        } else if diff_secs < 604800 {
            let days = diff_secs / 86400;
            if days == 1 {
                "1 day ago".to_string()
            } else {
                format!("{} days ago", days)
            }
        } else {
            // For older entries, use absolute format
            let dt = DateTime::from_timestamp_millis(timestamp)
                .unwrap_or_else(|| Utc::now());
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_single_keyword() {
        let keywords = SearchService::parse_query("hello");
        assert_eq!(keywords, vec!["hello"]);
    }

    #[test]
    fn test_parse_query_multiple_keywords() {
        let keywords = SearchService::parse_query("hello world test");
        assert_eq!(keywords, vec!["hello", "world", "test"]);
    }

    #[test]
    fn test_parse_query_with_extra_whitespace() {
        let keywords = SearchService::parse_query("  hello   world  ");
        assert_eq!(keywords, vec!["hello", "world"]);
    }

    #[test]
    fn test_parse_query_empty() {
        let keywords = SearchService::parse_query("");
        assert_eq!(keywords.len(), 0);
    }

    #[test]
    fn test_create_preview_short_content() {
        let preview = SearchService::create_preview("Hello world", 100);
        assert_eq!(preview, "Hello world");
    }

    #[test]
    fn test_create_preview_long_content() {
        let content = "a".repeat(150);
        let preview = SearchService::create_preview(&content, 100);
        assert_eq!(preview.len(), 103); // 100 chars + "..."
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn test_format_relative_time_just_now() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 30_000); // 30 seconds ago
        assert_eq!(result, "Just now");
    }

    #[test]
    fn test_format_relative_time_minutes() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 120_000); // 2 minutes ago
        assert_eq!(result, "2 mins ago");
    }

    #[test]
    fn test_format_relative_time_one_minute() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 60_000); // 1 minute ago
        assert_eq!(result, "1 min ago");
    }

    #[test]
    fn test_format_relative_time_hours() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 7_200_000); // 2 hours ago
        assert_eq!(result, "2 hours ago");
    }

    #[test]
    fn test_format_relative_time_one_hour() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 3_600_000); // 1 hour ago
        assert_eq!(result, "1 hour ago");
    }

    #[test]
    fn test_format_relative_time_days() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 172_800_000); // 2 days ago
        assert_eq!(result, "2 days ago");
    }

    #[test]
    fn test_format_relative_time_one_day() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 86_400_000); // 1 day ago
        assert_eq!(result, "1 day ago");
    }

    #[test]
    fn test_format_relative_time_absolute() {
        let now = Utc::now().timestamp_millis();
        let result = SearchService::format_relative_time(now - 604_800_000); // 7 days ago
        // Should be in absolute format YYYY-MM-DD HH:MM:SS
        assert!(result.contains("-"));
        assert!(result.contains(":"));
    }
}
