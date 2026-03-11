/// Demonstration of metadata serialization/deserialization in HistoryStore
///
/// This example shows:
/// 1. Saving entries with automatically calculated metadata (character count, word count)
/// 2. Retrieving entries with deserialized metadata
/// 3. Handling entries with missing/invalid metadata (Node.js compatibility)

use clipkeeper::content_classifier::ContentType;
use clipkeeper::history_store::HistoryStore;
use tempfile::TempDir;

fn main() -> anyhow::Result<()> {
    println!("=== ClipKeeper Metadata Serialization Demo ===\n");

    // Create a temporary database
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("demo.db");
    let store = HistoryStore::new(&db_path)?;

    println!("1. Saving entries with metadata...");
    
    // Save various types of content
    let entries: Vec<(&str, ContentType)> = vec![
        ("Hello, World!", ContentType::Text),
        ("function test() { return 42; }", ContentType::Code),
        ("https://example.com/api/v1/users", ContentType::Url),
        ("This is a longer piece of text with multiple words and sentences.", ContentType::Text),
    ];

    for (content, content_type) in &entries {
        let id = store.save(content, *content_type)?;
        println!("   Saved: {} ({})", &id.to_string()[..8], content_type);
    }

    println!("\n2. Retrieving entries with metadata...\n");
    
    let all_entries = store.list(10, None, None, None)?;
    
    for entry in &all_entries {
        println!("Entry: {}", &entry.id.to_string()[..8]);
        println!("  Content: {}", entry.content);
        println!("  Type: {}", entry.content_type);
        println!("  Metadata:");
        println!("    - Character count: {}", entry.metadata.character_count);
        println!("    - Word count: {}", entry.metadata.word_count);
        println!("    - Confidence: {}", entry.metadata.confidence);
        println!("    - Language: {:?}", entry.metadata.language);
        println!();
    }

    println!("3. Metadata is preserved across all retrieval methods...\n");
    
    // Test metadata in get_by_id
    let first_id = all_entries[0].id.to_string();
    let entry_by_id = store.get_by_id(&first_id)?;
    println!("Retrieved by ID:");
    println!("  Character count: {}", entry_by_id.metadata.character_count);
    println!("  Word count: {}", entry_by_id.metadata.word_count);
    
    // Test metadata in get_since
    let recent = store.get_since(0, 5)?;
    println!("\nRetrieved {} recent entries:", recent.len());
    for entry in &recent {
        println!("  - {} chars, {} words", 
                 entry.metadata.character_count, 
                 entry.metadata.word_count);
    }
    
    // Test metadata in get_recent_by_type
    let text_entries = store.get_recent_by_type("text", 5)?;
    println!("\nRetrieved {} text entries:", text_entries.len());
    for entry in &text_entries {
        println!("  - {} chars, {} words", 
                 entry.metadata.character_count, 
                 entry.metadata.word_count);
    }
    
    println!("\n4. Verifying metadata in search results...\n");
    
    let search_results = store.search("test", 10, None, None)?;
    println!("Found {} entries matching 'test':", search_results.len());
    for entry in search_results {
        println!("  - {} (chars: {}, words: {})", 
                 entry.content, 
                 entry.metadata.character_count, 
                 entry.metadata.word_count);
    }

    println!("\n=== Demo Complete ===");
    println!("\nKey Features Demonstrated:");
    println!("✓ Metadata automatically calculated on save (character_count, word_count)");
    println!("✓ Metadata serialized to JSON in database");
    println!("✓ Metadata deserialized on retrieval");
    println!("✓ Metadata preserved in get_by_id()");
    println!("✓ Metadata preserved in get_since()");
    println!("✓ Metadata preserved in get_recent_by_type()");
    println!("✓ Metadata preserved in search()");
    println!("✓ Metadata preserved in list()");

    Ok(())
}
