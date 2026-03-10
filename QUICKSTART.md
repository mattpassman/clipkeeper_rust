# ClipKeeper Quick Start

## Install

```bash
# Build
cargo build --release

# Copy to PATH
cp target/release/clipkeeper ~/.local/bin/
```

## Basic Usage

```bash
# Start monitoring
clipkeeper start

# View recent clips
clipkeeper list

# Search
clipkeeper search "hello world"

# Copy entry back to clipboard
clipkeeper copy <id>

# Stop
clipkeeper stop
```

## Common Tasks

```bash
# List as JSON
clipkeeper list --format json

# Search with type filter
clipkeeper search "function" --content-type code

# Run in foreground with monitoring
clipkeeper start --monitor

# View metrics
clipkeeper metrics

# Clear history (with confirmation)
clipkeeper clear

# Update config
clipkeeper config set privacy.enabled true
clipkeeper config set retention.days 7
```
