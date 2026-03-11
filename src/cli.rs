use clap::{Parser, Subcommand};
use std::path::Path;
use crate::errors::{Context, Result};

#[derive(Parser)]
#[command(name = "clipkeeper")]
#[command(about = "Smart clipboard history manager with automatic content classification and privacy filtering", long_about = None)]
#[command(version = "0.3.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the background clipboard monitoring service
    Start {
        /// Keep process in foreground with monitoring output
        #[arg(long)]
        monitor: bool,
        /// Run as background service (internal use, hidden from help)
        #[arg(long, hide = true)]
        service: bool,
    },
    /// Stop the background monitoring service
    Stop,
    /// Check service status with statistics
    Status,
    /// List recent clipboard entries
    List {
        #[arg(short, long, default_value = "10")]
        limit: usize,
        #[arg(short = 't', long)]
        content_type: Option<String>,
        #[arg(short, long)]
        search: Option<String>,
        #[arg(long)]
        since: Option<String>,
        /// Output format: table, json, csv
        #[arg(short, long, default_value = "table")]
        format: String,
        /// Disable interactive mode
        #[arg(long)]
        no_interactive: bool,
    },
    /// Search clipboard history by text
    Search {
        query: String,
        #[arg(short, long, default_value = "10")]
        limit: usize,
        #[arg(short = 't', long)]
        content_type: Option<String>,
        #[arg(long)]
        since: Option<String>,
        /// Disable interactive mode
        #[arg(long)]
        no_interactive: bool,
    },
    /// Copy a clipboard entry back to clipboard
    Copy {
        /// Entry ID
        id: String,
    },
    /// Clear all clipboard history
    Clear {
        /// Skip confirmation prompt
        #[arg(long)]
        confirm: bool,
    },
    /// Show resource metrics
    Metrics {
        /// Show metrics history
        #[arg(long)]
        history: bool,
        /// Number of history samples to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Clear metrics log
        #[arg(long)]
        clear: bool,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Display all configuration settings
    Show,
    /// Get a specific configuration value
    Get { key: String },
    /// Set a configuration value
    Set { key: String, value: String },
}

use crate::config::Config;
use crate::service::ServiceManager;
use crate::history_store::HistoryStore;

// ═══════════════════════════════════════════════════════════════
// Box drawing characters for table formatting (Task 14.1)
// ═══════════════════════════════════════════════════════════════
const TOP_LEFT: &str = "╔";
const TOP_RIGHT: &str = "╗";
const BOTTOM_LEFT: &str = "╚";
const BOTTOM_RIGHT: &str = "╝";
const HORIZONTAL: &str = "═";
const VERTICAL: &str = "║";
const T_DOWN: &str = "╦";
const T_UP: &str = "╩";
const T_RIGHT: &str = "╠";
const T_LEFT: &str = "╣";
const CROSS: &str = "╬";
const CHECK: &str = "✓";
const CROSS_MARK: &str = "✗";

fn format_relative_time(timestamp_ms: i64) -> String {
    let now = crate::time_utils::now_millis();
    let diff_secs = (now - timestamp_ms) / 1000;
    if diff_secs < 60 { return "Just now".to_string(); }
    if diff_secs < 3600 { return format!("{} mins ago", diff_secs / 60); }
    if diff_secs < 86400 { return format!("{} hours ago", diff_secs / 3600); }
    format!("{} days ago", diff_secs / 86400)
}

fn format_preview(content: &str, max_len: usize) -> String {
    let single_line = content.replace('\n', " ").replace('\r', "");
    if single_line.len() <= max_len {
        single_line
    } else {
        format!("{}...", &single_line[..max_len.saturating_sub(3)])
    }
}

fn print_table_row(widths: &[usize], values: &[&str]) {
    let mut parts = Vec::new();
    for (i, val) in values.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(10);
        parts.push(format!(" {:<width$} ", val, width = w));
    }
    println!("{}{}{}", VERTICAL, parts.join(VERTICAL), VERTICAL);
}

fn print_table_border(widths: &[usize], left: &str, mid: &str, right: &str) {
    let segments: Vec<String> = widths.iter().map(|w| HORIZONTAL.repeat(w + 2)).collect();
    println!("{}{}{}", left, segments.join(mid), right);
}

fn format_uptime(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 { return format!("{}s", secs); }
    if secs < 3600 { return format!("{}m {}s", secs / 60, secs % 60); }
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    if hours < 24 { return format!("{}h {}m", hours, mins); }
    format!("{}d {}h", hours / 24, hours % 24)
}

pub fn handle_start(monitor: bool, service: bool) -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;

    // --service: we're the spawned background process, just run directly
    if service {
        // Ensure data directory exists
        std::fs::create_dir_all(&config.storage.data_dir)
            .context("Failed to create data directory")?;
        let pid_file = config.storage.data_dir.join("clipkeeper.pid");
        std::fs::write(&pid_file, std::process::id().to_string())
            .context("Failed to write PID file")?;
        crate::app::run_service(monitor)?;
        return Ok(());
    }

    let service_manager = ServiceManager::new(&config).with_monitor(monitor);

    if service_manager.is_running()? {
        let pid = service_manager.get_pid()?;
        if monitor {
            // Send SIGUSR1 to enable monitoring on the running service
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                kill(Pid::from_raw(pid as i32), Signal::SIGUSR1)
                    .context("Failed to send monitor signal to service")?;
                println!("{} Resource monitoring enabled on running service (PID: {})", CHECK, pid);
                println!("View metrics with: clipkeeper metrics");
            }
            #[cfg(not(unix))]
            {
                println!("Enabling monitoring on a running service is not supported on this platform.");
                println!("Please stop and restart with: clipkeeper start --monitor");
            }
        } else {
            println!("{} Service is already running (PID: {})", CHECK, pid);
        }
        return Ok(());
    }

    println!("Starting clipkeeper service...");
    service_manager.start().context("Failed to start service")?;
    println!("{} Service started successfully", CHECK);
    if monitor {
        println!("Resource monitoring is enabled. View metrics with: clipkeeper metrics");
    }
    println!("Use \"clipkeeper stop\" to stop the service.");
    Ok(())
}

pub fn handle_stop() -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;
    let service_manager = ServiceManager::new(&config);

    if !service_manager.is_running()? {
        println!("{} Service is not running", CROSS_MARK);
        return Ok(());
    }

    let pid = service_manager.get_pid()?;
    service_manager.stop().context("Failed to stop service")?;
    println!("{} Service stopped (PID: {})", CHECK, pid);
    Ok(())
}

pub fn handle_status() -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;
    let service_manager = ServiceManager::new(&config);
    let store = HistoryStore::new(&config.storage.get_db_path())
        .context("Failed to open database")?;

    let running = service_manager.is_running()?;

    println!("\nclipkeeper Service Status:");
    println!("{}", "═".repeat(60));

    if running {
        let pid = service_manager.get_pid()?;
        let uptime_str = service_manager.get_uptime()
            .map(format_uptime)
            .unwrap_or_else(|| "unknown".to_string());
        println!("Status:   {} Running", CHECK);
        println!("PID:      {}", pid);
        println!("Uptime:   {}", uptime_str);
    } else {
        println!("Status:   {} Not running", CROSS_MARK);
    }

    let stats = store.get_statistics().context("Failed to get statistics")?;

    // Last activity
    let last_activity = store.list(1, None, None, None)
        .ok()
        .and_then(|entries| entries.first().map(|e| {
            let now = crate::time_utils::now_millis();
            let diff_ms = now - e.timestamp;
            if diff_ms < 60_000 {
                "Just now".to_string()
            } else if diff_ms < 3_600_000 {
                let mins = diff_ms / 60_000;
                format!("{} minute{} ago", mins, if mins > 1 { "s" } else { "" })
            } else if diff_ms < 86_400_000 {
                let hours = diff_ms / 3_600_000;
                format!("{} hour{} ago", hours, if hours > 1 { "s" } else { "" })
            } else {
                let days = diff_ms / 86_400_000;
                format!("{} day{} ago", days, if days > 1 { "s" } else { "" })
            }
        }))
        .unwrap_or_else(|| "Never".to_string());

    println!("\nClipboard History:");
    println!("  Total entries:  {}", stats.total);
    println!("  Last activity:  {}", last_activity);

    if !stats.by_type.is_empty() {
        println!("\nEntries by type:");
        for (content_type, count) in &stats.by_type {
            println!("  {:<12} {}", content_type, count);
        }
    }

    println!("\nPID File: {}", service_manager.pid_file_path().display());
    println!("{}", "═".repeat(60));

    Ok(())
}

pub fn handle_list(
    limit: usize,
    content_type: Option<String>,
    search: Option<String>,
    since: Option<String>,
    format: String,
    no_interactive: bool,
) -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;
    let store = HistoryStore::new(&config.storage.get_db_path())
        .context("Failed to list entries")?;

    let entries = store.list(limit, content_type.as_deref(), search.as_deref(), since.as_deref())
        .context("Failed to list entries")?;

    if entries.is_empty() {
        if search.is_some() {
            println!("No clipboard entries found matching \"{}\".", search.unwrap());
        } else if content_type.is_some() {
            println!("No clipboard entries found with type \"{}\".", content_type.unwrap());
        } else if since.is_some() {
            println!("No clipboard entries found since {}.", since.unwrap());
        } else {
            println!("No clipboard entries found.");
        }
        return Ok(());
    }

    // Get total count for summary
    let total_count = if search.is_some() {
        entries.len()
    } else {
        let stats = store.get_statistics().context("Failed to get statistics")?;
        if let Some(ref ct) = content_type {
            stats.by_type.iter().find(|(t, _)| t == ct).map(|(_, c)| *c).unwrap_or(0)
        } else {
            stats.total
        }
    };

    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&entries)?),
        "csv" => {
            println!("id,content_type,timestamp,preview");
            for entry in &entries {
                let preview = format_preview(&entry.content, 50);
                println!("{},{},{},{}", entry.id, entry.content_type, entry.timestamp, preview);
            }
        }
        _ => {
            // Interactive mode (Task 13.1) - only when TTY and not --no-interactive
            let is_tty = atty::is(atty::Stream::Stdout);
            if is_tty && !no_interactive {
                handle_interactive_list(&entries, &store)?;
            } else {
                println!("\nClipboard History:");
                println!("{}", "─".repeat(100));
                display_entries_table(&entries);
                println!("{}", "─".repeat(100));
                println!("\nShowing {} of {} total entries", entries.len(), total_count);
                println!("\nUse \"clipkeeper copy <id>\" to copy an entry back to clipboard");
            }
        }
    }

    Ok(())
}

fn display_entries_table(entries: &[crate::history_store::ClipboardEntry]) {
    let widths = [8, 12, 16, 40];
    print_table_border(&widths, TOP_LEFT, T_DOWN, TOP_RIGHT);
    print_table_row(&widths, &["ID", "Type", "Time", "Preview"]);
    print_table_border(&widths, T_RIGHT, CROSS, T_LEFT);
    for entry in entries {
        let id_str = entry.id.to_string();
        let id_short = &id_str[..8];
        let time = format_relative_time(entry.timestamp);
        let preview = format_preview(&entry.content, 40);
        print_table_row(&widths, &[id_short, entry.content_type.as_str(), &time, &preview]);
    }
    print_table_border(&widths, BOTTOM_LEFT, T_UP, BOTTOM_RIGHT);
}

fn handle_interactive_list(
    entries: &[crate::history_store::ClipboardEntry],
    _store: &HistoryStore,
) -> Result<()> {
    use dialoguer::Select;

    let items: Vec<String> = entries.iter().map(|e| {
        let preview = format_preview(&e.content, 60);
        let time = format_relative_time(e.timestamp);
        format!("[{}] {} - {}", e.content_type.as_str(), preview, time)
    }).collect();

    let selection = Select::new()
        .with_prompt("Select entry to copy (Esc to cancel)")
        .items(&items)
        .default(0)
        .interact_opt()
        .context("Interactive selection failed")?;

    match selection {
        Some(idx) => {
            let entry = &entries[idx];
            let clipboard_service = crate::clipboard_service::ClipboardService::new()
                .context("Failed to initialize clipboard")?;
            clipboard_service.copy(&entry.content)
                .context("Failed to copy to clipboard")?;
            println!("{} Copied to clipboard", CHECK);
        }
        None => println!("Cancelled"),
    }

    Ok(())
}

pub fn handle_search(
    query: &str,
    limit: usize,
    content_type: Option<String>,
    since: Option<String>,
    no_interactive: bool,
) -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;
    let store = HistoryStore::new(&config.storage.get_db_path())
        .context("Failed to open database")?;

    let entries = store.search(query, limit, content_type.as_deref(), since.as_deref())
        .context("Failed to search entries")?;

    if entries.is_empty() {
        println!("\nNo results found.");
        println!("\nTry a different search query or check your filters.");
        return Ok(());
    }

    // Interactive mode is default (unless --no-interactive is specified)
    let is_tty = atty::is(atty::Stream::Stdout);
    if is_tty && !no_interactive {
        handle_interactive_list(&entries, &store)?;
        return Ok(());
    }

    // Non-interactive mode - display table with IDs
    println!("\nSearch Results:");
    println!("{}", "─".repeat(100));

    let widths = [8, 12, 16, 40];
    print_table_border(&widths, TOP_LEFT, T_DOWN, TOP_RIGHT);
    print_table_row(&widths, &["ID", "Type", "Time", "Preview"]);
    print_table_border(&widths, T_RIGHT, CROSS, T_LEFT);
    for entry in &entries {
        let id_str = entry.id.to_string();
        let id_short = &id_str[..8];
        let time = format_relative_time(entry.timestamp);
        let preview = format_preview(&entry.content, 40);
        print_table_row(&widths, &[id_short, entry.content_type.as_str(), &time, &preview]);
    }
    print_table_border(&widths, BOTTOM_LEFT, T_UP, BOTTOM_RIGHT);

    println!("{}", "─".repeat(100));
    println!("\nFound {} matching entries", entries.len());
    println!("\nUse \"clipkeeper copy <id>\" to copy an entry back to clipboard");

    Ok(())
}

pub fn handle_copy(id: &str) -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;
    let store = HistoryStore::new(&config.storage.get_db_path())
        .context("Failed to open database")?;

    let entry = store.get_by_id(id).context("Failed to get entry")?;
    let clipboard_service = crate::clipboard_service::ClipboardService::new()
        .context("Failed to initialize clipboard service")?;
    clipboard_service.copy(&entry.content).context("Failed to copy to clipboard")?;
    println!("{} Copied entry {} to clipboard", CHECK, id);
    Ok(())
}

/// Clear with confirmation prompt (Task 15.1)
pub fn handle_clear(confirm: bool) -> Result<()> {
    if !confirm {
        // Interactive confirmation
        use dialoguer::Confirm;
        let proceed = Confirm::new()
            .with_prompt("Are you sure you want to clear all clipboard history?")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !proceed {
            println!("Cancelled");
            return Ok(());
        }
    }

    let config = Config::load().context("Failed to load configuration")?;
    let store = HistoryStore::new(&config.storage.get_db_path())
        .context("Failed to open database")?;
    store.clear().context("Failed to clear history")?;
    println!("{} Clipboard history cleared", CHECK);
    Ok(())
}

/// Metrics command (Task 15.2)
pub fn handle_metrics(_history: bool, limit: usize, clear: bool) -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;
    let data_dir = &config.storage.data_dir;
    let metrics_path = Path::new(data_dir).join("metrics.log");

    if clear {
        if metrics_path.exists() {
            std::fs::remove_file(&metrics_path).context("Failed to clear metrics file")?;
        }
        println!("{} Metrics log cleared", CHECK);
        return Ok(());
    }

    // Try to read metrics from file first (written by the running service)
    if metrics_path.exists() {
        let content = std::fs::read_to_string(&metrics_path)
            .context("Failed to read metrics file")?;
        let lines: Vec<&str> = content.trim().split('\n').filter(|l| !l.is_empty()).collect();

        if !lines.is_empty() {
            // Parse JSON lines
            let metrics: Vec<serde_json::Value> = lines.iter()
                .rev()
                .take(limit)
                .rev()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            if !metrics.is_empty() {
                let latest = &metrics[metrics.len() - 1];
                let first = &metrics[0];

                println!("\nResource Usage Metrics");
                println!("{}", "═".repeat(60));

                // Period info
                if let (Some(start), Some(end)) = (first.get("datetime"), latest.get("datetime")) {
                    println!("\nPeriod: {} to {}", start.as_str().unwrap_or("?"), end.as_str().unwrap_or("?"));
                }
                println!("Samples: {}", metrics.len());

                if let Some(uptime) = latest.get("uptime_secs") {
                    let secs = uptime.as_u64().unwrap_or(0);
                    println!("Uptime: {}", format_uptime(std::time::Duration::from_secs(secs)));
                }

                // Memory
                if let Some(mem) = latest.get("memory_rss_mb") {
                    let rss_values: Vec<f64> = metrics.iter()
                        .filter_map(|m| m.get("memory_rss_mb").and_then(|v| v.as_f64()))
                        .collect();
                    let current = mem.as_f64().unwrap_or(0.0);
                    let min = rss_values.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max = rss_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let avg = rss_values.iter().sum::<f64>() / rss_values.len() as f64;

                    println!("\nMemory (MB):");
                    println!("  RSS (Resident Set Size):");
                    println!("    Current: {:.2} MB", current);
                    println!("    Min:     {:.2} MB", min);
                    println!("    Max:     {:.2} MB", max);
                    println!("    Avg:     {:.2} MB", avg);
                }

                // CPU
                if let Some(cpu) = latest.get("cpu_usage_percent") {
                    println!("\nCPU Usage:");
                    println!("  Current: {:.1}%", cpu.as_f64().unwrap_or(0.0));
                }

                // System
                println!("\nSystem:");
                println!("  Platform:      {}", std::env::consts::OS);
                println!("  Architecture:  {}", std::env::consts::ARCH);
                let (total_mem, avail_mem) = crate::resource_monitor::read_meminfo();
                println!("  Total Memory:  {} MB", total_mem / 1024 / 1024);
                println!("  Free Memory:   {} MB", avail_mem / 1024 / 1024);

                // Database
                if let Some(db_size) = latest.get("database_size_kb") {
                    println!("\nDatabase:");
                    println!("  File size:     {:.1} KB", db_size.as_f64().unwrap_or(0.0));
                }
                if let Some(entries) = latest.get("entry_count") {
                    println!("  Total entries: {}", entries.as_u64().unwrap_or(0));
                }

                println!("\n{}", "═".repeat(60));
                println!("\nMetrics file: {}", metrics_path.display());
                println!("Sampling interval: Every 300 seconds");
                println!("Use --limit to show more samples, or --clear to reset metrics");
                return Ok(());
            }
        }
    }

    // Fallback: show live snapshot if no metrics file
    let store = HistoryStore::new(&config.storage.get_db_path())
        .context("Failed to open database")?;
    let stats = store.get_statistics().context("Failed to get statistics")?;

    let db_path = config.storage.get_db_path();
    let db_size = std::fs::metadata(&db_path)
        .map(|m| m.len() as f64 / 1024.0)
        .unwrap_or(0.0);

    println!("\nNo metrics data available from service.");
    println!("Start the service with --monitor flag to collect metrics:");
    println!("  clipkeeper start --monitor\n");
    println!("Current snapshot:");
    println!("  Database:     {:.1} KB", db_size);
    println!("  Entries:      {}", stats.total);

    Ok(())
}

pub fn handle_config(action: ConfigAction) -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;

    match action {
        ConfigAction::Show => {
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigAction::Get { key } => {
            let value = config.get(&key).context("Failed to get configuration value")?;
            println!("{}", value);
        }
        ConfigAction::Set { key, value } => {
            let mut config = config;
            config.set(&key, &value).context("Failed to set configuration value")?;
            config.save().context("Failed to save configuration")?;
            println!("{} Configuration updated: {} = {}", CHECK, key, value);
        }
    }

    Ok(())
}
