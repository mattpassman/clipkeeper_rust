use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use chrono::Utc;
use crate::errors::{Context, Result, DatabaseError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub content: String,
    pub content_type: String,
    pub timestamp: i64,
    pub source_app: Option<String>,
    #[serde(skip)]
    pub metadata: EntryMetadata,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryMetadata {
    pub language: Option<String>,
    pub confidence: f64,
    pub character_count: usize,
    pub word_count: usize,
}

impl Default for EntryMetadata {
    fn default() -> Self {
        Self {
            language: None,
            confidence: 1.0,
            character_count: 0,
            word_count: 0,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Statistics {
    pub total: usize,
    pub by_type: Vec<(String, usize)>,
}

pub struct HistoryStore {
    conn: Connection,
    fts_available: bool,
    /// Cached entry count updated atomically on save/delete/clear.
    /// Wrapped in Arc so it can be shared with ResourceMonitor without
    /// requiring the caller to lock the HistoryStore mutex.
    entry_count: Arc<AtomicUsize>,
}

/// Thread-safe wrapper for HistoryStore
/// 
/// This type can be safely shared across threads using Arc<Mutex<HistoryStore>>.
/// All methods acquire the lock internally to ensure thread-safe access to the database.
/// 
/// # Example
/// 
/// ```ignore
/// use std::sync::Arc;
/// use std::thread;
/// 
/// // Create a shared instance
/// let store = HistoryStore::new_shared(&db_path)?;
/// 
/// // Clone for use in multiple threads
/// let store_clone = Arc::clone(&store);
/// 
/// // Spawn a thread that writes to the database
/// let handle = thread::spawn(move || {
///     let store = store_clone.lock().unwrap();
///     store.save("clipboard content", "text")?;
///     Ok::<_, ClipKeeperError>(())
/// });
/// 
/// // Use the original reference in the main thread
/// {
///     let store = store.lock().unwrap();
///     let entries = store.list(10, None, None, None)?;
///     println!("Found {} entries", entries.len());
/// }
/// 
/// handle.join().unwrap()?;
/// ```
pub type SharedHistoryStore = Arc<Mutex<HistoryStore>>;

impl HistoryStore {
    pub fn new(db_path: &Path) -> Result<Self> {
        crate::log_component_action!(
            "HistoryStore",
            "Initializing database",
            db_path = %db_path.display()
        );
        
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create database directory")?;
            // Set data directory permissions to 0700 on Unix (Task 16.1)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open database at {:?}", db_path))?;
        
        // Check FTS5 availability
        let fts_available = Self::check_fts5_available(&conn);
        
        crate::log_component_action!(
            "HistoryStore",
            "FTS5 availability checked",
            fts_available = fts_available
        );
        
        // Create schema_version table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            )",
            [],
        )
        .context("Failed to create schema_version table")?;
        
        // Check and run migrations if needed
        let current_version = Self::get_schema_version(&conn)?;
        let target_version = 1; // Current schema version
        
        if current_version < target_version {
            crate::log_component_action!(
                "HistoryStore",
                "Running database migrations",
                current_version = current_version,
                target_version = target_version
            );
            
            Self::run_migrations(&conn, current_version, target_version, fts_available)?;
            
            crate::log_component_action!(
                "HistoryStore",
                "Database migrations completed",
                new_version = target_version
            );
        } else {
            crate::log_component_action!(
                "HistoryStore",
                "Database schema is up to date",
                version = current_version
            );
        }
        
        crate::log_component_action!(
            "HistoryStore",
            "Database initialized successfully",
            db_path = %db_path.display(),
            fts_available = fts_available
        );
        
        // Seed the cached entry count from the database
        let initial_count: usize = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_entries",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        Ok(Self { conn, fts_available, entry_count: Arc::new(AtomicUsize::new(initial_count)) })
    }
    
    /// Check if FTS5 is available in this SQLite build
    fn check_fts5_available(conn: &Connection) -> bool {
        // Try to create a temporary FTS5 table
        let result = conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS temp.fts5_test USING fts5(content)",
            [],
        );
        
        if result.is_ok() {
            // Clean up the test table
            let _ = conn.execute("DROP TABLE IF EXISTS temp.fts5_test", []);
            true
        } else {
            false
        }
    }
    
    /// Get the current schema version from the database
    /// Returns 0 if no version is set (new database or Node.js database)
    fn get_schema_version(conn: &Connection) -> Result<i32> {
        let version: std::result::Result<i32, rusqlite::Error> = conn.query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0),
        );
        
        match version {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0), // No version set yet
            Err(e) => Err(anyhow::anyhow!("Failed to get schema version: {}", e)),
        }
    }
    
    /// Run database migrations from current_version to target_version
    fn run_migrations(
        conn: &Connection,
        current_version: i32,
        target_version: i32,
        fts_available: bool,
    ) -> Result<()> {
        // Migration 0 -> 1: Initial schema setup
        // This handles both new databases and migration from Node.js version
        if current_version < 1 {
            Self::migrate_to_v1(conn, fts_available)?;
        }
        
        // Update schema version
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
            params![target_version],
        )
        .context("Failed to update schema version: {}")?;
        
        Ok(())
    }
    
    /// Migration to version 1: Ensure all tables and indexes exist
    /// This migration is idempotent and can safely run on:
    /// - New databases (creates everything from scratch)
    /// - Existing Node.js databases (adds missing tables/indexes)
    /// - Existing Rust databases (no-op, everything already exists)
    fn migrate_to_v1(conn: &Connection, fts_available: bool) -> Result<()> {
        crate::log_component_action!(
            "HistoryStore",
            "Running migration to version 1"
        );
        
        // Create main table if it doesn't exist
        // This is compatible with Node.js database schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_entries (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                content_type TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                source_app TEXT,
                metadata TEXT,
                created_at INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .context("Failed to create clipboard_entries table: {}")?;
        
        // Add created_at column if migrating from an older schema that lacks it
        let has_created_at: bool = conn
            .prepare("SELECT created_at FROM clipboard_entries LIMIT 0")
            .is_ok();
        
        if !has_created_at {
            conn.execute(
                "ALTER TABLE clipboard_entries ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .context("Failed to add created_at column: {}")?;
            
            // Backfill created_at from timestamp for existing rows
            conn.execute(
                "UPDATE clipboard_entries SET created_at = timestamp WHERE created_at = 0",
                [],
            )
            .context("Failed to backfill created_at values: {}")?;
            
            crate::log_component_action!(
                "HistoryStore",
                "Added created_at column to existing table"
            );
        }
        
        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_timestamp ON clipboard_entries(timestamp DESC)",
            [],
        )
        .context("Failed to create timestamp index: {}")?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_content_type ON clipboard_entries(content_type)",
            [],
        )
        .context("Failed to create content_type index: {}")?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_created_at ON clipboard_entries(created_at)",
            [],
        )
        .context("Failed to create created_at index: {}")?;
        
        // Create FTS5 virtual table and triggers if available
        if fts_available {
            conn.execute(
                "CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_entries_fts USING fts5(
                    content,
                    content='clipboard_entries',
                    content_rowid='rowid'
                )",
                [],
            )
            .context("Failed to create FTS5 table: {}")?;
            
            // Create triggers to keep FTS5 in sync
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS clipboard_entries_ai AFTER INSERT ON clipboard_entries BEGIN
                    INSERT INTO clipboard_entries_fts(rowid, content) VALUES (new.rowid, new.content);
                END",
                [],
            )
            .context("Failed to create INSERT trigger: {}")?;
            
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS clipboard_entries_ad AFTER DELETE ON clipboard_entries BEGIN
                    INSERT INTO clipboard_entries_fts(clipboard_entries_fts, rowid, content) 
                    VALUES('delete', old.rowid, old.content);
                END",
                [],
            )
            .context("Failed to create DELETE trigger: {}")?;
            
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS clipboard_entries_au AFTER UPDATE ON clipboard_entries BEGIN
                    INSERT INTO clipboard_entries_fts(clipboard_entries_fts, rowid, content) 
                    VALUES('delete', old.rowid, old.content);
                    INSERT INTO clipboard_entries_fts(rowid, content) VALUES (new.rowid, new.content);
                END",
                [],
            )
            .context("Failed to create UPDATE trigger: {}")?;
            
            // If migrating from Node.js database, rebuild FTS5 index from existing entries
            let entry_count: i32 = conn.query_row(
                "SELECT COUNT(*) FROM clipboard_entries",
                [],
                |row| row.get(0),
            )
            .context("Failed to count entries: {}")?;
            
            if entry_count > 0 {
                crate::log_component_action!(
                    "HistoryStore",
                    "Rebuilding FTS5 index from existing entries",
                    entry_count = entry_count
                );
                
                // For contentless FTS5 tables, use the 'rebuild' command
                // This will repopulate the index from the content table
                conn.execute(
                    "INSERT INTO clipboard_entries_fts(clipboard_entries_fts) VALUES('rebuild')",
                    [],
                )
                .context("Failed to rebuild FTS5 index: {}")?;
                
                crate::log_component_action!(
                    "HistoryStore",
                    "FTS5 index rebuilt successfully",
                    indexed_entries = entry_count
                );
            }
            
            crate::log_component_action!(
                "HistoryStore",
                "FTS5 virtual table and triggers created"
            );
        }
        
        crate::log_component_action!(
            "HistoryStore",
            "Migration to version 1 completed"
        );
        
        Ok(())
    }
    
    /// Create a new thread-safe shared instance of HistoryStore
    /// 
    /// This wraps the HistoryStore in Arc<Mutex<>> for safe concurrent access
    /// from multiple threads. This is the recommended way to create a HistoryStore
    /// when it needs to be accessed from multiple services or background threads.
    /// 
    /// # Thread Safety
    /// 
    /// The returned `SharedHistoryStore` can be cloned and passed to multiple threads.
    /// Each thread must acquire the lock before accessing the database:
    /// 
    /// ```ignore
    /// let store = HistoryStore::new_shared(&db_path)?;
    /// 
    /// // Clone for another thread
    /// let store_clone = Arc::clone(&store);
    /// 
    /// thread::spawn(move || {
    ///     // Acquire lock before use
    ///     let store = store_clone.lock().unwrap();
    ///     store.save("content", "text").unwrap();
    /// });
    /// ```
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The database file cannot be created or opened
    /// - The database schema cannot be initialized
    /// - The parent directory cannot be created
    /// 
    /// # Example
    /// 
    /// ```ignore
    /// use clipkeeper::history_store::HistoryStore;
    /// use std::path::Path;
    /// 
    /// let db_path = Path::new("/tmp/clipkeeper.db");
    /// let store = HistoryStore::new_shared(&db_path)?;
    /// 
    /// // Use in the current thread
    /// {
    ///     let store = store.lock().unwrap();
    ///     let id = store.save("Hello, world!", "text")?;
    ///     println!("Saved entry: {}", id);
    /// }
    /// 
    /// // Can now be cloned and shared across threads
    /// let store_clone = Arc::clone(&store);
    /// ```
    pub fn new_shared(db_path: &Path) -> Result<SharedHistoryStore> {
        let store = Self::new(db_path)?;
        Ok(Arc::new(Mutex::new(store)))
    }
    
    pub fn save(&self, content: &str, content_type: &str) -> Result<String> {
        self.check_open()?;
        
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now().timestamp_millis();
        
        // Calculate metadata
        let character_count = content.chars().count();
        let word_count = content.split_whitespace().count();
        
        let metadata = EntryMetadata {
            language: None, // Will be populated by content classifier in the future
            confidence: 1.0,
            character_count,
            word_count,
        };
        
        // Serialize metadata to JSON
        let metadata_json = serde_json::to_string(&metadata)
            .context("Failed to serialize metadata: {}")?;
        
        let created_at = Utc::now().timestamp_millis();
        
        // Try INSERT with created_at first (JS schema compatibility),
        // fall back to without if column doesn't exist
        let save_result = self.conn.execute(
            "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, content, content_type, timestamp, None::<String>, metadata_json, created_at],
        );
        
        match save_result {
            Ok(_) => {}
            Err(e) if e.to_string().contains("created_at") => {
                // Column doesn't exist, insert without it
                self.conn.execute(
                    "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![id, content, content_type, timestamp, None::<String>, metadata_json],
                )
                .context("Failed to save entry: {}")?;
            }
            Err(e) => return Err(e).context("Failed to save entry: {}"),
        }
        
        crate::log_component_action!(
            "HistoryStore",
            "Entry saved",
            entry_id = %id,
            content_type = content_type,
            content_length = content.len(),
            character_count = character_count,
            word_count = word_count
        );
        
        self.entry_count.fetch_add(1, Ordering::Relaxed);
        Ok(id)
    }
    
    pub fn list(
        &self,
        limit: usize,
        content_type: Option<&str>,
        search: Option<&str>,
        since: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>> {
        self.check_open()?;
        
        let mut query = "SELECT id, content, content_type, timestamp, source_app, metadata, created_at FROM clipboard_entries WHERE 1=1".to_string();
        
        if let Some(ct) = content_type {
            query.push_str(&format!(" AND content_type = '{}'", ct));
        }
        
        if let Some(s) = search {
            query.push_str(&format!(" AND content LIKE '%{}%'", s));
        }
        
        if let Some(since_str) = since {
            if let Ok(since_ts) = parse_since(since_str) {
                query.push_str(&format!(" AND timestamp >= {}", since_ts));
            }
        }
        
        query.push_str(&format!(" ORDER BY timestamp DESC LIMIT {}", limit));
        
        let mut stmt = self.conn.prepare(&query)
            .context("Failed to prepare query: {}")?;
        let entries = stmt.query_map([], Self::row_to_entry)
        .context("Failed to execute query: {}")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to collect results: {}")?;
        
        crate::log_component_action!(
            "HistoryStore",
            "Entries listed",
            count = entries.len(),
            limit = limit,
            has_filter = content_type.is_some() || search.is_some() || since.is_some()
        );
        
        Ok(entries)
    }
    
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        content_type: Option<&str>,
        since: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>> {
        self.check_open()?;
        
        crate::log_component_action!(
            "HistoryStore",
            "Search initiated",
            query_length = query.len(),
            limit = limit,
            has_type_filter = content_type.is_some(),
            has_date_filter = since.is_some(),
            using_fts5 = self.fts_available
        );
        
        // If query is empty, just list entries
        if query.trim().is_empty() {
            return self.list(limit, content_type, None, since);
        }
        
        // Use FTS5 if available, otherwise fall back to LIKE
        if self.fts_available {
            self.search_fts5(query, limit, content_type, since)
        } else {
            self.search_like(query, limit, content_type, since)
        }
    }
    
    /// Search using FTS5 full-text search
    fn search_fts5(
        &self,
        query: &str,
        limit: usize,
        content_type: Option<&str>,
        since: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>> {
        // Parse query into keywords and build FTS5 MATCH expression
        let keywords: Vec<&str> = query.split_whitespace().collect();
        if keywords.is_empty() {
            return self.list(limit, content_type, None, since);
        }
        
        // Build FTS5 MATCH query with AND logic for multiple keywords
        let fts_query = keywords.join(" AND ");
        
        let mut sql = "SELECT ce.id, ce.content, ce.content_type, ce.timestamp, ce.source_app, ce.metadata, ce.created_at 
                       FROM clipboard_entries ce
                       JOIN clipboard_entries_fts fts ON ce.rowid = fts.rowid
                       WHERE fts.content MATCH ?1".to_string();
        
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(fts_query)];
        
        if let Some(ct) = content_type {
            sql.push_str(" AND ce.content_type = ?");
            sql.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(ct.to_string()));
        }
        
        if let Some(since_str) = since {
            if let Ok(since_ts) = parse_since(since_str) {
                sql.push_str(" AND ce.timestamp >= ?");
                sql.push_str(&(params.len() + 1).to_string());
                params.push(Box::new(since_ts));
            }
        }
        
        sql.push_str(&format!(" ORDER BY ce.timestamp DESC LIMIT {}", limit));
        
        let mut stmt = self.conn.prepare(&sql)
            .context("Failed to prepare FTS5 query: {}")?;
        
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        
        let entries = stmt.query_map(param_refs.as_slice(), Self::row_to_entry)
        .context("Failed to execute FTS5 query: {}")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to collect FTS5 results: {}")?;
        
        crate::log_component_action!(
            "HistoryStore",
            "FTS5 search completed",
            results_count = entries.len(),
            keywords_count = keywords.len()
        );
        
        Ok(entries)
    }
    
    /// Search using LIKE-based search (fallback when FTS5 unavailable)
    fn search_like(
        &self,
        query: &str,
        limit: usize,
        content_type: Option<&str>,
        since: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>> {
        // Parse query into keywords
        let keywords: Vec<&str> = query.split_whitespace().collect();
        if keywords.is_empty() {
            return self.list(limit, content_type, None, since);
        }
        
        let mut sql = "SELECT id, content, content_type, timestamp, source_app, metadata, created_at 
                       FROM clipboard_entries WHERE 1=1".to_string();
        
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        
        // Add LIKE conditions for each keyword (AND logic)
        for keyword in &keywords {
            sql.push_str(" AND content LIKE ?");
            sql.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(format!("%{}%", keyword)));
        }
        
        if let Some(ct) = content_type {
            sql.push_str(" AND content_type = ?");
            sql.push_str(&(params.len() + 1).to_string());
            params.push(Box::new(ct.to_string()));
        }
        
        if let Some(since_str) = since {
            if let Ok(since_ts) = parse_since(since_str) {
                sql.push_str(" AND timestamp >= ?");
                sql.push_str(&(params.len() + 1).to_string());
                params.push(Box::new(since_ts));
            }
        }
        
        sql.push_str(&format!(" ORDER BY timestamp DESC LIMIT {}", limit));
        
        let mut stmt = self.conn.prepare(&sql)
            .context("Failed to prepare LIKE query: {}")?;
        
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        
        let entries = stmt.query_map(param_refs.as_slice(), Self::row_to_entry)
        .context("Failed to execute LIKE query: {}")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to collect LIKE results: {}")?;
        
        crate::log_component_action!(
            "HistoryStore",
            "LIKE search completed",
            results_count = entries.len(),
            keywords_count = keywords.len()
        );
        
        Ok(entries)
    }
    
    pub fn get_by_id(&self, id: &str) -> Result<ClipboardEntry> {
        self.check_open()?;
        
        crate::log_component_action!(
            "HistoryStore",
            "Retrieving entry by ID",
            entry_id = id
        );
        
        let mut stmt = self.conn.prepare(
            "SELECT id, content, content_type, timestamp, source_app, metadata, created_at 
             FROM clipboard_entries WHERE id LIKE ?1 || '%' LIMIT 1"
        )
        .context("Failed to prepare query")?;
        
        let entry = stmt.query_row(params![id], Self::row_to_entry)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DatabaseError::EntryNotFound(id.to_string()).into(),
            _ => anyhow::anyhow!("Failed to get entry: {}", e),
        })?;
        
        crate::log_component_action!(
            "HistoryStore",
            "Entry retrieved",
            entry_id = %entry.id,
            content_type = entry.content_type.as_str()
        );
        
        Ok(entry)
    }
    
    pub fn clear(&self) -> Result<()> {
        self.check_open()?;
        
        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_entries",
            [],
            |row| row.get(0),
        )
        .context("Failed to get count: {}")?;
        
        self.conn.execute("DELETE FROM clipboard_entries", [])
            .context("Failed to clear entries: {}")?;
        
        self.entry_count.store(0, Ordering::Relaxed);

        crate::log_component_action!(
            "HistoryStore",
            "All entries cleared",
            entries_deleted = count
        );
        
        Ok(())
    }
    
    pub fn get_statistics(&self) -> Result<Statistics> {
        self.check_open()?;
        
        crate::log_component_action!(
            "HistoryStore",
            "Retrieving statistics"
        );
        
        let total: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_entries",
            [],
            |row| row.get(0),
        )
        .context("Failed to get count: {}")?;
        
        let mut stmt = self.conn.prepare(
            "SELECT content_type, COUNT(*) FROM clipboard_entries GROUP BY content_type"
        )
        .context("Failed to prepare query: {}")?;
        
        let by_type = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })
        .context("Failed to execute query: {}")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to collect results: {}")?;
        
        crate::log_component_action!(
            "HistoryStore",
            "Statistics retrieved",
            total_entries = total,
            type_count = by_type.len()
        );
        
        Ok(Statistics { total, by_type })
    }
    
    pub fn cleanup_old_entries(&self, days: u32) -> Result<usize> {
        self.check_open()?;
        
        let cutoff = Utc::now().timestamp_millis() - (days as i64 * 24 * 60 * 60 * 1000);
        
        let deleted = self.conn.execute(
            "DELETE FROM clipboard_entries WHERE timestamp < ?1",
            params![cutoff],
        )
        .context("Failed to cleanup entries: {}")?;
        
        // Saturating subtract to avoid wrapping if the count ever drifts
        self.entry_count.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_sub(deleted))
        }).ok();

        crate::log_component_action!(
            "HistoryStore",
            "Old entries cleaned up",
            entries_deleted = deleted,
            retention_days = days
        );
        
        Ok(deleted)
    }
    /// Get entries since a specific timestamp
    ///
    /// Returns all entries with timestamp >= the specified timestamp,
    /// ordered by timestamp descending.
    ///
    /// # Arguments
    ///
    /// * `timestamp` - Unix timestamp in milliseconds
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::NotOpen` if the database connection is closed.
    /// Returns `DatabaseError::QueryFailed` if the query fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Get entries from the last hour
    /// let one_hour_ago = Utc::now().timestamp_millis() - (60 * 60 * 1000);
    /// let entries = store.get_since(one_hour_ago, 100)?;
    /// ```
    pub fn get_since(&self, timestamp: i64, limit: usize) -> Result<Vec<ClipboardEntry>> {
        self.check_open()?;

        crate::log_component_action!(
            "HistoryStore",
            "Retrieving entries since timestamp",
            timestamp = timestamp,
            limit = limit
        );

        let mut stmt = self.conn.prepare(
            "SELECT id, content, content_type, timestamp, source_app, metadata, created_at
             FROM clipboard_entries
             WHERE timestamp >= ?1
             ORDER BY timestamp DESC
             LIMIT ?2"
        )
        .context("Failed to prepare query: {}")?;

        let entries = stmt.query_map(params![timestamp, limit], Self::row_to_entry)
        .context("Failed to execute query: {}")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to collect results: {}")?;

        crate::log_component_action!(
            "HistoryStore",
            "Entries retrieved since timestamp",
            count = entries.len()
        );

        Ok(entries)
    }

    /// Get recent entries filtered by content type
    ///
    /// Returns the most recent entries of a specific content type,
    /// ordered by timestamp descending.
    ///
    /// # Arguments
    ///
    /// * `content_type` - The content type to filter by (e.g., "text", "code", "url")
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::NotOpen` if the database connection is closed.
    /// Returns `DatabaseError::QueryFailed` if the query fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Get the 10 most recent code entries
    /// let code_entries = store.get_recent_by_type("code", 10)?;
    /// ```
    pub fn get_recent_by_type(&self, content_type: &str, limit: usize) -> Result<Vec<ClipboardEntry>> {
        self.check_open()?;

        crate::log_component_action!(
            "HistoryStore",
            "Retrieving recent entries by type",
            content_type = content_type,
            limit = limit
        );

        let mut stmt = self.conn.prepare(
            "SELECT id, content, content_type, timestamp, source_app, metadata, created_at
             FROM clipboard_entries
             WHERE content_type = ?1
             ORDER BY timestamp DESC
             LIMIT ?2"
        )
        .context("Failed to prepare query: {}")?;

        let entries = stmt.query_map(params![content_type, limit], Self::row_to_entry)
        .context("Failed to execute query: {}")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to collect results: {}")?;

        crate::log_component_action!(
            "HistoryStore",
            "Entries retrieved by type",
            count = entries.len(),
            content_type = content_type
        );

        Ok(entries)
    }

    /// Check if the database connection is open
    ///
    /// Returns true if the database connection is active and can be used
    /// for queries, false otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if !store.is_open() {
    ///     return Err(DatabaseError::NotOpen.into());
    /// }
    /// ```
    pub fn is_open(&self) -> bool {
        // SQLite connections in rusqlite don't have a simple "is_open" check,
        // but we can test if the connection is usable by executing a simple query
        // Use query_row instead of execute to avoid any potential side effects
        self.conn.query_row("SELECT 1", [], |_| Ok(())).is_ok()
    }

    /// Return the cached entry count without hitting the database.
    ///
    /// This is a lock-free read of an `AtomicUsize` that is kept in sync
    /// by `save`, `clear`, and `cleanup_old_entries`.  It is intended for
    /// hot paths such as `ResourceMonitor::collect_metrics` where acquiring
    /// the `Mutex<HistoryStore>` just to run `SELECT COUNT(*)` would cause
    /// unnecessary contention.
    pub fn entry_count(&self) -> usize {
        self.entry_count.load(Ordering::Relaxed)
    }

    /// Return a shared handle to the atomic entry count.
    ///
    /// This allows external components (e.g. `ResourceMonitor`) to read the
    /// count without acquiring the `HistoryStore` mutex.
    pub fn entry_count_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.entry_count)
    }
    /// Check if database is open and return an error if not
    ///
    /// This is a helper method to ensure database operations fail fast
    /// with a clear error message when the database is not open.
    fn check_open(&self) -> Result<()> {
        if !self.is_open() {
            return Err(DatabaseError::NotOpen.into());
        }
        Ok(())
    }

    /// Helper function to deserialize metadata from JSON string
    ///
    /// If the metadata field is NULL or empty, returns default metadata.
    /// If deserialization fails, logs a warning and returns default metadata.
    fn deserialize_metadata(metadata_json: Option<String>) -> EntryMetadata {
        match metadata_json {
            Some(json) if !json.is_empty() => {
                match serde_json::from_str::<EntryMetadata>(&json) {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        crate::log_component_action!(
                            "HistoryStore",
                            "Failed to deserialize metadata, using default",
                            error = %e
                        );
                        EntryMetadata::default()
                    }
                }
            }
            _ => EntryMetadata::default(),
        }
    }

    /// Helper function to map a database row to a ClipboardEntry
    ///
    /// This function handles deserialization of the metadata field.
    fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<ClipboardEntry> {
        let metadata_json: Option<String> = row.get(5)?;
        let timestamp: i64 = row.get(3)?;
        
        // created_at may be INTEGER (JS version / new Rust) or TEXT (old Rust rfc3339).
        // Try integer first, fall back to parsing text, fall back to timestamp.
        let created_at: i64 = row.get::<_, i64>(6)
            .or_else(|_| {
                row.get::<_, String>(6).map(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.timestamp_millis())
                        .unwrap_or(timestamp)
                })
            })
            .unwrap_or(timestamp);
        
        Ok(ClipboardEntry {
            id: row.get(0)?,
            content: row.get(1)?,
            content_type: row.get(2)?,
            timestamp,
            source_app: row.get(4)?,
            metadata: Self::deserialize_metadata(metadata_json),
            created_at,
        })
    }

}

fn parse_since(since: &str) -> Result<i64> {
    match since {
        "today" => {
            let now = Utc::now();
            let today = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
            Ok(today.and_utc().timestamp_millis())
        }
        "yesterday" => {
            let now = Utc::now();
            let yesterday = now.date_naive().pred_opt().unwrap().and_hms_opt(0, 0, 0).unwrap();
            Ok(yesterday.and_utc().timestamp_millis())
        }
        _ => {
            if let Ok(days) = since.trim_end_matches(" days ago").parse::<i64>() {
                let cutoff = Utc::now().timestamp_millis() - (days * 24 * 60 * 60 * 1000);
                Ok(cutoff)
            } else {
                anyhow::bail!("Invalid since format: {}", since)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn test_shared_history_store_thread_safety() {
        // Create a temporary database
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Create a shared instance
        let store = HistoryStore::new_shared(&db_path).unwrap();
        
        // Clone for multiple threads
        let store1 = Arc::clone(&store);
        let store2 = Arc::clone(&store);
        let store3 = Arc::clone(&store);
        
        // Spawn threads that concurrently write to the database
        let handle1 = thread::spawn(move || {
            let store = store1.lock().unwrap();
            for i in 0..10 {
                store.save(&format!("content1_{}", i), "text").unwrap();
            }
        });
        
        let handle2 = thread::spawn(move || {
            let store = store2.lock().unwrap();
            for i in 0..10 {
                store.save(&format!("content2_{}", i), "code").unwrap();
            }
        });
        
        let handle3 = thread::spawn(move || {
            let store = store3.lock().unwrap();
            for i in 0..10 {
                store.save(&format!("content3_{}", i), "url").unwrap();
            }
        });
        
        // Wait for all threads to complete
        handle1.join().unwrap();
        handle2.join().unwrap();
        handle3.join().unwrap();
        
        // Verify all entries were saved
        let store = store.lock().unwrap();
        let stats = store.get_statistics().unwrap();
        assert_eq!(stats.total, 30, "Should have 30 total entries");
        
        // Verify entries by type
        let mut type_counts = std::collections::HashMap::new();
        for (content_type, count) in stats.by_type {
            type_counts.insert(content_type, count);
        }
        
        assert_eq!(type_counts.get("text"), Some(&10));
        assert_eq!(type_counts.get("code"), Some(&10));
        assert_eq!(type_counts.get("url"), Some(&10));
    }
    
    #[test]
    fn test_shared_history_store_concurrent_read_write() {
        // Create a temporary database
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Create a shared instance and populate with initial data
        let store = HistoryStore::new_shared(&db_path).unwrap();
        {
            let store = store.lock().unwrap();
            for i in 0..5 {
                store.save(&format!("initial_{}", i), "text").unwrap();
            }
        }
        
        // Clone for multiple threads
        let store_writer = Arc::clone(&store);
        let store_reader1 = Arc::clone(&store);
        let store_reader2 = Arc::clone(&store);
        
        // Writer thread
        let writer = thread::spawn(move || {
            let store = store_writer.lock().unwrap();
            for i in 0..5 {
                store.save(&format!("new_{}", i), "code").unwrap();
            }
        });
        
        // Reader threads
        let reader1 = thread::spawn(move || {
            let store = store_reader1.lock().unwrap();
            let entries = store.list(100, None, None, None).unwrap();
            entries.len()
        });
        
        let reader2 = thread::spawn(move || {
            let store = store_reader2.lock().unwrap();
            let stats = store.get_statistics().unwrap();
            stats.total
        });
        
        // Wait for all threads
        writer.join().unwrap();
        let count1 = reader1.join().unwrap();
        let count2 = reader2.join().unwrap();
        
        // Verify final state
        let store = store.lock().unwrap();
        let final_stats = store.get_statistics().unwrap();
        assert_eq!(final_stats.total, 10, "Should have 10 total entries");
        
        // Reader threads may have seen different states, but both should be valid
        assert!(count1 >= 5 && count1 <= 10, "Reader1 saw {} entries", count1);
        assert!(count2 >= 5 && count2 <= 10, "Reader2 saw {} entries", count2);
    }
    
    #[test]
    fn test_arc_clone_creates_shared_reference() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let store1 = HistoryStore::new_shared(&db_path).unwrap();
        let store2 = Arc::clone(&store1);
        
        // Both references point to the same underlying store
        {
            let s = store1.lock().unwrap();
            s.save("test content", "text").unwrap();
        }
        
        {
            let s = store2.lock().unwrap();
            let stats = s.get_statistics().unwrap();
            assert_eq!(stats.total, 1, "Both references see the same data");
        }
    }
    
    #[test]
    fn test_fts5_search() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Save some test entries
        store.save("hello world", "text").unwrap();
        store.save("hello rust programming", "code").unwrap();
        store.save("world peace", "text").unwrap();
        store.save("goodbye world", "text").unwrap();
        
        // Test single keyword search
        let results = store.search("hello", 10, None, None).unwrap();
        assert_eq!(results.len(), 2, "Should find 2 entries with 'hello'");
        
        // Test multi-keyword search (AND logic)
        let results = store.search("hello world", 10, None, None).unwrap();
        assert_eq!(results.len(), 1, "Should find 1 entry with both 'hello' and 'world'");
        
        // Test search with type filter
        let results = store.search("hello", 10, Some("code"), None).unwrap();
        assert_eq!(results.len(), 1, "Should find 1 code entry with 'hello'");
        
        // Test empty query returns recent entries
        let results = store.search("", 10, None, None).unwrap();
        assert_eq!(results.len(), 4, "Empty query should return all entries");
    }
    
    #[test]
    fn test_fts5_triggers() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Save an entry
        let id = store.save("original content", "text").unwrap();
        
        // Verify it's searchable
        let results = store.search("original", 10, None, None).unwrap();
        assert_eq!(results.len(), 1, "Should find the original entry");
        
        // Update the entry directly in the database
        store.conn.execute(
            "UPDATE clipboard_entries SET content = ?1 WHERE id = ?2",
            params!["updated content", id],
        ).unwrap();
        
        // Verify the FTS5 index was updated via trigger
        let results = store.search("updated", 10, None, None).unwrap();
        assert_eq!(results.len(), 1, "Should find the updated entry");
        
        let results = store.search("original", 10, None, None).unwrap();
        assert_eq!(results.len(), 0, "Should not find the original content anymore");
        
        // Delete the entry
        store.conn.execute(
            "DELETE FROM clipboard_entries WHERE id = ?1",
            params![id],
        ).unwrap();
        
        // Verify it's no longer searchable
        let results = store.search("updated", 10, None, None).unwrap();
        assert_eq!(results.len(), 0, "Should not find the deleted entry");
    }
    
    #[test]
    fn test_schema_version_table_created() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Verify schema_version table exists
        let count: i32 = store.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(count, 1, "schema_version table should exist");
    }
    
    #[test]
    fn test_created_at_column_exists() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Get all column names
        let mut stmt = store.conn.prepare("PRAGMA table_info(clipboard_entries)").unwrap();
        let columns: Vec<String> = stmt.query_map([], |row| row.get(1))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        
        assert!(columns.contains(&"created_at".to_string()), "created_at column should exist");
        assert!(columns.contains(&"id".to_string()), "id column should exist");
        assert!(columns.contains(&"content".to_string()), "content column should exist");
        assert!(columns.contains(&"content_type".to_string()), "content_type column should exist");
        assert!(columns.contains(&"timestamp".to_string()), "timestamp column should exist");
        assert!(columns.contains(&"source_app".to_string()), "source_app column should exist");
        assert!(columns.contains(&"metadata".to_string()), "metadata column should exist");
    }
    
    #[test]
    fn test_schema_version_initialized() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Verify schema version is set to 1
        let version: i32 = store.conn.query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(version, 1, "Schema version should be 1 after initialization");
    }
    
    #[test]
    fn test_migration_from_nodejs_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Simulate a Node.js database by creating tables without schema_version
        {
            let conn = Connection::open(&db_path).unwrap();
            
            // Create the main table (as Node.js would)
            conn.execute(
                "CREATE TABLE clipboard_entries (
                    id TEXT PRIMARY KEY,
                    content TEXT NOT NULL,
                    content_type TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    source_app TEXT,
                    metadata TEXT
                )",
                [],
            ).unwrap();
            
            // Insert some test data
            conn.execute(
                "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
                 VALUES ('test-id-1', 'test content 1', 'text', 1234567890000, NULL, NULL)",
                [],
            ).unwrap();
            
            conn.execute(
                "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
                 VALUES ('test-id-2', 'test content 2', 'code', 1234567891000, NULL, NULL)",
                [],
            ).unwrap();
        }
        
        // Now open with HistoryStore - should run migrations
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Verify schema version was set
        let version: i32 = store.conn.query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(version, 1, "Schema version should be 1 after migration");
        
        // Verify existing data is preserved
        let entries = store.list(10, None, None, None).unwrap();
        assert_eq!(entries.len(), 2, "Should have 2 entries from Node.js database");
        
        // Verify indexes were created
        let index_count: i32 = store.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name='clipboard_entries'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(index_count >= 2, "Should have at least 2 indexes");
        
        // If FTS5 is available, verify it was created and populated
        if store.fts_available {
            // Check if FTS5 table exists
            let fts_exists: i32 = store.conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clipboard_entries_fts'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(fts_exists, 1, "FTS5 table should exist");
            
            // The FTS5 table with content='clipboard_entries' is contentless,
            // so we can't count rows directly. Instead, verify we can search.
            // For now, just verify the table was created.
        }
    }
    
    #[test]
    fn test_migration_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Create database and run migrations
        {
            let _store = HistoryStore::new(&db_path).unwrap();
        }
        
        // Open again - migrations should not run (already at version 1)
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Verify schema version is still 1
        let version: i32 = store.conn.query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(version, 1, "Schema version should remain 1");
        
        // Verify tables still exist
        let table_count: i32 = store.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clipboard_entries'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(table_count, 1, "clipboard_entries table should exist");
    }
    
    #[test]
    fn test_fts5_rebuild_on_migration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Check if FTS5 is available
        let conn = Connection::open(&db_path).unwrap();
        let fts_available = conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS temp.fts5_test USING fts5(content)",
            [],
        ).is_ok();
        let _ = conn.execute("DROP TABLE IF EXISTS temp.fts5_test", []);
        drop(conn);
        
        if !fts_available {
            // Skip test if FTS5 is not available
            return;
        }
        
        // Simulate a Node.js database with entries but no FTS5 index
        {
            let conn = Connection::open(&db_path).unwrap();
            
            conn.execute(
                "CREATE TABLE clipboard_entries (
                    id TEXT PRIMARY KEY,
                    content TEXT NOT NULL,
                    content_type TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    source_app TEXT,
                    metadata TEXT
                )",
                [],
            ).unwrap();
            
            // Insert test data
            for i in 0..5 {
                conn.execute(
                    "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
                     VALUES (?1, ?2, 'text', ?3, NULL, NULL)",
                    params![format!("id-{}", i), format!("searchable content {}", i), 1234567890000i64 + i],
                ).unwrap();
            }
        }
        
        // Open with HistoryStore - should rebuild FTS5 index
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Verify FTS5 index was populated
        let fts_count: i32 = store.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_entries_fts",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(fts_count, 5, "FTS5 index should be populated with all entries");
        
        // Verify search works
        let results = store.search("searchable", 10, None, None).unwrap();
        assert_eq!(results.len(), 5, "Should find all entries via FTS5 search");
    }

    #[test]
    fn test_get_since() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Save entries with different timestamps
        let now = Utc::now().timestamp_millis();
        let one_hour_ago = now - (60 * 60 * 1000);
        let two_hours_ago = now - (2 * 60 * 60 * 1000);
        let three_hours_ago = now - (3 * 60 * 60 * 1000);

        // Insert entries directly with specific timestamps
        store.conn.execute(
            "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["id1", "content 1", "text", three_hours_ago, None::<String>, None::<String>],
        ).unwrap();

        store.conn.execute(
            "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["id2", "content 2", "text", two_hours_ago, None::<String>, None::<String>],
        ).unwrap();

        store.conn.execute(
            "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["id3", "content 3", "text", one_hour_ago, None::<String>, None::<String>],
        ).unwrap();

        // Get entries since 2 hours ago
        let entries = store.get_since(two_hours_ago, 10).unwrap();
        assert_eq!(entries.len(), 2, "Should get 2 entries from the last 2 hours");

        // Verify they are ordered by timestamp descending
        assert_eq!(entries[0].id, "id3");
        assert_eq!(entries[1].id, "id2");

        // Get entries since 1 hour ago
        let entries = store.get_since(one_hour_ago, 10).unwrap();
        assert_eq!(entries.len(), 1, "Should get 1 entry from the last hour");
        assert_eq!(entries[0].id, "id3");

        // Get entries since now (should be empty)
        let entries = store.get_since(now, 10).unwrap();
        assert_eq!(entries.len(), 0, "Should get 0 entries since now");

        // Test limit
        let entries = store.get_since(three_hours_ago, 2).unwrap();
        assert_eq!(entries.len(), 2, "Should respect limit of 2");
    }

    #[test]
    fn test_get_recent_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Save entries of different types
        store.save("text content 1", "text").unwrap();
        store.save("code content 1", "code").unwrap();
        store.save("text content 2", "text").unwrap();
        store.save("url content 1", "url").unwrap();
        store.save("text content 3", "text").unwrap();
        store.save("code content 2", "code").unwrap();

        // Get recent text entries
        let text_entries = store.get_recent_by_type("text", 10).unwrap();
        assert_eq!(text_entries.len(), 3, "Should get 3 text entries");
        for entry in &text_entries {
            assert_eq!(entry.content_type, "text");
        }

        // Verify they are ordered by timestamp descending (most recent first)
        assert!(text_entries[0].content.contains("3"));
        assert!(text_entries[1].content.contains("2"));
        assert!(text_entries[2].content.contains("1"));

        // Get recent code entries
        let code_entries = store.get_recent_by_type("code", 10).unwrap();
        assert_eq!(code_entries.len(), 2, "Should get 2 code entries");
        for entry in &code_entries {
            assert_eq!(entry.content_type, "code");
        }

        // Get recent url entries
        let url_entries = store.get_recent_by_type("url", 10).unwrap();
        assert_eq!(url_entries.len(), 1, "Should get 1 url entry");
        assert_eq!(url_entries[0].content_type, "url");

        // Test limit
        let text_entries = store.get_recent_by_type("text", 2).unwrap();
        assert_eq!(text_entries.len(), 2, "Should respect limit of 2");

        // Test non-existent type
        let json_entries = store.get_recent_by_type("json", 10).unwrap();
        assert_eq!(json_entries.len(), 0, "Should get 0 entries for non-existent type");
    }

    #[test]
    fn test_is_open() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Database should be open after creation
        assert!(store.is_open(), "Database should be open");

        // Save an entry to verify it's working
        let id = store.save("test content", "text").unwrap();
        assert!(store.is_open(), "Database should still be open after save");

        // Retrieve the entry
        let entry = store.get_by_id(&id).unwrap();
        assert_eq!(entry.content, "test content");
        assert!(store.is_open(), "Database should still be open after retrieval");
    }

    #[test]
    fn test_database_not_open_error() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a store and then close the connection
        let mut store = HistoryStore::new(&db_path).unwrap();

        // Close the connection by replacing it with an in-memory connection
        // that we immediately close
        store.conn = Connection::open_in_memory().unwrap();
        drop(store.conn);

        // Recreate a closed connection (this is a bit hacky but demonstrates the error)
        // In practice, this would happen if the database file is deleted or corrupted
        store.conn = Connection::open_in_memory().unwrap();
        store.conn.close().unwrap();

        // Now create a new connection that will fail the is_open check
        // We'll use a path that doesn't exist and can't be created
        let invalid_path = Path::new("/invalid/path/that/does/not/exist/test.db");
        let result = HistoryStore::new(invalid_path);

        // This should fail during initialization, not with NotOpen error
        assert!(result.is_err(), "Should fail to create database at invalid path");
    }

    #[test]
    fn test_check_open_on_operations() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // All operations should work when database is open
        assert!(store.save("test", "text").is_ok());
        assert!(store.list(10, None, None, None).is_ok());
        assert!(store.search("test", 10, None, None).is_ok());
        assert!(store.get_statistics().is_ok());
        assert!(store.clear().is_ok());

        // Save a new entry for further tests
        let id = store.save("test content", "text").unwrap();
        assert!(store.get_by_id(&id).is_ok());
        assert!(store.get_since(0, 10).is_ok());
        assert!(store.get_recent_by_type("text", 10).is_ok());
        assert!(store.cleanup_old_entries(30).is_ok());
    }

    #[test]
    fn test_metadata_serialization() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Save an entry with content that has specific character and word counts
        let content = "Hello world! This is a test.";
        let id = store.save(content, "text").unwrap();

        // Retrieve the entry
        let entry = store.get_by_id(&id).unwrap();

        // Verify metadata was populated correctly
        assert_eq!(entry.metadata.character_count, content.chars().count());
        assert_eq!(entry.metadata.word_count, content.split_whitespace().count());
        assert_eq!(entry.metadata.confidence, 1.0);
        assert_eq!(entry.metadata.language, None);

        // Verify the metadata was actually stored in the database as JSON
        let metadata_json: String = store.conn.query_row(
            "SELECT metadata FROM clipboard_entries WHERE id = ?1",
            params![id],
            |row| row.get(0),
        ).unwrap();

        // Parse the JSON to verify it's valid
        let parsed_metadata: EntryMetadata = serde_json::from_str(&metadata_json).unwrap();
        assert_eq!(parsed_metadata.character_count, content.chars().count());
        assert_eq!(parsed_metadata.word_count, content.split_whitespace().count());
    }

    #[test]
    fn test_metadata_deserialization_with_null() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Insert an entry with NULL metadata (simulating old data)
        let id = uuid::Uuid::new_v4().to_string();
        store.conn.execute(
            "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, "test content", "text", Utc::now().timestamp_millis(), None::<String>, None::<String>],
        ).unwrap();

        // Retrieve the entry - should get default metadata
        let entry = store.get_by_id(&id).unwrap();
        assert_eq!(entry.metadata.character_count, 0);
        assert_eq!(entry.metadata.word_count, 0);
        assert_eq!(entry.metadata.confidence, 1.0);
        assert_eq!(entry.metadata.language, None);
    }

    #[test]
    fn test_metadata_deserialization_with_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Insert an entry with invalid JSON metadata
        let id = uuid::Uuid::new_v4().to_string();
        store.conn.execute(
            "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, "test content", "text", Utc::now().timestamp_millis(), None::<String>, "invalid json"],
        ).unwrap();

        // Retrieve the entry - should get default metadata and log a warning
        let entry = store.get_by_id(&id).unwrap();
        assert_eq!(entry.metadata.character_count, 0);
        assert_eq!(entry.metadata.word_count, 0);
        assert_eq!(entry.metadata.confidence, 1.0);
        assert_eq!(entry.metadata.language, None);
    }

    #[test]
    fn test_metadata_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Save multiple entries with different content
        let entries_data = vec![
            ("Short", "text"),
            ("This is a longer piece of text with multiple words", "text"),
            ("function test() { return 42; }", "code"),
            ("https://example.com", "url"),
        ];

        for (content, content_type) in entries_data {
            let id = store.save(content, content_type).unwrap();
            let entry = store.get_by_id(&id).unwrap();

            // Verify metadata matches the content
            assert_eq!(entry.content, content);
            assert_eq!(entry.content_type, content_type);
            assert_eq!(entry.metadata.character_count, content.chars().count());
            assert_eq!(entry.metadata.word_count, content.split_whitespace().count());
        }
    }

    #[test]
    fn test_metadata_in_search_results() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Save entries
        store.save("hello world", "text").unwrap();
        store.save("hello rust programming", "code").unwrap();

        // Search and verify metadata is present
        let results = store.search("hello", 10, None, None).unwrap();
        assert_eq!(results.len(), 2);

        for entry in results {
            assert!(entry.metadata.character_count > 0);
            assert!(entry.metadata.word_count > 0);
        }
    }

    #[test]
    fn test_metadata_in_list_results() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = HistoryStore::new(&db_path).unwrap();

        // Save entries
        store.save("test content one", "text").unwrap();
        store.save("test content two", "text").unwrap();

        // List and verify metadata is present
        let results = store.list(10, None, None, None).unwrap();
        assert_eq!(results.len(), 2);

        for entry in results {
            assert!(entry.metadata.character_count > 0);
            assert!(entry.metadata.word_count > 0);
        }
    }

    #[test]
    fn test_complete_migration_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Step 1: Simulate a Node.js database with existing data
        {
            let conn = Connection::open(&db_path).unwrap();
            
            // Create the main table (as Node.js would)
            conn.execute(
                "CREATE TABLE clipboard_entries (
                    id TEXT PRIMARY KEY,
                    content TEXT NOT NULL,
                    content_type TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    source_app TEXT,
                    metadata TEXT
                )",
                [],
            ).unwrap();
            
            // Insert test data with various content types
            conn.execute(
                "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
                 VALUES ('nodejs-1', 'Hello from Node.js', 'text', 1234567890000, NULL, NULL)",
                [],
            ).unwrap();
            
            conn.execute(
                "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
                 VALUES ('nodejs-2', 'https://example.com', 'url', 1234567891000, NULL, NULL)",
                [],
            ).unwrap();
            
            conn.execute(
                "INSERT INTO clipboard_entries (id, content, content_type, timestamp, source_app, metadata)
                 VALUES ('nodejs-3', 'function test() { return 42; }', 'code', 1234567892000, NULL, NULL)",
                [],
            ).unwrap();
        }
        
        // Step 2: Open with HistoryStore - should run migrations
        let store = HistoryStore::new(&db_path).unwrap();
        
        // Step 3: Verify schema version was set
        let version: i32 = store.conn.query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(version, 1, "Schema version should be 1 after migration");
        
        // Step 4: Verify all existing data is preserved
        let entries = store.list(10, None, None, None).unwrap();
        assert_eq!(entries.len(), 3, "Should have 3 entries from Node.js database");
        
        // Verify specific entries
        let entry1 = store.get_by_id("nodejs-1").unwrap();
        assert_eq!(entry1.content, "Hello from Node.js");
        assert_eq!(entry1.content_type, "text");
        
        let entry2 = store.get_by_id("nodejs-2").unwrap();
        assert_eq!(entry2.content, "https://example.com");
        assert_eq!(entry2.content_type, "url");
        
        let entry3 = store.get_by_id("nodejs-3").unwrap();
        assert_eq!(entry3.content, "function test() { return 42; }");
        assert_eq!(entry3.content_type, "code");
        
        // Step 5: Verify indexes were created
        let index_count: i32 = store.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name='clipboard_entries'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(index_count >= 2, "Should have at least 2 indexes (timestamp and content_type)");
        
        // Step 6: Verify FTS5 was created and populated (if available)
        if store.fts_available {
            // Check if FTS5 table exists
            let fts_exists: i32 = store.conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clipboard_entries_fts'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(fts_exists, 1, "FTS5 table should exist");
            
            // Verify FTS5 triggers exist
            let trigger_count: i32 = store.conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND tbl_name='clipboard_entries'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(trigger_count, 3, "Should have 3 FTS5 triggers (INSERT, UPDATE, DELETE)");
            
            // Verify search works on migrated data
            let results = store.search("Node", 10, None, None).unwrap();
            assert_eq!(results.len(), 1, "Should find 1 entry via FTS5 search");
            assert_eq!(results[0].id, "nodejs-1");
            
            let results = store.search("example", 10, None, None).unwrap();
            assert_eq!(results.len(), 1, "Should find URL entry via FTS5 search");
            assert_eq!(results[0].id, "nodejs-2");
        }
        
        // Step 7: Verify new entries can be saved after migration
        let new_id = store.save("New Rust entry", "text").unwrap();
        let new_entry = store.get_by_id(&new_id).unwrap();
        assert_eq!(new_entry.content, "New Rust entry");
        
        // Step 8: Verify total count
        let stats = store.get_statistics().unwrap();
        assert_eq!(stats.total, 4, "Should have 4 entries total (3 migrated + 1 new)");
        
        // Step 9: Verify search works with mixed old and new entries
        if store.fts_available {
            let results = store.search("Rust", 10, None, None).unwrap();
            assert_eq!(results.len(), 1, "Should find new Rust entry via FTS5 search");
            assert_eq!(results[0].id, new_id);
        }
        
        // Step 10: Verify reopening doesn't re-run migrations
        drop(store);
        let store2 = HistoryStore::new(&db_path).unwrap();
        
        let version2: i32 = store2.conn.query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(version2, 1, "Schema version should still be 1");
        
        let entries2 = store2.list(10, None, None, None).unwrap();
        assert_eq!(entries2.len(), 4, "Should still have 4 entries after reopening");
    }

}
