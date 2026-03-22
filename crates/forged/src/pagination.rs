use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};

/// Parameters extracted from a paginated gRPC request.
#[derive(Debug, Clone)]
pub struct PaginationParams {
    pub page_size: u32,
    pub page_token: Option<String>,
}

/// A page of results with an optional continuation token.
#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub next_page_token: Option<String>,
}

/// Clamp a requested page size to a valid range.
///
/// - If `requested` is <= 0, returns `default`.
/// - If `requested` exceeds `max`, returns `max`.
/// - Otherwise returns `requested` as `u32`.
pub fn resolve_page_size(requested: i32, default: u32, max: u32) -> u32 {
    if requested <= 0 {
        default
    } else if requested as u32 > max {
        max
    } else {
        requested as u32
    }
}

/// Encode a `DateTime<Utc>` as a URL-safe base64 cursor string.
pub fn encode_cursor(timestamp: &DateTime<Utc>) -> String {
    let rfc3339 = timestamp.to_rfc3339();
    URL_SAFE_NO_PAD.encode(rfc3339.as_bytes())
}

/// Decode a cursor string back into a `DateTime<Utc>`.
///
/// Returns `None` if the token is empty or cannot be decoded.
pub fn decode_cursor(token: &str) -> Option<DateTime<Utc>> {
    if token.is_empty() {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    let s = std::str::from_utf8(&bytes).ok()?;
    s.parse::<DateTime<Utc>>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_page_size_default() {
        assert_eq!(resolve_page_size(0, 50, 1000), 50);
        assert_eq!(resolve_page_size(-5, 50, 1000), 50);
    }

    #[test]
    fn test_resolve_page_size_clamped() {
        assert_eq!(resolve_page_size(2000, 50, 1000), 1000);
    }

    #[test]
    fn test_resolve_page_size_normal() {
        assert_eq!(resolve_page_size(25, 50, 1000), 25);
    }

    #[test]
    fn test_cursor_roundtrip() {
        let now = Utc::now();
        let encoded = encode_cursor(&now);
        let decoded = decode_cursor(&encoded).expect("should decode");
        assert_eq!(now.to_rfc3339(), decoded.to_rfc3339());
    }

    #[test]
    fn test_decode_empty_cursor() {
        assert!(decode_cursor("").is_none());
    }

    #[test]
    fn test_decode_invalid_cursor() {
        assert!(decode_cursor("not-valid-base64!!!").is_none());
    }
}
