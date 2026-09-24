//! `unpeel-web`: Dioxus web target for the Unpeel clients.
//!
//! This is a **component preview**, not a functional Controller: it renders
//! the real `unpeel-ui` component library (pairing, session list, terminal,
//! find bar, toasts) in a browser against in-memory demo state. No network,
//! no threads, no filesystem, no keychain — everything the browser build
//! cannot do is replaced with scripted demo data, and the banner says so.
//!
//! Purpose: `dx build --platform web` proves the shared UI crate compiles
//! for `wasm32-unknown-unknown`, and the Playwright suite in `tests/web/`
//! proves it renders in headless Chromium.
//!
//! A real web Controller (live Host I/O over `fetch`/WebSocket) would need
//! the blocking transport layer rewritten against browser async APIs; that
//! is future work, not this target.

use dioxus::prelude::*;
use unpeel_client::dto::{ActivityState, SessionCapabilities, SessionStatus, SessionSummary};
use unpeel_client::types::PairedHostRecord;
use unpeel_ui::{
    AnnotationMode, AnnotationResult, BrowserGalleryPanel, Composer, DictationView, FindBar,
    FindState, GalleryDetailView, PairingStatus, PairingView, SessionList, TerminalModel,
    TerminalView, ToastCenter, ToastOverlay, APP_CSS,
};

fn main() {
    dioxus::launch(App);
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Pairing,
    Sessions,
    Terminal,
    Composer,
    Gallery,
    Dictation,
    Extras,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Pairing => "Pairing",
            Tab::Sessions => "Sessions",
            Tab::Terminal => "Terminal",
            Tab::Composer => "Composer",
            Tab::Gallery => "Gallery",
            Tab::Dictation => "Dictation",
            Tab::Extras => "Toasts & Find",
        }
    }
}

fn demo_hosts() -> Vec<PairedHostRecord> {
    vec![PairedHostRecord {
        host_id: "demo-mac-1".to_string(),
        name: "Demo Mac".to_string(),
        endpoint: "http://demo.invalid".to_string(),
        controller_device_id: "demo-controller".to_string(),
        paired_at_unix_ms: 1_787_000_000_000,
        certificate_fingerprint: None,
        remote_server_port: None,
        remote_server_certificate_fingerprint: None,
        link_enabled: Some(true),
    }]
}

fn demo_sessions() -> Vec<SessionSummary> {
    vec![
        SessionSummary {
            id: "sess-demo-1".to_string(),
            project_id: "demo".to_string(),
            active_runtime_id: None,
            runtime_launch_pending: false,
            provider_id: Some("codex".to_string()),
            title: "harness build".to_string(),
            command: "unpeel".to_string(),
            created_at_unix_ms: 1_786_999_100_000,
            updated_at_unix_ms: None,
            status: SessionStatus::Running,
            activity: ActivityState::Working,
            activity_source: None,
            unread: true,
            pinned: false,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            last_output_preview: Some("$ cargo test".to_string()),
            notify_when_done: false,
            terminal_background_hex: None,
            archived: false,
            capabilities: SessionCapabilities::default(),
        },
        SessionSummary {
            id: "sess-demo-2".to_string(),
            project_id: "demo".to_string(),
            active_runtime_id: None,
            runtime_launch_pending: false,
            provider_id: Some("claude".to_string()),
            title: "docs pass".to_string(),
            command: "unpeel".to_string(),
            created_at_unix_ms: 1_786_999_000_000,
            updated_at_unix_ms: None,
            status: SessionStatus::Running,
            activity: ActivityState::Idle,
            activity_source: None,
            unread: false,
            pinned: true,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            last_output_preview: None,
            notify_when_done: false,
            terminal_background_hex: None,
            archived: false,
            capabilities: SessionCapabilities::default(),
        },
    ]
}

/// Scripted PTY bytes: a fake shell session so the terminal demo has
/// styled content (bold prompt, colored output) without a Host.
const DEMO_SCRIPT: &[u8] = b"\x1b[1m$\x1b[0m unpeel status\r\n\x1b[32mhost:\x1b[0m demo-mac-1 (Direct)\r\nsessions: 2 running, 1 idle\r\n\x1b[1m$\x1b[0m _";

#[component]
fn App() -> Element {
    let mut tab = use_signal(|| Tab::Pairing);
    // The session selected from the Sessions tab; the Terminal tab renders it.
    let mut selected_session = use_signal(|| Some("sess-demo-1".to_string()));

    rsx! {
        style { "{APP_CSS}" }
        div { class: "web-demo",
            div { class: "demo-banner",
                "Web component preview — scripted demo data, no Host connection."
            }
            nav { class: "demo-tabs",
                for t in [Tab::Pairing, Tab::Sessions, Tab::Terminal, Tab::Composer, Tab::Gallery, Tab::Dictation, Tab::Extras] {
                    button {
                        key: "{t.label()}",
                        class: if *tab.read() == t { "demo-tab active" } else { "demo-tab" },
                        onclick: move |_| tab.set(t),
                        "{t.label()}"
                    }
                }
            }
            div { class: "demo-body",
                match *tab.read() {
                    Tab::Pairing => rsx! {
                        PairingDemo {
                            on_paired: move || tab.set(Tab::Sessions),
                        }
                    },
                    Tab::Sessions => rsx! {
                        SessionsDemo {
                            selected_id: selected_session(),
                            on_select: move |id: String| {
                                selected_session.set(Some(id));
                                // Faithful flow: opening a session navigates to its terminal.
                                tab.set(Tab::Terminal);
                            },
                        }
                    },
                    Tab::Terminal => rsx! {
                        TerminalDemo {
                            session_id: selected_session().unwrap_or_else(|| "sess-demo-1".to_string()),
                        }
                    },
                    Tab::Composer => rsx! { ComposerDemo {} },
                    Tab::Gallery => rsx! { GalleryDemo {} },
                    Tab::Dictation => rsx! { DictationDemo {} },
                    Tab::Extras => rsx! { ExtrasDemo {} },
                }
            }
        }
    }
}

#[component]
fn PairingDemo(on_paired: EventHandler<()>) -> Element {
    let mut status = use_signal(PairingStatus::default);
    let mut hosts = use_signal(demo_hosts);
    let mut active_id = use_signal(|| None::<String>);

    rsx! {
        PairingView {
            status: status(),
            keychain_notice: Some("Demo build: secrets stay in memory only.".to_string()),
            hosts: hosts(),
            connected_ids: vec!["demo-mac-1".to_string()],
            active_id: active_id(),
            on_submit: move |code: String| {
                // Demo pairing: accept the demo code, fail anything else.
                // On success the demo transitions to the Sessions tab, like
                // the real mobile launcher does after pairing.
                if code.trim() == "UNPEEL:1:demo" {
                    status.set(PairingStatus::Working);
                    on_paired.call(());
                } else {
                    status.set(PairingStatus::Failed(
                        "Demo build: pairing needs a real Host.".to_string(),
                    ));
                }
            },
            on_connect: move |id: String| active_id.set(Some(id)),
            on_forget: move |id: String| {
                hosts.write().retain(|h| h.host_id != id);
            },
        }
    }
}

#[component]
fn SessionsDemo(selected_id: Option<String>, on_select: EventHandler<String>) -> Element {
    let sessions = use_signal(demo_sessions);

    rsx! {
        SessionList {
            sessions: sessions(),
            selected_id: selected_id,
            on_select: move |id: String| on_select.call(id),
            on_archive: None,
            on_restore: None,
            on_organize: None,
            viewers: None,
        }
    }
}

#[component]
fn TerminalDemo(session_id: String) -> Element {
    let mut model = use_signal(|| {
        let mut m = TerminalModel::new(80, 24);
        m.feed(DEMO_SCRIPT);
        m
    });

    rsx! {
        div { class: "terminal-demo",
            p { class: "demo-hint", "Type into the terminal — keys echo through the real VT parser." }
            div { "data-testid": "terminal-session-id", hidden: true, "{session_id}" }
            TerminalView {
                snapshot: model.read().snapshot(),
                on_key: move |key: String| {
                    model.write().feed(key.as_bytes());
                },
            }
        }
    }
}

#[component]
fn GalleryDemo() -> Element {
    let mut entries = use_signal(|| {
        vec![unpeel_ui::GalleryEntry {
            meta: unpeel_client::ArtifactMeta {
                kind: "screenshot".to_string(),
                name: "demo-screenshot.png".to_string(),
                size: 1024,
                modified_at_unix_ms: 1_787_000_000_000,
            },
            preview_url: None,
        }]
    });
    let mut opened = use_signal(|| None::<unpeel_ui::GalleryEntry>);
    let mut editor = use_signal(|| None::<AnnotationMode>);
    let mut last_annotation = use_signal(|| None::<String>);

    // A 1x1 transparent PNG data URL so the detail view has an image to show.
    const PLACEHOLDER_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    rsx! {
        if let Some(entry) = opened() {
            GalleryDetailView {
                entry: entry.clone(),
                image_url: Some(PLACEHOLDER_PNG.to_string()),
                editor: editor(),
                on_close: move |_| {
                    opened.set(None);
                    editor.set(None);
                },
                on_delete: {
                    move |e: unpeel_ui::GalleryEntry| {
                        let name = e.meta.name.clone();
                        entries.write().retain(|x| x.meta.name != name);
                        opened.set(None);
                        editor.set(None);
                    }
                },
                on_editor: move |mode: Option<AnnotationMode>| editor.set(mode),
                on_annotation_done: move |result: AnnotationResult| {
                    let label = match &result {
                        AnnotationResult::Arrows(a) => format!("arrows:{}", a.len()),
                        AnnotationResult::Freehand(s) => format!("freehand:{}", s.len()),
                        AnnotationResult::Crop(_) => "crop".to_string(),
                    };
                    last_annotation.set(Some(label));
                },
                on_add_to_message: None,
                on_share: move |_| {},
            }
            if let Some(label) = last_annotation() {
                div { "data-testid": "annotation-result", class: "demo-hint", "Annotation done: {label}" }
            }
        } else {
            BrowserGalleryPanel {
                entries: entries(),
                loading: false,
                on_refresh: move |_| {},
                on_open: move |entry: unpeel_ui::GalleryEntry| {
                    opened.set(Some(entry));
                },
                on_delete: move |entry: unpeel_ui::GalleryEntry| {
                    let name = entry.meta.name.clone();
                    entries.write().retain(|e| e.meta.name != name);
                },
                on_upload: move |_| {},
                on_screenshot: move |_| {
                    // Demo: screenshot adds a new entry (mock Host capture).
                    let n = entries.read().len() + 1;
                    entries.write().push(unpeel_ui::GalleryEntry {
                        meta: unpeel_client::ArtifactMeta {
                            kind: "screenshot".to_string(),
                            name: format!("demo-screenshot-{n}.png"),
                            size: 2048,
                            modified_at_unix_ms: 1_787_000_000_000,
                        },
                        preview_url: None,
                    });
                },
            }
        }
    }
}

#[component]
fn ComposerDemo() -> Element {
    // Simulated turn state: the Host has no real running agent in the web
    // preview, so the demo script drives `turn_running` deterministically.
    let mut turn_running = use_signal(|| false);
    let mut session_id = use_signal(|| "sess-demo-1".to_string());
    let mut sent_log = use_signal(Vec::<String>::new);
    let mut stop_count = use_signal(|| 0u32);

    rsx! {
        div { class: "composer-demo",
            p { class: "demo-hint",
                "OpenMuse-style composer — deterministic simulation, no Host. "
                "Toggle the turn state to flip Send/Stop, queue follow-ups while a turn runs, "
                "and switch sessions to see per-session drafts survive."
            }
            div { class: "composer-demo-controls",
                button {
                    "data-testid": "turn-toggle",
                    onclick: move |_| turn_running.toggle(),
                    if *turn_running.read() { "End turn (simulate)" } else { "Start turn (simulate)" }
                }
                button {
                    "data-testid": "session-toggle",
                    onclick: move |_| {
                        let next = if *session_id.read() == "sess-demo-1" {
                            "sess-demo-2"
                        } else {
                            "sess-demo-1"
                        };
                        session_id.set(next.to_string());
                    },
                    "Switch session (now {session_id})"
                }
            }
            Composer {
                session_id: session_id(),
                turn_running: *turn_running.read(),
                on_send: move |msg: String| {
                    sent_log.write().push(msg);
                },
                on_stop: move |_| {
                    let n = *stop_count.read();
                    stop_count.set(n + 1);
                    // Simulate the Host finishing the turn when stopped.
                    turn_running.set(false);
                },
            }
            div { class: "composer-demo-log", "data-testid": "sent-log",
                "sent: {sent_log.read().len()}, stops: {stop_count}"
                for (i, msg) in sent_log.read().iter().enumerate() {
                    div { key: "{i}", class: "sent-msg", "{msg}" }
                }
            }
        }
    }
}

#[component]
fn DictationDemo() -> Element {
    let mut committed = use_signal(String::new);

    rsx! {
        div { class: "dictation-demo",
            p { class: "demo-hint",
                "Dictation control — the Web Speech API is stubbed in Playwright; "
                "the toggle exercises the control plane (start/stop) without a mic."
            }
            DictationView {
                settings: unpeel_ui::DictationSettings::default(),
                on_commit: move |text: String| committed.set(text),
            }
            if !committed.read().is_empty() {
                div { class: "demo-hint", "Committed: {committed}" }
            }
        }
    }
}
#[component]
fn ExtrasDemo() -> Element {
    let mut toasts = use_signal(ToastCenter::new);
    let mut model = use_signal(|| {
        let mut m = TerminalModel::new(80, 24);
        m.feed(DEMO_SCRIPT);
        m
    });
    let mut find = use_signal(FindState::new);

    let snapshot = model.read().snapshot();
    let highlight = find.read().highlight();

    rsx! {
        div { class: "extras-demo",
            div { class: "extras-row",
                button {
                    onclick: move |_| {
                        toasts.write().show("Demo toast from the shared ToastCenter.", 3.2);
                    },
                    "Show toast"
                }
                span { class: "demo-hint", "(tap the toast to dismiss)" }
            }
            ToastOverlay {
                toast: toasts.read().current().cloned(),
                on_tap: move |id: u64| toasts.write().dismiss_id(id),
            }
            div { class: "extras-row",
                FindBar {
                    query: find.read().query().to_string(),
                    counter: find.read().counter_text(),
                    on_query: move |q: String| {
                        let snap = model.read().snapshot();
                        find.write().set_query(&snap, q);
                    },
                    on_next: move |_| find.write().next(),
                    on_prev: move |_| find.write().prev(),
                    on_close: move |_| find.write().clear(),
                }
            }
            TerminalView {
                snapshot: snapshot,
                on_key: move |key: String| {
                    let snap = {
                        let mut m = model.write();
                        m.feed(key.as_bytes());
                        m.snapshot()
                    };
                    // Keep the find selection stable as new output arrives.
                    find.write().refresh(&snap);
                },
                find: highlight,
            }
        }
    }
}
