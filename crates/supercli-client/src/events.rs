//! Client DTOs for the typed session-event stream (Phase 6 R4).
//!
//! Mirrors `supercli-serve/src/session_events.rs`. The wire uses `kind` as the
//! discriminant; [`SessionEventWire`] is internally tagged so unknown kinds
//! from newer Hosts deserialize to [`SessionEventWire::Unknown`] instead of
//! failing the whole poll.
//!
//! [`ClientEventCursor`] tracks the per-session `after_seq` cursor so the
//! caller can resume polling without loss or duplication.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One session event as it arrives on the wire.
///
/// Internally tagged on `kind`. Unknown kinds (from newer Hosts) fall into
/// [`SessionEventWire::Unknown`] and never fail the poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SessionEventWire {
    #[serde(rename = "turn.started")]
    TurnStarted {
        session_id: String,
        seq: u64,
        at_ms: u64,
        turn_id: String,
        trigger: String,
    },
    #[serde(rename = "turn.finished")]
    TurnFinished {
        session_id: String,
        seq: u64,
        at_ms: u64,
        turn_id: String,
        outcome: String,
    },
    #[serde(rename = "tool.requested")]
    ToolRequested {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        tool: String,
        summary: String,
    },
    #[serde(rename = "tool.approved")]
    ToolApproved {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        answered_by: Option<String>,
    },
    #[serde(rename = "tool.denied")]
    ToolDenied {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        answered_by: Option<String>,
    },
    #[serde(rename = "needs_review")]
    NeedsReview {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        reason: String,
    },
    #[serde(rename = "turn.cancelled")]
    TurnCancelled {
        session_id: String,
        seq: u64,
        at_ms: u64,
        reason: String,
        #[serde(default)]
        ambiguous_attempts: Vec<String>,
    },
    #[serde(rename = "tool.executed")]
    ToolExecuted {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        exit: String,
    },
    #[serde(rename = "tool.ambiguous")]
    ToolAmbiguous {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        reason: String,
    },
    #[serde(rename = "tool.never_ran")]
    ToolNeverRan {
        session_id: String,
        seq: u64,
        at_ms: u64,
        review_id: String,
        reason: String,
    },
    #[serde(rename = "lease.fenced")]
    LeaseFenced {
        session_id: String,
        seq: u64,
        at_ms: u64,
        generation: u64,
        holder: String,
    },
    #[serde(rename = "lease.taken_over")]
    LeaseTakenOver {
        session_id: String,
        seq: u64,
        at_ms: u64,
        generation: u64,
        holder: String,
        previous_holder: String,
    },
    /// Unknown kinds from newer Hosts: kept opaque, never fail the poll.
    #[serde(other)]
    Unknown,
}

impl SessionEventWire {
    pub fn kind(&self) -> &'static str {
        match self {
            SessionEventWire::TurnStarted { .. } => "turn.started",
            SessionEventWire::TurnFinished { .. } => "turn.finished",
            SessionEventWire::ToolRequested { .. } => "tool.requested",
            SessionEventWire::ToolApproved { .. } => "tool.approved",
            SessionEventWire::ToolDenied { .. } => "tool.denied",
            SessionEventWire::NeedsReview { .. } => "needs_review",
            SessionEventWire::TurnCancelled { .. } => "turn.cancelled",
            SessionEventWire::ToolExecuted { .. } => "tool.executed",
            SessionEventWire::ToolAmbiguous { .. } => "tool.ambiguous",
            SessionEventWire::ToolNeverRan { .. } => "tool.never_ran",
            SessionEventWire::LeaseFenced { .. } => "lease.fenced",
            SessionEventWire::LeaseTakenOver { .. } => "lease.taken_over",
            SessionEventWire::Unknown => "unknown",
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            SessionEventWire::TurnStarted { session_id, .. }
            | SessionEventWire::TurnFinished { session_id, .. }
            | SessionEventWire::ToolRequested { session_id, .. }
            | SessionEventWire::ToolApproved { session_id, .. }
            | SessionEventWire::ToolDenied { session_id, .. }
            | SessionEventWire::NeedsReview { session_id, .. }
            | SessionEventWire::TurnCancelled { session_id, .. }
            | SessionEventWire::ToolExecuted { session_id, .. }
            | SessionEventWire::ToolAmbiguous { session_id, .. }
            | SessionEventWire::ToolNeverRan { session_id, .. }
            | SessionEventWire::LeaseFenced { session_id, .. }
            | SessionEventWire::LeaseTakenOver { session_id, .. } => Some(session_id),
            SessionEventWire::Unknown => None,
        }
    }

    pub fn seq(&self) -> Option<u64> {
        match self {
            SessionEventWire::TurnStarted { seq, .. }
            | SessionEventWire::TurnFinished { seq, .. }
            | SessionEventWire::ToolRequested { seq, .. }
            | SessionEventWire::ToolApproved { seq, .. }
            | SessionEventWire::ToolDenied { seq, .. }
            | SessionEventWire::NeedsReview { seq, .. }
            | SessionEventWire::TurnCancelled { seq, .. }
            | SessionEventWire::ToolExecuted { seq, .. }
            | SessionEventWire::ToolAmbiguous { seq, .. }
            | SessionEventWire::ToolNeverRan { seq, .. }
            | SessionEventWire::LeaseFenced { seq, .. }
            | SessionEventWire::LeaseTakenOver { seq, .. } => Some(*seq),
            SessionEventWire::Unknown => None,
        }
    }

    /// Whether this event means a turn is now running (drives Send→Stop).
    pub fn is_turn_started(&self) -> bool {
        matches!(self, SessionEventWire::TurnStarted { .. })
    }

    /// Whether this event means the turn ended (drives Stop→Send).
    /// A `turn.cancelled` also ends the turn from the client's perspective:
    /// the Host interrupted it, so the composer returns to Send.
    pub fn is_turn_finished(&self) -> bool {
        matches!(
            self,
            SessionEventWire::TurnFinished { .. } | SessionEventWire::TurnCancelled { .. }
        )
    }
}

/// Response body of `GET /mobile/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse {
    #[serde(default)]
    pub events: Vec<SessionEventWire>,
    #[serde(default)]
    pub next_seq: u64,
    #[serde(default)]
    pub resync: bool,
}

/// Per-session poll cursor. Keeps `after_seq` per session so a client can
/// poll in a loop without losing or duplicating events.
#[derive(Debug, Clone, Default)]
pub struct ClientEventCursor {
    cursors: HashMap<String, u64>,
}

impl ClientEventCursor {
    pub fn new() -> Self {
        ClientEventCursor {
            cursors: HashMap::new(),
        }
    }

    /// The `after_seq` to send for this session (0 = everything buffered).
    pub fn next(&self, session_id: &str) -> u64 {
        self.cursors.get(session_id).copied().unwrap_or(0)
    }

    /// Advance the cursor from a poll response. On `resync`, the caller
    /// should re-bootstrap session state, then the cursor still advances.
    pub fn advance(&mut self, session_id: &str, response: &EventsResponse) {
        self.cursors
            .insert(session_id.to_owned(), response.next_seq);
    }

    pub fn reset(&mut self, session_id: &str) {
        self.cursors.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_started_json() -> serde_json::Value {
        serde_json::json!({
            "kind": "turn.started",
            "session_id": "s1",
            "seq": 41,
            "at_ms": 1710000000000_u64,
            "turn_id": "t-9f3",
            "trigger": "user",
        })
    }

    #[test]
    fn deserializes_turn_started() {
        let event: SessionEventWire =
            serde_json::from_value(turn_started_json()).expect("turn.started");
        assert!(event.is_turn_started());
        assert!(!event.is_turn_finished());
        assert_eq!(event.seq(), Some(41));
        assert_eq!(event.session_id(), Some("s1"));
    }

    #[test]
    fn deserializes_tool_requested() {
        let event: SessionEventWire = serde_json::from_value(serde_json::json!({
            "kind": "tool.requested",
            "session_id": "s1",
            "seq": 43,
            "at_ms": 1,
            "review_id": "r-77",
            "tool": "bash.exec",
            "summary": "cargo test",
        }))
        .expect("tool.requested");
        assert_eq!(event.kind(), "tool.requested");
        match event {
            SessionEventWire::ToolRequested {
                review_id, tool, ..
            } => {
                assert_eq!(review_id, "r-77");
                assert_eq!(tool, "bash.exec");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn unknown_kind_does_not_fail_poll() {
        let response: EventsResponse = serde_json::from_value(serde_json::json!({
            "events": [
                turn_started_json(),
                {
                    "kind": "lease.changed",
                    "session_id": "s1",
                    "seq": 48,
                    "at_ms": 2,
                    "generation": 7,
                },
            ],
            "next_seq": 48,
            "resync": false,
        }))
        .expect("poll with future kind");
        assert_eq!(response.events.len(), 2);
        assert!(response.events[0].is_turn_started());
        assert_eq!(response.events[1], SessionEventWire::Unknown);
        assert_eq!(response.next_seq, 48);
    }

    #[test]
    fn cursor_advances_per_session() {
        let mut cursor = ClientEventCursor::new();
        assert_eq!(cursor.next("s1"), 0);
        cursor.advance(
            "s1",
            &EventsResponse {
                events: vec![],
                next_seq: 42,
                resync: false,
            },
        );
        assert_eq!(cursor.next("s1"), 42);
        // Other sessions are independent.
        assert_eq!(cursor.next("s2"), 0);
        cursor.reset("s1");
        assert_eq!(cursor.next("s1"), 0);
    }

    #[test]
    fn turn_finished_drives_stop_to_send() {
        let event: SessionEventWire = serde_json::from_value(serde_json::json!({
            "kind": "turn.finished",
            "session_id": "s1",
            "seq": 42,
            "at_ms": 3,
            "turn_id": "t-9f3",
            "outcome": "completed",
        }))
        .expect("turn.finished");
        assert!(event.is_turn_finished());
        assert!(!event.is_turn_started());
    }

    #[test]
    fn approval_answer_carries_actor() {
        let event: SessionEventWire = serde_json::from_value(serde_json::json!({
            "kind": "tool.approved",
            "session_id": "s1",
            "seq": 44,
            "at_ms": 4,
            "review_id": "r-77",
            "answered_by": "device:phone-1",
        }))
        .expect("tool.approved");
        match event {
            SessionEventWire::ToolApproved { answered_by, .. } => {
                assert_eq!(answered_by.as_deref(), Some("device:phone-1"));
            }
            _ => panic!("wrong variant"),
        }
    }
}
