//! Port of the portable parts of `RemoteTerminalWebSocket.swift`
//! (SupercliIOS).
//!
//! The Swift file owns the live WebSocket (certificate-pinned URLSession,
//! ping watchdog, input router). The live socket is now ported in
//! [`crate::remote_ws_transport`] (native targets only, via tungstenite +
//! rustls with exact leaf pinning). What IS portable and ported here:
//!
//! - [`RemoteTerminalWebSocketError`] — the error enum;
//! - [`InputRoute`] / [`route_input`] — the input router's transport
//!   decision: prefer a healthy WebSocket, fall back to HTTP, with the
//!   idempotency and size-threshold rules;
//! - [`WS_CLOSE_TIMEOUT_SECS`] — the bound on a WebSocket send attempt.
//!
//! The pure protocol parsing (hello/error frames, binary frame layout)
//! already lives in `remote_terminal_stream.rs`.
//!
//! Web-safe: compiles for `wasm32-unknown-unknown`.

/// Errors from the terminal-output WebSocket.
///
/// Port of `RemoteTerminalWebSocketError` from
/// `RemoteTerminalWebSocket.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RemoteTerminalWebSocketError {
    /// The server's first frame was not the hello frame within the timeout.
    #[error("hello timeout: server did not send hello frame")]
    HelloTimeout,
    /// The server sent a frame that violates the protocol.
    #[error("protocol violation: unexpected server frame")]
    ProtocolViolation,
    /// The socket is closed.
    #[error("websocket closed")]
    Closed,
}

/// Maximum seconds to wait for a WebSocket send before retiring the socket
/// and falling back to HTTP.
///
/// Port of the `0.5` second timeout in `RemoteTerminalInputRouter.send(_:)`
/// from `RemoteTerminalWebSocket.swift`. A WS that reported "not closed" can
/// still be half-dead; without this bound every queued keystroke would wait
/// behind the stalled send (the 3–5s typing lag the Swift comment describes).
pub const WS_SEND_TIMEOUT_SECS: f64 = 0.5;

/// Maximum input bytes per WebSocket frame.
///
/// Large pastes are routed directly over HTTP so delivery is one idempotent
/// operation — a split WS send could time out after only some frames
/// arrived, which cannot be made equivalent to retrying the whole string
/// under one idempotency key.
///
/// Port of `RemoteTerminalWSClientMessage.maxInputBytesPerFrame` as used in
/// `RemoteTerminalInputRouter.send(_:)`.
///
/// The Swift value is `32 * 1024` (see
/// `RemoteTerminalStreamTransport.swift:211`); this matches
/// [`crate::terminal_stream::RemoteTerminalWsClientMessage::MAX_INPUT_BYTES_PER_FRAME`].
pub const MAX_INPUT_BYTES_PER_FRAME: usize = 32 * 1024;

/// Where a unit of PTY input should be sent.
///
/// Port of the routing decision in `RemoteTerminalInputRouter.send(_:)` from
/// `RemoteTerminalWebSocket.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRoute {
    /// Send over the healthy WebSocket (bounded by [`WS_SEND_TIMEOUT_SECS`]).
    WebSocket,
    /// Send over HTTP (the proven write path).
    Http,
}

/// Decide where a unit of PTY input should be sent.
///
/// Rules (from `RemoteTerminalInputRouter.send(_:)`):
/// 1. If the text exceeds [`MAX_INPUT_BYTES_PER_FRAME`], route over HTTP
///    directly — a split WS send cannot be made idempotent.
/// 2. If a WebSocket connection is healthy, try it (the caller bounds the
///    attempt with [`WS_SEND_TIMEOUT_SECS`]).
/// 3. Otherwise (no WS, or the WS attempt failed/stalled), use HTTP.
///
/// `websocket_healthy` should reflect whether a WS connection exists and is
/// not closed. On a WS send failure the caller should retire that socket
/// (so later keystrokes skip it) and fall back to HTTP for this send.
///
/// Returns the route. The caller generates one idempotency key per logical
/// send and passes it to whichever transport executes, so a WS attempt whose
/// delivery was ambiguous does not double-apply the keystroke when the HTTP
/// fallback reuses the key.
pub fn route_input(text: &str, websocket_healthy: bool) -> InputRoute {
    if text.len() > MAX_INPUT_BYTES_PER_FRAME {
        return InputRoute::Http;
    }
    if websocket_healthy {
        InputRoute::WebSocket
    } else {
        InputRoute::Http
    }
}

/// Outcome of a WebSocket send attempt, telling the caller what to do next.
///
/// Port of the try/retire/fall-through logic in
/// `RemoteTerminalInputRouter.send(_:)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsSendOutcome {
    /// The send succeeded; nothing more to do.
    Sent,
    /// The send failed or stalled; retire the socket and send over HTTP.
    RetireAndFallbackToHttp,
}

/// Map a WebSocket send result to the next action.
///
/// `Ok(())` → [`WsSendOutcome::Sent`]; any error (including timeout) →
/// [`WsSendOutcome::RetireAndFallbackToHttp`].
pub fn on_ws_send_result(result: Result<(), RemoteTerminalWebSocketError>) -> WsSendOutcome {
    match result {
        Ok(()) => WsSendOutcome::Sent,
        Err(_) => WsSendOutcome::RetireAndFallbackToHttp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        assert_eq!(
            RemoteTerminalWebSocketError::HelloTimeout.to_string(),
            "hello timeout: server did not send hello frame"
        );
        assert_eq!(
            RemoteTerminalWebSocketError::ProtocolViolation.to_string(),
            "protocol violation: unexpected server frame"
        );
        assert_eq!(
            RemoteTerminalWebSocketError::Closed.to_string(),
            "websocket closed"
        );
    }

    #[test]
    fn route_prefers_healthy_websocket() {
        assert_eq!(route_input("ls\n", true), InputRoute::WebSocket);
    }

    #[test]
    fn route_falls_back_to_http_without_websocket() {
        assert_eq!(route_input("ls\n", false), InputRoute::Http);
    }

    #[test]
    fn route_large_paste_goes_http_even_with_healthy_ws() {
        let big = "x".repeat(MAX_INPUT_BYTES_PER_FRAME + 1);
        assert_eq!(route_input(&big, true), InputRoute::Http);
    }

    #[test]
    fn route_at_threshold_stays_websocket() {
        let at = "x".repeat(MAX_INPUT_BYTES_PER_FRAME);
        assert_eq!(route_input(&at, true), InputRoute::WebSocket);
    }

    #[test]
    fn ws_send_outcome_ok_is_sent() {
        assert_eq!(on_ws_send_result(Ok(())), WsSendOutcome::Sent);
    }

    #[test]
    fn ws_send_outcome_err_retires() {
        assert_eq!(
            on_ws_send_result(Err(RemoteTerminalWebSocketError::Closed)),
            WsSendOutcome::RetireAndFallbackToHttp
        );
        // Timeout is also an error → retire.
        assert_eq!(
            on_ws_send_result(Err(RemoteTerminalWebSocketError::HelloTimeout)),
            WsSendOutcome::RetireAndFallbackToHttp
        );
    }

    #[test]
    fn timeout_constant_matches_swift() {
        // The Swift original uses 0.5 seconds.
        assert_eq!(WS_SEND_TIMEOUT_SECS, 0.5);
    }
}
