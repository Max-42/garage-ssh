/// Sanitize user-provided strings to prevent XSS and injection attacks
pub fn sanitize_string(input: &str) -> String {
    // Use ammonia to strip all HTML tags and dangerous content
    let cleaned = ammonia::clean(input);

    // Additional: limit length to prevent storage abuse
    let max_len = 1024;
    if cleaned.len() > max_len {
        cleaned[..max_len].to_string()
    } else {
        cleaned
    }
}

/// Sanitize a short field (username, device name, etc.)
pub fn sanitize_short_string(input: &str) -> String {
    let cleaned = ammonia::clean(input);
    let max_len = 256;
    if cleaned.len() > max_len {
        cleaned[..max_len].to_string()
    } else {
        cleaned
    }
}

/// Validate and sanitize JSON payload from client
pub fn sanitize_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(sanitize_string(s)),
        serde_json::Value::Object(map) => {
            let mut sanitized = serde_json::Map::new();
            // Limit to 50 keys to prevent abuse
            for (i, (key, val)) in map.iter().enumerate() {
                if i >= 50 {
                    break;
                }
                let clean_key = sanitize_short_string(key);
                sanitized.insert(clean_key, sanitize_json_value(val));
            }
            serde_json::Value::Object(sanitized)
        }
        serde_json::Value::Array(arr) => {
            // Limit array size
            let sanitized: Vec<_> = arr.iter().take(100).map(sanitize_json_value).collect();
            serde_json::Value::Array(sanitized)
        }
        // Numbers, booleans, null are safe
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_xss() {
        assert_eq!(
            sanitize_string("<script>alert('xss')</script>Hello"),
            "Hello"
        );
    }

    #[test]
    fn test_sanitize_normal_string() {
        assert_eq!(sanitize_string("Max's iPad"), "Max's iPad");
    }

    #[test]
    fn test_sanitize_long_string() {
        let long = "a".repeat(2000);
        assert_eq!(sanitize_string(&long).len(), 1024);
    }
}
