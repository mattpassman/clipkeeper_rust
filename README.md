# clipkeeper

A smart clipboard history manager with automatic content classification and privacy filtering — written in Rust.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org)

## Overview

clipkeeper runs in the background and automatically captures everything you copy to your clipboard. It stores your clipboard history locally with intelligent content classification and privacy filtering, making it easy to find and reuse previously copied content.

This is the Rust implementation of [clipkeeper](https://github.com/mattpassman/clipkeeper), offering the same features with native performance and lower resource usage.

## Features

**Currently Available:**
- **Background Monitoring** - Automatically captures all clipboard activity
- **Resource Monitoring** - Track memory, CPU, and database metrics over time
- **Local Storage** - All data stored in SQLite on your machine
- **Content Classification** - Automatically detects content types (text, code, URLs, JSON, XML, markdown, file_path, image)
- **Language Detection** - Identifies JavaScript, TypeScript, Python, Java, C++, Go, Rust, Ruby, PHP, SQL
- **Privacy Filtering** - Automatically blocks passwords, credit cards, API keys, SSH keys
- **Custom Patterns** - Add your own regex patterns for privacy filtering
- **Text Search** - Search clipboard history with keywords and filters
- **Copy History** - Copy previous clipboard entries back to clipboard
- **Auto Retention** - Automatically clean up old entries based on retention policy
- **List & Filter** - View history with filtering by content type, date, and text
- **Configuration** - Customize retention period, poll interval, and privacy settings
- **Cross-Platform** - Works on Windows, macOS, and Linux

**Planned Features:**
- Semantic search with natural language queries
- LLM embedding integration (OpenAI, Anthropic, Ollama)
- Vector similarity search

## Installation

### Windows

1. Download `clipkeeper.exe` from the [latest release](https://github.com/mattpassman/clipkeeper-rust/releases/latest)
2. Move it to a folder in your PATH, for example `C:\Users\<YourUser>\bin\`
3. If the folder isn't already in your PATH, add it:

**Add to PATH (PowerShell — run once):**
```powershell
$binDir = "$env:USERPROFILE\bin"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null
[Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path', 'User'));$binDir", "User")
```

Restart your terminal, then verify:
```powershell
clipkeeper status
```

### Quick Install (Linux/macOS)

```bash
git clone https://github.com/mattpassman/clipkeeper-rust.git
cd clipkeeper-rust
./install.sh
```

This builds a release binary and installs it to `~/.local/bin/clipkeeper`.

### Build from Source

```bash
cargo build --release
cp target/release/clipkeeper ~/.local/bin/
```

**Requirements:**
- Rust 1.70+
- On Linux: X11 or Wayland clipboard support
- Windows, macOS, or Linux

## Quick Start

### 1. Start the Background Service

```bash
clipkeeper start
```

The service will run in the background and monitor your clipboard automatically.

### 2. Copy Some Content

Copy anything to your clipboard — text, code, URLs, etc. clipkeeper captures it all.

### 3. View Your History

```bash
# List last 10 entries
clipkeeper list

# List last 50 entries
clipkeeper list --limit 50

# Filter by content type
clipkeeper list --content-type url
clipkeeper list --content-type code
clipkeeper list --content-type json

# Search clipboard history
clipkeeper search "error message"
clipkeeper search "function" --content-type code
clipkeeper search "https" --limit 20

# Filter by date
clipkeeper list --since 2024-01-01
clipkeeper list --since yesterday
clipkeeper list --since "10 days ago"
```

Available content types: `text`, `code`, `url`, `json`, `xml`, `markdown`, `file_path`, `image`

### 4. Copy Previous Entries

```bash
# List entries with IDs
clipkeeper list

# Copy an entry back to clipboard by ID
clipkeeper copy <entry-id>
```

### 5. Check Service Status

```bash
clipkeeper status
```

Shows service status, uptime, total entries, and breakdown by content type.

### 6. Stop the Service

```bash
clipkeeper stop
```

## Commands

### Service Management

| Command | Description |
|---------|-------------|
| `clipkeeper start` | Start the background monitoring service |
| `clipkeeper start --monitor` | Start in foreground with monitoring output |
| `clipkeeper stop` | Stop the background service |
| `clipkeeper status` | Check service status with statistics |
| `clipkeeper metrics [options]` | View resource usage metrics |
| `  --limit <number>` | Number of metrics to show (default: 10) |
| `  --clear` | Clear all metrics history |
| `  --history` | Show metrics history |

### Clipboard History

| Command | Description |
|---------|-------------|
| `clipkeeper list [options]` | List recent clipboard entries |
| `  --limit <number>` | Number of entries to show (default: 10) |
| `  --content-type <type>` | Filter by content type |
| `  --search <text>` | Filter by text content |
| `  --since <date>` | Show entries after date (YYYY-MM-DD, "yesterday", "today") |
| `  --format <format>` | Output format: table, json, csv (default: table) |
| `  --no-interactive` | Disable interactive selection |
| `clipkeeper search <query> [options]` | Search clipboard history by text |
| `  --limit <number>` | Maximum results (default: 10) |
| `  --content-type <type>` | Filter by content type |
| `  --since <date>` | Only entries after date |
| `  --no-interactive` | Disable interactive selection |
| `clipkeeper copy <id>` | Copy a clipboard entry back to clipboard |
| `clipkeeper clear [--confirm]` | Clear all clipboard history |

### Configuration

| Command | Description |
|---------|-------------|
| `clipkeeper config show` | Display all configuration settings |
| `clipkeeper config get <key>` | Get a specific configuration value |
| `clipkeeper config set <key> <value>` | Set a configuration value |

## Configuration

clipkeeper stores configuration in:
- **Windows:** `%APPDATA%\clipkeeper\config.json`
- **macOS:** `~/Library/Application Support/clipkeeper/config.json`
- **Linux:** `~/.config/clipkeeper/config.json`

### Common Settings

```bash
# Set retention period (days)
clipkeeper config set retention.days 60

# Adjust clipboard polling interval (milliseconds)
clipkeeper config set monitoring.pollInterval 500

# Enable/disable privacy filtering
clipkeeper config set privacy.enabled true
```

### Configuration Keys

| Key | Description | Default |
|-----|-------------|---------|
| `retention.days` | Days to keep history (0 = unlimited) | 30 |
| `monitoring.pollInterval` | Clipboard check interval (ms) | 500 |
| `privacy.enabled` | Enable privacy filtering | true |

Retention cleanup runs automatically every hour when the service is running.

## Privacy & Security

clipkeeper is designed with privacy as a priority:

- **All data stays local** - Nothing is sent to external services
- **Automatic filtering** - Sensitive content is blocked by default:
  - Passwords (8+ chars with mixed case, numbers, symbols)
  - Credit card numbers (validated with Luhn algorithm)
  - API keys (Bearer tokens, sk-* keys)
  - Private keys (PEM format)
  - SSH keys (RSA, Ed25519)
- **Custom patterns** - Add your own regex patterns for privacy filtering
- **Secure storage** - Config files have restricted permissions (700)
- **Configurable** - Customize privacy settings and add custom patterns

### Data Storage Locations

**Clipboard History:**
- Windows: `%LOCALAPPDATA%\clipkeeper\clipkeeper.db`
- macOS: `~/Library/Application Support/clipkeeper/clipkeeper.db`
- Linux: `~/.local/share/clipkeeper/clipkeeper.db`

**Logs:**
- Windows: `%APPDATA%\clipkeeper\clipkeeper.log`
- macOS: `~/Library/Application Support/clipkeeper/clipkeeper.log`
- Linux: `~/.local/share/clipkeeper/clipkeeper.log`

**Metrics (when monitoring is enabled):**
- Windows: `%LOCALAPPDATA%\clipkeeper\metrics.log`
- macOS: `~/Library/Application Support/clipkeeper/metrics.log`
- Linux: `~/.local/share/clipkeeper/metrics.log`

## Examples

### Basic Usage

```bash
# Start monitoring
clipkeeper start

# Start in foreground with resource monitoring output
clipkeeper start --monitor

# Copy some text, code, URLs...
# (clipkeeper captures everything automatically)

# View your history
clipkeeper list

# View resource usage metrics
clipkeeper metrics
clipkeeper metrics --limit 20

# Search for specific content
clipkeeper search "error"
clipkeeper search "function" --content-type code

# Copy a previous entry back to clipboard
clipkeeper list  # Note the entry ID
clipkeeper copy abc123

# View only URLs you've copied
clipkeeper list --content-type url --limit 20

# View entries from the last week
clipkeeper list --since "7 days ago"

# View entries from the last 10 days
clipkeeper list --since "10 days ago"

# Export history as JSON
clipkeeper list --format json > history.json

# Clear old entries
clipkeeper clear

# Stop when done
clipkeeper stop
```

### Search Examples

```bash
# Simple text search
clipkeeper search "password reset"

# Search with type filter
clipkeeper search "import" --content-type code

# Search with date filter
clipkeeper search "meeting" --since yesterday

# Search with result limit
clipkeeper search "http" --limit 5
```

### Configuration Examples

```bash
# Keep history for 90 days
clipkeeper config set retention.days 90

# Check current retention setting
clipkeeper config get retention.days

# View all settings
clipkeeper config show
```

## Resource Monitoring

When you start clipkeeper with the `--monitor` flag, it runs in the foreground and tracks resource usage metrics.

### Viewing Metrics

```bash
# View last 10 samples (default)
clipkeeper metrics

# View last 50 samples
clipkeeper metrics --limit 50

# Clear metrics history
clipkeeper metrics --clear
```

### Understanding the Metrics

**Memory Metrics:**
- **RSS (Resident Set Size)**: Total physical RAM used by clipkeeper

**CPU Metrics:**
- **CPU Usage %**: Percentage of CPU time used by the process

**Database Metrics:**
- **File size**: Size of the SQLite database file in MB
- **Total entries**: Number of clipboard entries stored
- **Entries by type**: Breakdown by content type (text, code, url, etc.)

**System Metrics:**
- **Platform/Architecture**: Your operating system and CPU type
- **Total/Free Memory**: System RAM

### What's Normal?

For typical usage:
- **Memory (RSS)**: 5-15 MB (significantly lower than the Node.js version)
- **CPU Usage**: Should be < 1% most of the time (spikes briefly when clipboard changes)
- **Database Size**: Grows over time; depends on retention settings and clipboard activity

## Troubleshooting

### Service won't start

```bash
# Check if already running
clipkeeper status

# Check logs
# Linux: ~/.local/share/clipkeeper/clipkeeper.log
# macOS: ~/Library/Application Support/clipkeeper/clipkeeper.log
```

### No entries showing up

1. Make sure the service is running: `clipkeeper status`
2. Copy something to your clipboard
3. Wait a moment (default poll interval is 500ms)
4. Try `clipkeeper list` again

### Permission errors

- **Linux/macOS:** Check file permissions in data directory
- **Windows:** May need to run as Administrator

### Some clipboard content not captured

- Browser clipboard operations may temporarily lock the clipboard
- The service retries once after a short delay
- Most content will be captured on the next poll

## Development

```bash
# Clone the repository
git clone https://github.com/mattpassman/clipkeeper-rust.git
cd clipkeeper-rust

# Build
cargo build

# Run tests
cargo test

# Run a specific test
cargo test test_name

# Build release
cargo build --release

# Test CLI locally
cargo run -- start
```

## Architecture

clipkeeper consists of several key components:

- **ClipboardMonitor** - Polls system clipboard for changes with retry logic
- **HistoryStore** - SQLite database (with FTS5) for clipboard entries with metadata
- **PrivacyFilter** - Detects and filters sensitive content patterns (with custom regex support)
- **ContentClassifier** - Identifies content types and programming languages using heuristics
- **ConfigurationManager** - Manages settings with validation
- **ServiceManager** - Background service lifecycle management (daemonized on Unix)
- **ResourceMonitor** - Tracks memory, CPU, and database metrics over time
- **SearchService** - Full-text search with FTS5 and LIKE fallback
- **RetentionService** - Automatic cleanup of old entries
- **CLI** - Command-line interface using clap

### Implementation Details

- **std::thread + mpsc channels** - No async runtime, simple thread-based concurrency
- **SQLite with FTS5** - Full-text search with graceful LIKE fallback
- **Arc<Mutex<>>** - Thread-safe shared state
- **Structured logging** - JSON log output via tracing

## Differences from the Node.js Version

| Aspect | Node.js | Rust |
|--------|---------|------|
| Memory usage | ~30-60 MB | ~5-15 MB |
| Binary | Requires Node.js runtime | Single static binary |
| Startup time | ~200ms | ~5ms |
| Dependencies | npm packages | Compiled in |
| Daemonization | Process fork | Unix daemonize crate |

## Roadmap

### v0.3.0 (Current Release)
- Full feature parity with Node.js clipkeeper v0.3.1
- Resource monitoring with metrics tracking
- Content classification with language detection
- Privacy filtering with custom patterns
- Full-text search with FTS5
- Interactive CLI with arrow-key selection
- Multiple output formats (table, JSON, CSV)

### v0.4.0 (Planned)
- Semantic search with natural language queries
- LLM embedding integration (OpenAI, Anthropic, Ollama)
- Vector similarity search
- Usage statistics and analytics

### v0.5.0 (Planned)
- Sync across devices (optional)
- GUI application
- Plugin system
- Mobile companion app

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

MIT License - see [LICENSE](LICENSE) file for details

## Acknowledgments

- Built with [arboard](https://github.com/1Password/arboard) for cross-platform clipboard access
- Uses [rusqlite](https://github.com/rusqlite/rusqlite) for SQLite storage (bundled)
- CLI powered by [clap](https://github.com/clap-rs/clap)
- Structured logging via [tracing](https://github.com/tokio-rs/tracing)

## Support

- [Documentation](https://github.com/mattpassman/clipkeeper-rust)
- [Issue Tracker](https://github.com/mattpassman/clipkeeper-rust/issues)
- [Discussions](https://github.com/mattpassman/clipkeeper-rust/discussions)

---

Made with love by the clipkeeper team
