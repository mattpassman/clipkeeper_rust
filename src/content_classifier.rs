use regex::Regex;
use std::collections::HashMap;

use std::fmt;

/// The fixed set of content types that the classifier can produce.
/// Stored on the stack — no heap allocation needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Url,
    Json,
    Xml,
    Code,
    Markdown,
    FilePath,
    Image,
}

impl ContentType {
    /// Parse from a database / user-supplied string.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "url" => Self::Url,
            "json" => Self::Json,
            "xml" => Self::Xml,
            "code" => Self::Code,
            "markdown" => Self::Markdown,
            "file_path" => Self::FilePath,
            "image" => Self::Image,
            _ => Self::Text,
        }
    }

    /// The canonical string representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Url => "url",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Code => "code",
            Self::Markdown => "markdown",
            Self::FilePath => "file_path",
            Self::Image => "image",
        }
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl rusqlite::types::ToSql for ContentType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(
            rusqlite::types::ValueRef::Text(self.as_str().as_bytes()),
        ))
    }
}

impl rusqlite::types::FromSql for ContentType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_str().map(ContentType::from_str_lossy)
    }
}

/// Structured classification result matching JS feature parity.
///
/// Contains the detected content type, optional programming language,
/// confidence score, and metadata (character/word counts).
pub struct Classification {
    pub content_type: ContentType,
    pub language: Option<String>,
    pub confidence: f64,
    pub metadata: ClassificationMetadata,
}

pub struct ClassificationMetadata {
    pub character_count: usize,
    pub word_count: usize,
}

/// Classifies clipboard content into types: url, json, xml, code, markdown, file_path, image, text.
///
/// Supports language detection for code content and returns structured classification
/// with confidence scores and metadata.
pub struct ContentClassifier {
    url_pattern: Regex,
    markdown_patterns: Vec<Regex>,
    language_keywords: HashMap<&'static str, Vec<&'static str>>,
}

impl ContentClassifier {
    pub fn new() -> Self {
        let mut language_keywords = HashMap::new();
        language_keywords.insert("javascript", vec!["function", "const", "let", "var", "class", "import", "export", "async", "await", "=>"]);
        language_keywords.insert("typescript", vec!["interface", "type", "enum", "namespace", "implements", "extends"]);
        language_keywords.insert("python", vec!["def", "class", "import", "from", "lambda", "yield", "async", "await", "__init__"]);
        language_keywords.insert("java", vec!["public", "private", "protected", "class", "interface", "extends", "implements", "package"]);
        language_keywords.insert("cpp", vec!["#include", "namespace", "class", "template", "typename", "std::"]);
        language_keywords.insert("go", vec!["package", "import", "func", "type", "struct", "interface", "defer", "go"]);
        language_keywords.insert("rust", vec!["fn", "let", "mut", "impl", "trait", "struct", "enum", "use", "mod"]);
        language_keywords.insert("ruby", vec!["def", "class", "module", "require", "end", "do", "yield"]);
        language_keywords.insert("php", vec!["<?php", "function", "class", "namespace", "use", "public", "private", "protected"]);
        language_keywords.insert("sql", vec!["SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "CREATE", "TABLE"]);

        Self {
            url_pattern: Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap(),
            markdown_patterns: vec![
                Regex::new(r"(?m)^#{1,6}\s+.+").unwrap(),
                Regex::new(r"\*\*[^*]+\*\*").unwrap(),
                Regex::new(r"\*[^*]+\*").unwrap(),
                Regex::new(r"\[.+?\]\(.+?\)").unwrap(),
                Regex::new(r"(?m)^[-*+]\s+.+").unwrap(),
                Regex::new(r"(?m)^\d+\.\s+.+").unwrap(),
                Regex::new(r"(?ms)^```[\s\S]*?```$").unwrap(),
                Regex::new(r"`[^`]+`").unwrap(),
            ],
            language_keywords,
        }
    }

    /// Classify content and return a ContentType enum (backward compatible)
    pub fn classify(&self, content: &str) -> ContentType {
        self.classify_detailed(content).content_type
    }

    /// Classify content and return structured Classification (Task 8.3)
    pub fn classify_detailed(&self, content: &str) -> Classification {
        let metadata = ClassificationMetadata {
            character_count: content.len(),
            word_count: content.split_whitespace().count(),
        };

        if content.is_empty() {
            return Classification {
                content_type: ContentType::Text,
                language: None,
                confidence: 1.0,
                metadata,
            };
        }

        // Task 8.1: Image detection (for future image support)
        if content.starts_with("\u{89}PNG") || content.starts_with("GIF8")
            || (content.len() >= 2 && content.as_bytes()[0] == 0xFF && content.as_bytes()[1] == 0xD8)
        {
            log_classification("image", content.len());
            return Classification {
                content_type: ContentType::Image,
                language: None,
                confidence: 1.0,
                metadata,
            };
        }

        // Check URL
        if self.url_pattern.is_match(content.trim()) {
            log_classification("url", content.len());
            return Classification {
                content_type: ContentType::Url,
                language: None,
                confidence: 0.95,
                metadata,
            };
        }

        // Check JSON
        if (content.trim().starts_with('{') && content.trim().ends_with('}'))
            || (content.trim().starts_with('[') && content.trim().ends_with(']'))
        {
            if serde_json::from_str::<serde_json::Value>(content).is_ok() {
                log_classification("json", content.len());
                return Classification {
                    content_type: ContentType::Json,
                    language: None,
                    confidence: 0.95,
                    metadata,
                };
            }
        }

        // Check XML (Task 8.4: improved validation with matching tags)
        if content.trim().starts_with('<') {
            let trimmed = content.trim();
            let has_xml_decl = trimmed.starts_with("<?xml");
            let has_root_element = Regex::new(r"<([a-zA-Z][^>]*)>[\s\S]*</([a-zA-Z][^>]*)>")
                .unwrap()
                .is_match(trimmed);

            if has_xml_decl || has_root_element {
                log_classification("xml", content.len());
                return Classification {
                    content_type: ContentType::Xml,
                    language: None,
                    confidence: 0.9,
                    metadata,
                };
            }
        }

        // Check file path
        if is_file_path(content) {
            log_classification("file_path", content.len());
            return Classification {
                content_type: ContentType::FilePath,
                language: None,
                confidence: 0.9,
                metadata,
            };
        }

        // Check markdown
        for pattern in &self.markdown_patterns {
            if pattern.is_match(content) {
                log_classification("markdown", content.len());
                return Classification {
                    content_type: ContentType::Markdown,
                    language: None,
                    confidence: 0.8,
                    metadata,
                };
            }
        }

        // Check code with language detection (Task 8.2)
        if let Some((language, confidence)) = self.detect_code(content) {
            log_classification("code", content.len());
            return Classification {
                content_type: ContentType::Code,
                language: Some(language),
                confidence,
                metadata,
            };
        }

        log_classification("text", content.len());
        Classification {
            content_type: ContentType::Text,
            language: None,
            confidence: 1.0,
            metadata,
        }
    }

    /// Detect code and identify programming language (Task 8.2)
    fn detect_code(&self, content: &str) -> Option<(String, f64)> {
        let trimmed = content.trim();

        let has_code_chars = Regex::new(r"[{}\[\]();]").unwrap().is_match(trimmed);
        let has_indentation = Regex::new(r"(?m)^[ \t]+").unwrap().is_match(trimmed);
        let has_operators = Regex::new(r"[=<>!+\-*/%&|^~]").unwrap().is_match(trimmed);

        let mut code_score: f64 = 0.0;
        if has_code_chars { code_score += 0.3; }
        if has_indentation { code_score += 0.2; }
        if has_operators { code_score += 0.2; }

        // Score each language by keyword matches
        let mut best_language: Option<String> = None;
        let mut max_score: f64 = 0.0;

        for (&language, keywords) in &self.language_keywords {
            let mut score: f64 = 0.0;
            for &keyword in keywords {
                let is_special = keyword.contains('<') || keyword.contains("::") || keyword.contains("<?");
                let matched = if is_special {
                    trimmed.contains(keyword)
                } else {
                    let pattern = format!(r"(?i)\b{}\b", regex::escape(keyword));
                    Regex::new(&pattern).map(|r| r.is_match(trimmed)).unwrap_or(false)
                };
                if matched {
                    let weight = if keyword.len() > 5 { 1.5 } else { 1.0 };
                    score += weight;
                }
            }
            if score > max_score {
                max_score = score;
                best_language = Some(language.to_string());
            }
        }

        if max_score > 0.0 {
            code_score += (max_score * 0.1).min(0.5);
        }

        let is_long = trimmed.len() > 200;
        let min_code_score = if is_long { 0.7 } else { 0.5 };
        let min_keyword_score = if is_long { 3.0 } else { 2.0 };

        if code_score >= min_code_score || max_score >= min_keyword_score {
            return Some((
                best_language.unwrap_or_else(|| "unknown".to_string()),
                code_score.min(1.0),
            ));
        }

        None
    }
}

fn log_classification(content_type: &str, content_length: usize) {
    crate::log_component_action!(
        "ContentClassifier",
        "Content classified",
        content_type = content_type,
        content_length = content_length
    );
}

fn is_file_path(content: &str) -> bool {
    let windows_path = Regex::new(r#"^[a-zA-Z]:\\(?:[^\\/:*?"<>|\r\n]+\\)*[^\\/:*?"<>|\r\n]*$"#).unwrap();
    let unix_path = Regex::new(r"^/(?:[^/]+/?)*$|^~(?:/[^/]+)*/?$").unwrap();
    windows_path.is_match(content) || unix_path.is_match(content)
}

impl Default for ContentClassifier {
    fn default() -> Self {
        Self::new()
    }
}
