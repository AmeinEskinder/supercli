//! Chat composer: send/stop toggle, follow-up queue, per-session drafts.
//!
//! The state logic is pure Rust ([`ComposerState`]) so it can be unit-tested
//! without rendering; the Dioxus [`Composer`] component renders it and emits
//! intents back to the app shell via [`EventHandler`]s.
//!
//! A "turn" is one user message plus the agent's response. While a turn is
//! running the primary control flips from Send to Stop and anything the user
//! types lands in a visible follow-up queue instead of being sent. When the
//! turn finishes the queue drains in order. Drafts are kept per session id so
//! switching sessions preserves in-progress text.

use std::collections::HashMap;

use dioxus::html::Key;
use dioxus::prelude::*;

/// Pure composer state. Keyed by session id so drafts and queues survive
/// session switches without Dioxus context.
#[derive(Clone, Default, Debug)]
pub struct ComposerState {
    /// Unsent draft text per session id.
    drafts: HashMap<String, String>,
    /// Follow-ups typed while a turn was running, in FIFO order.
    followup_queue: Vec<String>,
}

impl ComposerState {
    /// Draft text currently held for `session_id`, or empty if none.
    pub fn draft_for(&self, session_id: &str) -> String {
        self.drafts.get(session_id).cloned().unwrap_or_default()
    }

    /// Save `text` as the draft for `session_id`. An empty draft is dropped
    /// so the map does not accumulate entries for untouched sessions.
    pub fn save_draft(&mut self, session_id: &str, text: String) {
        if text.trim().is_empty() {
            self.drafts.remove(session_id);
        } else {
            self.drafts.insert(session_id.to_string(), text);
        }
    }

    /// Add a follow-up typed while a turn is running. Blank messages are
    /// ignored and the index of the queued item is returned.
    pub fn queue_followup(&mut self, msg: String) -> Option<usize> {
        if msg.trim().is_empty() {
            return None;
        }
        self.followup_queue.push(msg);
        Some(self.followup_queue.len() - 1)
    }

    /// Remove the queued follow-up at `index`, if present.
    pub fn remove_queued(&mut self, index: usize) -> Option<String> {
        if index < self.followup_queue.len() {
            Some(self.followup_queue.remove(index))
        } else {
            None
        }
    }

    /// Replace the queued follow-up at `index` with `msg`.
    /// Returns false when the index is out of range or the new text is blank.
    pub fn edit_queued(&mut self, index: usize, msg: String) -> bool {
        if msg.trim().is_empty() {
            return false;
        }
        let Some(slot) = self.followup_queue.get_mut(index) else {
            return false;
        };
        *slot = msg;
        true
    }

    /// Snapshot of the current follow-up queue, oldest first.
    pub fn queue(&self) -> &[String] {
        &self.followup_queue
    }

    /// Drain the queue in FIFO order. The caller sends each returned message
    /// once the running turn has finished.
    pub fn drain_queue(&mut self) -> Vec<String> {
        std::mem::take(&mut self.followup_queue)
    }

    /// Whether a session has a non-empty draft waiting.
    pub fn has_draft(&self, session_id: &str) -> bool {
        self.drafts
            .get(session_id)
            .is_some_and(|d| !d.trim().is_empty())
    }
}

/// OpenMuse-style composer.
///
/// - One combined Send/Stop control: "Send" while idle, "Stop" while a turn
///   runs.
/// - Messages typed while a turn runs are queued visibly; each queued item
///   can be edited or removed before it is sent.
/// - Drafts are retained across session switches via `session_id`.
///
/// The app shell owns the `turn_running` flag (driven by Host events once
/// Phase 6 R4 lands; until then a local placeholder) and forwards intents
/// through `on_send` / `on_stop`.
#[component]
pub fn Composer(
    session_id: String,
    turn_running: bool,
    on_send: EventHandler<String>,
    on_stop: EventHandler<()>,
) -> Element {
    // Owned draft state lives with the component so each mounted composer
    // (desktop, mobile, web demo) keeps its own drafts per session.
    let mut state = use_signal(ComposerState::default);
    // The textarea's live text.
    let mut text = use_signal(String::new);
    // Which session the textarea currently reflects.
    let mut bound_session = use_signal(String::new);
    // Which queued item is being edited, if any, and its in-progress text.
    // (Kept at component scope: hooks must not be created inside the render
    // loop below.)
    let mut editing = use_signal(|| None::<usize>);
    let mut edit_text = use_signal(String::new);

    // Props are plain values, which effects do not track. Mirror them into
    // signals so the effects below re-run when the parent changes sessions
    // or turn state. Each guard keeps its sync convergent (no render loop):
    // the write fires only on an actual change.
    let mut session_sig = use_signal(|| session_id.clone());
    if *session_sig.read() != session_id {
        session_sig.set(session_id.clone());
    }
    let mut running_sig = use_signal(|| turn_running);
    if *running_sig.read() != turn_running {
        running_sig.set(turn_running);
    }

    // On session switch: stash the outgoing draft, restore the incoming one.
    use_effect(move || {
        let current = session_sig.read().clone();
        let prev = bound_session.read().clone();
        if prev != current {
            state.write().save_draft(&prev, text.read().clone());
            text.set(state.read().draft_for(&current));
            bound_session.set(current);
            editing.set(None);
            edit_text.set(String::new());
        }
    });

    // When a turn finishes, flush queued follow-ups in order.
    use_effect(move || {
        if !*running_sig.read() {
            let pending = state.write().drain_queue();
            if !pending.is_empty() {
                editing.set(None);
                for msg in pending {
                    on_send.call(msg);
                }
            }
        }
    });

    let queue: Vec<String> = state.read().queue().to_vec();
    let button_label = if turn_running { "Stop" } else { "Send" };
    // Shared by the Send/Stop button and the Enter key: idle sends the
    // draft, running stops the turn.
    let primary_action = {
        let sid = session_id.clone();
        move || {
            if turn_running {
                on_stop.call(());
            } else {
                let msg = text.read().clone();
                if !msg.trim().is_empty() {
                    text.set(String::new());
                    state.write().save_draft(&sid, String::new());
                    on_send.call(msg);
                }
            }
        }
    };
    // Shared by the Queue button and the Enter key while a turn runs.
    let queue_action = {
        let sid = session_id.clone();
        move || {
            let msg = text.read().clone();
            if state.write().queue_followup(msg).is_some() {
                text.set(String::new());
                state.write().save_draft(&sid, String::new());
            }
        }
    };

    rsx! {
        div { class: "composer",
            textarea {
                value: "{text}",
                aria_label: "Message the agent",
                oninput: move |e| text.set(e.value()),
                onkeydown: {
                    let mut primary_action = primary_action.clone();
                    let mut queue_action = queue_action.clone();
                    move |e| {
                        // Enter sends (idle) or queues (running); Shift+Enter
                        // keeps the native newline. This is what the
                        // placeholder text promises, so it must exist.
                        if e.key() == Key::Enter && !e.modifiers().shift() {
                            e.prevent_default();
                            if turn_running {
                                queue_action();
                            } else {
                                primary_action();
                            }
                        }
                    }
                },
                placeholder: if turn_running {
                    "Turn running — type a follow-up… (Enter to queue)"
                } else {
                    "Message the agent… (Enter to send)"
                },
            }
            button {
                class: if turn_running { "stop" } else { "send" },
                // No aria-label: the visible "Send"/"Stop" text is the
                // accessible name (an override would break exact-name
                // lookups and WCAG 2.5.3 label-in-name).
                onclick: {
                    let mut primary_action = primary_action.clone();
                    move |_| primary_action()
                },
                {button_label}
            }
            if turn_running {
                // Queuing row: the textarea acts as the queue entry; pressing
                // the (now "Queue") control adds it instead of sending.
                button {
                    class: "queue-add",
                    // Visible "Queue" text is the accessible name.
                    onclick: {
                        let mut queue_action = queue_action.clone();
                        move |_| queue_action()
                    },
                    "Queue"
                }
            }
            if !queue.is_empty() {
                div { class: "followup-queue",
                    div { class: "queue-label", "Follow-ups ({queue.len()})" }
                    for (i, msg) in queue.iter().enumerate() {
                        div { class: "queued-item", key: "{i}",
                            if editing.read().as_ref() == Some(&i) {
                                {
                                    rsx! {
                                        input {
                                            class: "queued-edit",
                                            r#type: "text",
                                            aria_label: "Edit queued follow-up",
                                            value: "{edit_text}",
                                            oninput: move |e| edit_text.set(e.value()),
                                        }
                                        button {
                                            class: "queued-save",
                                            onclick: move |_| {
                                                let v = edit_text.read().clone();
                                                if state.write().edit_queued(i, v) {
                                                    editing.set(None);
                                                }
                                            },
                                            "Save"
                                        }
                                        button {
                                            class: "queued-cancel",
                                            onclick: move |_| editing.set(None),
                                            "Cancel"
                                        }
                                    }
                                }
                            } else {
                                span { class: "queued-text", "{msg}" }
                                button {
                                    class: "queued-edit-btn",
                                    onclick: {
                                        let current_msg = msg.clone();
                                        move |_| {
                                            edit_text.set(current_msg.clone());
                                            editing.set(Some(i));
                                        }
                                    },
                                    "Edit"
                                }
                                button {
                                    class: "queued-remove",
                                    onclick: move |_| {
                                        state.write().remove_queued(i);
                                        if editing.read().as_ref() == Some(&i) {
                                            editing.set(None);
                                        }
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_are_per_session_and_survive_switches() {
        let mut s = ComposerState::default();
        s.save_draft("a", "hello a".to_string());
        s.save_draft("b", "hello b".to_string());
        assert_eq!(s.draft_for("a"), "hello a");
        assert_eq!(s.draft_for("b"), "hello b");
        assert_eq!(s.draft_for("c"), "");
        assert!(s.has_draft("a"));
        assert!(!s.has_draft("c"));
    }

    #[test]
    fn blank_drafts_are_dropped() {
        let mut s = ComposerState::default();
        s.save_draft("a", "   ".to_string());
        assert!(!s.has_draft("a"));
        assert_eq!(s.draft_for("a"), "");
    }

    #[test]
    fn queue_is_fifo_and_drains_in_order() {
        let mut s = ComposerState::default();
        assert_eq!(s.queue_followup("first".into()), Some(0));
        assert_eq!(s.queue_followup("second".into()), Some(1));
        assert_eq!(s.queue_followup("   ".into()), None);
        assert_eq!(s.queue(), &["first".to_string(), "second".to_string()]);
        let drained = s.drain_queue();
        assert_eq!(drained, vec!["first".to_string(), "second".to_string()]);
        assert!(s.queue().is_empty());
    }

    #[test]
    fn queued_items_are_editable_and_removable() {
        let mut s = ComposerState::default();
        s.queue_followup("one".into());
        s.queue_followup("two".into());
        assert!(s.edit_queued(0, "ONE".into()));
        assert!(!s.edit_queued(1, "   ".into()));
        assert!(!s.edit_queued(9, "x".into()));
        assert_eq!(s.queue()[0], "ONE");
        assert_eq!(s.remove_queued(1), Some("two".to_string()));
        assert_eq!(s.remove_queued(5), None);
        assert_eq!(s.queue().len(), 1);
    }
}
