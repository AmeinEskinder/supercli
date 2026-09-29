//! Path and identifier validation for session-scoped resources.
//!
//! Single strict implementation used by all crates (`supercli-core`,
//! `supercli-serve`, `supercli-client`). Security-critical: a crafted
//! `session_id`, `kind`, or `name` must not escape the session directories.
//!
//! Consolidates the six prior validators (which had inconsistent rules):
//! - `core::controller_host::safe_session_id`
//! - `core::session_artifacts::safe_segment`
//! - `core::remote_session_backend::validate_session_id`
//! - `core::app_presentations::validate_app_presentation_session_id`
//! - `serve::mobile::safe_session_id`
//! - `shared::validation::{required_session_id, safe_artifact_segment}`
//!
//! The strict rule is the union of all prior rejections (strictest wins):
//! - non-empty
//! - at most [`MAX_ID_BYTES`] bytes (128; strictest max among session-ID validators)
//! - no `/` (POSIX path separator)
//! - no `\` (Windows path separator)
//! - no `..` (parent-directory traversal)
//! - no NUL (`\0`)
//! - no control characters (covers `\r`, `\n`, `\t`, etc.)
//! - no leading/trailing whitespace (must already be trimmed)

/// Maximum session ID / path segment length in bytes.
///
/// 128 is the strictest max among the prior session-ID validators
/// (`controller_host` and `remote_session_backend` both used 128;
/// `app_presentations` allowed 256).
pub const MAX_ID_BYTES: usize = 128;

/// Returns `true` if `value` is a safe session ID or path segment.
///
/// This is the single strict predicate all crates use. See the module docs
/// for the exact rejection rules.
pub fn is_safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
        && value.trim() == value
}

/// Returns `true` if `value` is a safe artifact path segment.
///
/// Same strict rules as [`is_safe_id`] except no max-length limit:
/// artifact names have their own bounds (see `safe_upload_filename` in
/// `supercli-core::session_artifacts`, which allows up to 180 bytes).
pub fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
        && value.trim() == value
}

/// Validation failure, mirroring `MobileRemoteError` (HTTP status + message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// HTTP-style status code (400 for invalid input).
    pub code: u16,
    /// Human-readable message.
    pub message: String,
}

impl ValidationError {
    fn bad_request(message: &str) -> Self {
        Self {
            code: 400,
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Validate a session ID from a query parameter.
///
/// Uses the strict [`is_safe_id`] rule. Unlike the original Swift port,
/// this does NOT silently trim whitespace — leading/trailing whitespace
/// is rejected. Returns the ID on success.
pub fn required_session_id(value: Option<&str>) -> Result<String, ValidationError> {
    let id = value.unwrap_or("");
    if !is_safe_id(id) {
        return Err(ValidationError::bad_request("invalid session id"));
    }
    Ok(id.to_string())
}

/// Validate a single artifact path segment (`kind` or `name`).
///
/// Uses the strict [`is_safe_id`] rule. Returns the segment on success.
pub fn safe_artifact_segment(value: Option<&str>) -> Result<String, ValidationError> {
    let segment = value.unwrap_or("");
    if !is_safe_id(segment) {
        return Err(ValidationError::bad_request("invalid artifact path"));
    }
    Ok(segment.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_ids() {
        assert!(is_safe_id("abc123"));
        assert!(is_safe_id("session-uuid-4f8a-9b2c"));
        assert!(is_safe_id("a"));
        assert!(is_safe_id(&"x".repeat(128)));
        assert_eq!(required_session_id(Some("abc123")).unwrap(), "abc123");
        assert_eq!(
            safe_artifact_segment(Some("screenshots")).unwrap(),
            "screenshots"
        );
        assert_eq!(safe_artifact_segment(Some("img.png")).unwrap(), "img.png");
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_safe_id(""));
        assert!(required_session_id(None).is_err());
        assert!(required_session_id(Some("")).is_err());
        assert!(safe_artifact_segment(None).is_err());
        assert!(safe_artifact_segment(Some("")).is_err());
    }

    #[test]
    fn rejects_posix_separator() {
        assert!(!is_safe_id("a/b"));
        assert!(!is_safe_id("/etc/passwd"));
        assert!(!is_safe_id("a/"));
    }

    #[test]
    fn rejects_windows_separator() {
        // Windows-style traversal: backslash is a separator on Windows.
        assert!(!is_safe_id("a\\b"));
        assert!(!is_safe_id("..\\..\\windows"));
        assert!(!is_safe_id("C:\\temp"));
        assert!(!is_safe_id("\\server\\share"));
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(!is_safe_id(".."));
        assert!(!is_safe_id("../etc"));
        assert!(!is_safe_id("a/../b"));
        assert!(!is_safe_id("...")); // contains ".."
                                     // Note: "a..b" contains ".." and is rejected (strictest rule).
        assert!(!is_safe_id("a..b"));
    }

    #[test]
    fn rejects_nul_byte() {
        assert!(!is_safe_id("abc\0"));
        assert!(!is_safe_id("\0"));
        assert!(!is_safe_id("a\0b"));
    }

    #[test]
    fn rejects_control_chars() {
        assert!(!is_safe_id("a\rb"));
        assert!(!is_safe_id("a\nb"));
        assert!(!is_safe_id("a\tb"));
        assert!(!is_safe_id("\x1b[31m")); // ANSI escape
        assert!(!is_safe_id("\x7f")); // DEL
    }

    #[test]
    fn rejects_untrimmed_whitespace() {
        assert!(!is_safe_id("  abc123"));
        assert!(!is_safe_id("abc123  "));
        assert!(!is_safe_id("  abc123  "));
        assert!(!is_safe_id("   "));
        // required_session_id no longer trims; it rejects.
        assert!(required_session_id(Some("  abc123  ")).is_err());
    }

    #[test]
    fn rejects_over_max_length() {
        assert!(!is_safe_id(&"x".repeat(129)));
        assert!(!is_safe_id(&"x".repeat(1000)));
        assert!(required_session_id(Some(&"x".repeat(129))).is_err());
    }

    #[test]
    fn rejects_windows_traversal_paths() {
        // Full Windows-style attack paths.
        assert!(!is_safe_id("..\\..\\..\\windows\\system32"));
        assert!(!is_safe_id("C:\\..\\..\\secret"));
        assert!(!is_safe_id("\\\\?\\C:\\temp"));
    }

    #[test]
    fn error_is_400() {
        let err = required_session_id(Some("../x")).unwrap_err();
        assert_eq!(err.code, 400);
        assert_eq!(err.message, "invalid session id");
        let err = safe_artifact_segment(Some("a/b")).unwrap_err();
        assert_eq!(err.code, 400);
        assert_eq!(err.message, "invalid artifact path");
    }

    #[test]
    fn max_id_bytes_is_128() {
        assert_eq!(MAX_ID_BYTES, 128);
    }

    #[test]
    fn is_safe_segment_matches_strict_rules_without_max_length() {
        // Same rejections as is_safe_id...
        assert!(is_safe_segment("abc123"));
        assert!(!is_safe_segment(""));
        assert!(!is_safe_segment("a/b"));
        assert!(!is_safe_segment("a\\b"));
        assert!(!is_safe_segment(".."));
        assert!(!is_safe_segment("a\0b"));
        assert!(!is_safe_segment("a\nb"));
        assert!(!is_safe_segment("  abc"));
        // ...but no max-length limit (artifact names allow up to 180).
        assert!(is_safe_segment(&"x".repeat(129)));
        assert!(is_safe_segment(&"x".repeat(180)));
        assert!(!is_safe_id(&"x".repeat(129)));
    }
}
