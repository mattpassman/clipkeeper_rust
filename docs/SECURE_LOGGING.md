# Secure Logging Implementation

## Overview

ClipKeeper implements secure logging to ensure that sensitive content (passwords, API keys, credit cards, etc.) is **never** written to log files. This document describes the secure logging implementation and how to use it correctly.

## Requirements

**Validates: Requirements 2.7, 9.4**

- **Requirement 2.7**: When the Privacy_Filter blocks content, THE ClipKeeper_System SHALL log the filtering action without logging the actual sensitive content
- **Requirement 9.4**: When sensitive content is filtered, THE Logger SHALL log the filtering action without logging the actual sensitive content

## Implementation

### Macros

The secure logging system provides three macros:

#### 1. `log_secure_action!` - For Privacy Filter Actions

Use this macro when logging privacy filter actions or any operation involving sensitive data.

```rust
log_secure_action!(
    "Privacy filter blocked content",
    pattern_type = "password",
    content_length = 42
);
```

**What gets logged:**
- ✓ Action description
- ✓ Pattern type that triggered the filter
- ✓ Content length
- ✓ Reason for filtering
- ✗ **NEVER** the actual sensitive content

#### 2. `log_component_action!` - For Component Actions

Use this macro for general component actions with proper component tagging.

```rust
log_component_action!(
    "ConfigurationManager",
    "Configuration loaded",
    config_path = %config_path.display()
);
```

#### 3. `log_component_error!` - For Component Errors

Use this macro for logging errors from any component.

```rust
log_component_error!(
    "HistoryStore",
    "Failed to save entry",
    error = %err
);
```

### Log Format

All logs are written in JSON format for structured logging:

```json
{
  "timestamp": "2024-01-15T10:30:45.123Z",
  "level": "INFO",
  "fields": {
    "message": "Content filtered by privacy filter",
    "component": "PrivacyFilter",
    "action": "filtered",
    "pattern_type": "password",
    "content_length": 14
  },
  "target": "clipkeeper::privacy_filter"
}
```

**Notice:** The actual password content is NOT in the log!

## Usage Examples

### Example 1: Privacy Filter

```rust
// ❌ WRONG - Logs sensitive content
tracing::info!(
    component = "PrivacyFilter",
    "Filtered password: {}",
    sensitive_password  // DON'T DO THIS!
);

// ✓ CORRECT - Logs metadata only
log_secure_action!(
    "Content filtered by privacy filter",
    pattern_type = "password",
    content_length = sensitive_password.len()
);
```

### Example 2: Configuration Manager

```rust
// ✓ CORRECT - Logs configuration action
log_component_action!(
    "ConfigurationManager",
    "Configuration loaded",
    config_path = %config_path.display()
);
```

### Example 3: History Store

```rust
// ✓ CORRECT - Logs database operation without content
log_component_action!(
    "HistoryStore",
    "Entry saved",
    entry_id = %entry_id,
    content_type = %content_type
);
```

## Testing

### Unit Tests

The logger module includes comprehensive unit tests:

```bash
cargo test --lib logger
```

### Integration Tests

Integration tests verify that:
1. Logs are written to files correctly
2. Sensitive content is NOT in log files
3. Component tags are present

```bash
cargo test --test secure_logging_integration
cargo test --test secure_logging_file_test
```

### Verification

To verify secure logging in production:

1. Enable privacy filtering
2. Copy sensitive content (password, API key, etc.)
3. Check log files in platform-specific directories:
   - **Windows**: `%LOCALAPPDATA%\clipkeeper\logs`
   - **macOS**: `~/Library/Logs/clipkeeper`
   - **Linux**: `~/.local/share/clipkeeper/logs`
4. Verify logs contain metadata but NOT actual sensitive content

## Log Locations

Logs are stored in platform-specific directories:

| Platform | Log Directory |
|----------|--------------|
| Windows  | `%LOCALAPPDATA%\clipkeeper\logs` |
| macOS    | `~/Library/Logs/clipkeeper` |
| Linux    | `~/.local/share/clipkeeper/logs` |

## Log Rotation

- **Rotation**: Daily
- **Retention**: 7 days
- **Format**: `clipkeeper.log.YYYY-MM-DD`

## Security Considerations

### What to Log

✓ **Safe to log:**
- Pattern types (e.g., "password", "credit_card")
- Content lengths
- Timestamps
- Entry IDs
- Content types
- Error messages (without sensitive data)

✗ **NEVER log:**
- Actual passwords
- Credit card numbers
- API keys
- Bearer tokens
- SSH keys
- Private keys
- Any content that matches privacy patterns

### Best Practices

1. **Always use `log_secure_action!`** when dealing with privacy filter operations
2. **Never log clipboard content directly** - use content_length instead
3. **Use component tags** for all log entries
4. **Test your logging** - verify sensitive content is not in logs
5. **Review logs regularly** - ensure no sensitive data leaks

## Component Tags

All log entries include component tags for filtering and analysis:

- `Application` - Main application orchestrator
- `ClipboardMonitor` - Clipboard monitoring service
- `PrivacyFilter` - Privacy filtering operations
- `ContentClassifier` - Content type classification
- `HistoryStore` - Database operations
- `ConfigurationManager` - Configuration management
- `Logger` - Logging system initialization

## Troubleshooting

### Issue: Logs not being written

**Solution:** Check that:
1. Log directory exists and has proper permissions (0700 on Unix)
2. Logger is initialized before logging
3. Disk space is available

### Issue: Sensitive content in logs

**Solution:** This is a **CRITICAL SECURITY ISSUE**. If you find sensitive content in logs:
1. Immediately stop the application
2. Delete the log files containing sensitive data
3. Review the code to find where sensitive content was logged
4. Use `log_secure_action!` instead of direct logging
5. Add tests to verify the fix

## References

- **Requirements**: 2.7, 9.4
- **Design**: Logger module specification
- **Implementation**: `src/logger.rs`, `src/privacy_filter.rs`
- **Tests**: `tests/secure_logging_integration.rs`, `tests/secure_logging_file_test.rs`
