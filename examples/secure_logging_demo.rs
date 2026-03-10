/// Demonstration of secure logging functionality
/// 
/// This example shows how the secure logging macros prevent sensitive content
/// from being logged while still capturing important metadata.

use clipkeeper::{log_secure_action, log_component_action, log_component_error};

fn main() {
    // Initialize logging (in a real app, this would be done once at startup)
    println!("=== Secure Logging Demo ===\n");
    
    // Simulate filtering sensitive content
    let sensitive_password = "MyP@ssw0rd123!";
    let content_length = sensitive_password.len();
    
    println!("1. Filtering sensitive content (password):");
    println!("   Actual content: '{}' (length: {})", sensitive_password, content_length);
    println!("   What gets logged:");
    
    // This macro logs the action WITHOUT the actual password
    log_secure_action!(
        "Content filtered by privacy filter",
        pattern_type = "password",
        reason = "Matches password pattern",
        content_length = content_length
    );
    println!("   ✓ Logged metadata only (pattern_type, reason, content_length)\n");
    
    // Simulate filtering a credit card
    let credit_card = "4532-1234-5678-9010";
    println!("2. Filtering sensitive content (credit card):");
    println!("   Actual content: '{}' (length: {})", credit_card, credit_card.len());
    println!("   What gets logged:");
    
    log_secure_action!(
        "Content filtered by privacy filter",
        pattern_type = "credit_card",
        reason = "Valid credit card number detected",
        content_length = credit_card.len()
    );
    println!("   ✓ Logged metadata only (pattern_type, reason, content_length)\n");
    
    // Demonstrate component logging
    println!("3. Component action logging:");
    log_component_action!(
        "ConfigurationManager",
        "Configuration loaded successfully",
        config_path = "/path/to/config.json"
    );
    println!("   ✓ Logged with component tag: ConfigurationManager\n");
    
    // Demonstrate component error logging
    println!("4. Component error logging:");
    log_component_error!(
        "HistoryStore",
        "Failed to save entry",
        error = "Database connection lost"
    );
    println!("   ✓ Logged error with component tag: HistoryStore\n");
    
    println!("=== Key Points ===");
    println!("✓ Sensitive content is NEVER logged");
    println!("✓ Only metadata (pattern type, length, reason) is logged");
    println!("✓ All log entries include component tags for structured logging");
    println!("✓ Logs are written in JSON format for easy parsing");
}
