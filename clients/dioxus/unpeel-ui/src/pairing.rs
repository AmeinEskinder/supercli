//! Pairing screen: paste a pairing code from the Host to link this device.
//!
//! The view is deliberately dumb — it collects the code and reports it.
//! The app shell owns the [`unpeel_client`] store, runs `pair()` on a
//! background thread, and feeds back [`PairingStatus`]. (QR scanning is a
//! platform seam the mobile launchers add later; the paste field is the
//! universal fallback and the desktop path.)

use dioxus::prelude::*;
use unpeel_client::PairedHostRecord;

/// Status of the in-flight pairing exchange, owned by the app shell.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PairingStatus {
    #[default]
    Idle,
    Working,
    Failed(String),
}

/// Paste-a-code pairing screen, plus the previously-paired host list with
/// connect/forget actions.
#[component]
pub fn PairingView(
    status: PairingStatus,
    keychain_notice: Option<String>,
    hosts: Vec<PairedHostRecord>,
    /// Ids of Hosts with a live connection. Their rows offer "Switch"
    /// instead of "Connect": switching never tears the connection down.
    connected_ids: Vec<String>,
    /// Id of the Host currently in view, if any. Its row gets a badge.
    active_id: Option<String>,
    on_submit: EventHandler<String>,
    on_connect: EventHandler<String>,
    on_forget: EventHandler<String>,
) -> Element {
    let mut code = use_signal(String::new);
    let working = matches!(status, PairingStatus::Working);
    let submittable = !working && !code.read().trim().is_empty();

    rsx! {
        div { class: "pairing", "data-testid": "pairing-view",
            h2 { "data-testid": "pairing-title", "Pair with an Unpeel Host" }
            p { class: "hint",
                "On the Host, show its pairing code, then paste it below. Codes are single-use and expire after a few minutes."
            }
            if let Some(notice) = keychain_notice {
                div { class: "notice", "{notice}" }
            }
            textarea {
                class: "pairing-code",
                "data-testid": "pairing-code-input",
                placeholder: "UNPEEL:1:host:port:…",
                aria_label: "Pairing code from the Host",
                rows: 3,
                spellcheck: false,
                disabled: working,
                value: "{code}",
                oninput: move |e| code.set(e.value()),
            }
            div { class: "actions",
                button {
                    "data-testid": "pairing-submit",
                    disabled: !submittable,
                    onclick: move |_| on_submit.call(code.read().trim().to_string()),
                    if working { "Pairing…" } else { "Pair" }
                }
            }
            if let PairingStatus::Failed(message) = &status {
                div { class: "error", "{message}" }
            }
            if !hosts.is_empty() {
                h3 { "Previously paired" }
                for host in hosts {
                    {
                        let connect_id = host.host_id.clone();
                        let forget_id = host.host_id.clone();
                        let is_active = active_id.as_deref() == Some(host.host_id.as_str());
                        let is_connected = connected_ids.iter().any(|id| id == &host.host_id);
                        rsx! {
                            div { class: "host-row", key: "{host.host_id}",
                                div { class: "host-info",
                                    span { class: "host-name", "{host.name}" }
                                    span { class: "host-endpoint", "{host.endpoint}" }
                                    if is_active {
                                        span { class: "host-badge", "● viewing" }
                                    } else if is_connected {
                                        span { class: "host-badge", "connected" }
                                    }
                                }
                                div { class: "host-actions",
                                    button {
                                        disabled: is_active,
                                        onclick: move |_| on_connect.call(connect_id.clone()),
                                        if is_active { "Viewing" } else if is_connected { "Switch" } else { "Connect" }
                                    }
                                    button {
                                        class: "danger",
                                        onclick: move |_| on_forget.call(forget_id.clone()),
                                        "Forget"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
