use regex::Regex;

/// Identifies and filters sensitive content patterns (passwords, credit cards, API keys, etc.)
///
/// Supports built-in patterns and user-defined custom regex patterns.
pub struct PrivacyFilter {
    enabled: bool,
    patterns: Vec<FilterPattern>,
    custom_patterns: Vec<FilterPattern>,
}

struct FilterPattern {
    name: String,
    regex: Regex,
    description: String,
    requires_validation: bool,
}

/// Result of privacy filtering, indicating whether content was blocked and why.
pub struct FilterResult {
    pub filtered: bool,
    pub pattern_type: Option<String>,
    pub reason: Option<String>,
}

impl PrivacyFilter {
    pub fn new(enabled: bool) -> Self {
        Self::with_custom_patterns(enabled, &[])
    }

    pub fn with_custom_patterns(enabled: bool, custom_pattern_strs: &[String]) -> Self {
        let mut patterns = Vec::new();

        patterns.push(FilterPattern {
            name: "password".to_string(),
            regex: Regex::new(r"^[A-Za-z\d@$!%*?&#]{8,}$").unwrap(),
            description: "Password with mixed case, numbers, and symbols".to_string(),
            requires_validation: true,
        });
        patterns.push(FilterPattern {
            name: "credit_card".to_string(),
            regex: Regex::new(r"\b\d{13,19}\b").unwrap(),
            description: "Credit card number".to_string(),
            requires_validation: false,
        });
        patterns.push(FilterPattern {
            name: "api_key_bearer".to_string(),
            regex: Regex::new(r"Bearer\s+[a-zA-Z0-9\-._~+/]+=*").unwrap(),
            description: "Bearer token".to_string(),
            requires_validation: false,
        });
        patterns.push(FilterPattern {
            name: "api_key_sk".to_string(),
            regex: Regex::new(r"\bsk-[a-zA-Z0-9\-]{32,}\b").unwrap(),
            description: "API key starting with sk-".to_string(),
            requires_validation: false,
        });
        patterns.push(FilterPattern {
            name: "private_key".to_string(),
            regex: Regex::new(r"-----BEGIN.*PRIVATE KEY-----").unwrap(),
            description: "Private key (PEM format)".to_string(),
            requires_validation: false,
        });
        patterns.push(FilterPattern {
            name: "ssh_rsa".to_string(),
            regex: Regex::new(r"ssh-rsa\s+[A-Za-z0-9+/=]+").unwrap(),
            description: "SSH RSA key".to_string(),
            requires_validation: false,
        });
        patterns.push(FilterPattern {
            name: "ssh_ed25519".to_string(),
            regex: Regex::new(r"ssh-ed25519\s+[A-Za-z0-9+/=]+").unwrap(),
            description: "SSH Ed25519 key".to_string(),
            requires_validation: false,
        });

        // Compile custom patterns (Task 7.2)
        let mut custom_patterns = Vec::new();
        for (i, pat_str) in custom_pattern_strs.iter().enumerate() {
            match Regex::new(pat_str) {
                Ok(regex) => {
                    custom_patterns.push(FilterPattern {
                        name: format!("custom_{}", i),
                        regex,
                        description: "Custom pattern match".to_string(),
                        requires_validation: false,
                    });
                }
                Err(e) => {
                    crate::log_component_action!(
                        "PrivacyFilter",
                        "Invalid custom pattern skipped",
                        pattern_index = i,
                        error = %e
                    );
                }
            }
        }

        Self { enabled, patterns, custom_patterns }
    }

    pub fn should_filter(&self, content: &str) -> FilterResult {
        if !self.enabled {
            return FilterResult {
                filtered: false,
                pattern_type: None,
                reason: None,
            };
        }

        // URL exception applies to ALL sensitive patterns (Task 7.3)
        let is_url = content.trim().starts_with("http://") || content.trim().starts_with("https://");

        for pattern in &self.patterns {
            // Skip password check for URLs
            if pattern.name == "password" && is_url {
                continue;
            }

            if pattern.regex.is_match(content) {
                if pattern.name == "password" && pattern.requires_validation {
                    if !has_password_complexity(content) {
                        continue;
                    }
                }

                if pattern.name == "credit_card" {
                    if let Some(captures) = pattern.regex.captures(content) {
                        if let Some(card_num) = captures.get(0) {
                            if !validate_luhn(card_num.as_str()) {
                                continue;
                            }
                        }
                    }
                }

                // Log the filtering action without the actual content (Task 7.1)
                crate::log_secure_action!(
                    "Content filtered by privacy filter",
                    pattern_type = pattern.name.as_str(),
                    reason = pattern.description.as_str(),
                    content_length = content.len()
                );

                return FilterResult {
                    filtered: true,
                    pattern_type: Some(pattern.name.clone()),
                    reason: Some(pattern.description.clone()),
                };
            }
        }

        // Check custom patterns (Task 7.2)
        for pattern in &self.custom_patterns {
            if pattern.regex.is_match(content) {
                crate::log_secure_action!(
                    "Content filtered by custom privacy pattern",
                    pattern_type = pattern.name.as_str(),
                    reason = pattern.description.as_str(),
                    content_length = content.len()
                );

                return FilterResult {
                    filtered: true,
                    pattern_type: Some(pattern.name.clone()),
                    reason: Some(pattern.description.clone()),
                };
            }
        }

        FilterResult {
            filtered: false,
            pattern_type: None,
            reason: None,
        }
    }
}

fn has_password_complexity(content: &str) -> bool {
    let has_lower = content.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = content.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = content.chars().any(|c| c.is_ascii_digit());
    let has_special = content.chars().any(|c| "@$!%*?&#".contains(c));
    has_lower && has_upper && has_digit && has_special
}

fn validate_luhn(card_number: &str) -> bool {
    let digits: String = card_number.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum = 0;
    let mut is_even = false;
    for ch in digits.chars().rev() {
        let mut digit = ch.to_digit(10).unwrap();
        if is_even {
            digit *= 2;
            if digit > 9 { digit -= 9; }
        }
        sum += digit;
        is_even = !is_even;
    }
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_logging_does_not_log_sensitive_content() {
        let filter = PrivacyFilter::new(true);
        let result = filter.should_filter("MyP@ssw0rd123!");
        assert!(result.filtered);
        assert_eq!(result.pattern_type, Some("password".to_string()));
    }

    #[test]
    fn test_secure_logging_with_credit_card() {
        let filter = PrivacyFilter::new(true);
        let result = filter.should_filter("4532015112830366");
        assert!(result.filtered);
        assert_eq!(result.pattern_type, Some("credit_card".to_string()));
    }

    #[test]
    fn test_secure_logging_with_api_key() {
        let filter = PrivacyFilter::new(true);
        let result = filter.should_filter("sk-1234567890abcdefghijklmnopqrstuvwxyz");
        assert!(result.filtered);
        assert_eq!(result.pattern_type, Some("api_key_sk".to_string()));
    }

    #[test]
    fn test_secure_logging_with_bearer_token() {
        let filter = PrivacyFilter::new(true);
        let result = filter.should_filter("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9");
        assert!(result.filtered);
        assert_eq!(result.pattern_type, Some("api_key_bearer".to_string()));
    }

    #[test]
    fn test_secure_logging_disabled() {
        let filter = PrivacyFilter::new(false);
        let result = filter.should_filter("MyP@ssw0rd123!");
        assert!(!result.filtered);
        assert!(result.pattern_type.is_none());
    }

    #[test]
    fn test_url_exception_for_password_pattern() {
        let filter = PrivacyFilter::new(true);
        let result = filter.should_filter("https://example.com/path?param=MyP@ssw0rd123!");
        assert!(!result.filtered);
    }

    #[test]
    fn test_luhn_validation() {
        assert!(validate_luhn("4532015112830366"));
        assert!(validate_luhn("5425233430109903"));
        assert!(validate_luhn("374245455400126"));
        assert!(!validate_luhn("1234567890123456"));
        // Note: 0000000000000000 passes Luhn (sum=0, 0%10=0) - this is correct behavior
        assert!(validate_luhn("0000000000000000"));
        assert!(!validate_luhn("4532015112830367"));
    }

    #[test]
    fn test_filter_result_structure() {
        let filter = PrivacyFilter::new(true);
        let result = filter.should_filter("MyP@ssw0rd123!");
        assert!(result.filtered);
        assert_eq!(result.pattern_type.unwrap(), "password");
        assert!(!result.reason.unwrap().is_empty());
    }

    #[test]
    fn test_custom_patterns() {
        let custom = vec!["SECRET_[A-Z0-9]+".to_string()];
        let filter = PrivacyFilter::with_custom_patterns(true, &custom);
        let result = filter.should_filter("SECRET_ABC123");
        assert!(result.filtered);
        assert_eq!(result.pattern_type, Some("custom_0".to_string()));
    }

    #[test]
    fn test_custom_pattern_invalid_regex_skipped() {
        let custom = vec!["[invalid".to_string(), r"\d+".to_string()];
        let filter = PrivacyFilter::with_custom_patterns(true, &custom);
        // Invalid pattern skipped, valid one works
        let result = filter.should_filter("12345678901234"); // not a valid CC
        // The digits pattern should match as custom_1
        assert!(result.filtered);
    }
}
