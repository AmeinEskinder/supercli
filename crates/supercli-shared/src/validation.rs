//! Path and identifier validation for session-scoped resources.
//!
//! Ported from `MobileSessionControl.swift` (SupercliNative):
//! - `requiredSessionID` — session ID must be non-empty with no path traversal
//! - `safeArtifactSegment` — artifact kind/name segments with no traversal
//!
//! These are security-critical: a crafted `session_id`, `kind`, or `name`
//! must not escape the session artifacts directory.

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
/// Trims whitespace; rejects empty values and anything containing `/` or
/// `..` (path traversal). Returns the trimmed ID on success.
pub fn required_session_id(value: Option<&str>) -> Result<String, ValidationError> {
    let id = value.unwrap_or("").trim();
    if id.is_empty() || id.contains('/') || id.contains("..") {
        return Err(ValidationError::bad_request("invalid session id"));
    }
    Ok(id.to_string())
}

/// Validate a single artifact path segment (`kind` or `name`).
///
/// Trims whitespace; rejects empty values and anything containing `/`,
/// `\`, or `..`. Same traversal rule the desktop applies to session IDs,
/// applied to artifact segments so a crafted request can't escape the
/// artifacts dir.
pub fn safe_artifact_segment(value: Option<&str>) -> Result<String, ValidationError> {
    let segment = value.unwrap_or("").trim();
    if segment.is_empty()
        || segment.contains('/')
        || segment.contains('\\')
        || segment.contains("..")
    {
        return Err(ValidationError::bad_request("invalid artifact path"));
    }
    Ok(segment.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_session_id_accepts_valid() {
        assert_eq!(required_session_id(Some("abc123")).unwrap(), "abc123");
        assert_eq!(required_session_id(Some("  abc123  ")).unwrap(), "abc123");
    }

    #[test]
    fn required_session_id_rejects_empty() {
        assert!(required_session_id(None).is_err());
        assert!(required_session_id(Some("")).is_err());
        assert!(required_session_id(Some("   ")).is_err());
    }

    #[test]
    fn required_session_id_rejects_traversal() {
        assert!(required_session_id(Some("../etc")).is_err());
        assert!(required_session_id(Some("a/b")).is_err());
        assert!(required_session_id(Some("..")).is_err());
        assert!(required_session_id(Some("a..b")).is_err());
    }

    #[test]
    fn required_session_id_error_is_400() {
        let err = required_session_id(Some("../x")).unwrap_err();
        assert_eq!(err.code, 400);
        assert_eq!(err.message, "invalid session id");
    }

    #[test]
    fn safe_artifact_segment_accepts_valid() {
        assert_eq!(safe_artifact_segment(Some("screenshots")).unwrap(), "screenshots");
        assert_eq!(safe_artifact_segment(Some("img.png")).unwrap(), "img.png");
    }

    #[test]
    fn safe_artifact_segment_rejects_traversal() {
        assert!(safe_artifact_segment(Some("../secret")).is_err());
        assert!(safe_artifact_segment(Some("a/b")).is_err());
        assert!(safe_artifact_segment(Some("a\\b")).is_err());
        assert!(safe_artifact_segment(Some("..")).is_err());
        assert!(safe_artifact_segment(None).is_err());
        assert!(safe_artifact_segment(Some("")).is_err());
    }

    #[test]
    fn safe_artifact_segment_error_is_400() {
        let err = safe_artifact_segment(Some("a/b")).unwrap_err();
        assert_eq!(err.code, 400);
        assert_eq!(err.message, "invalid artifact path");
    }
}
