//! Viewer presence — port of `ViewerPresence.swift`.
//!
//! Tracks which other devices are currently viewing a session's terminal,
//! so session rows can show presence chips. Presence is device-level
//! observation, not human membership or a terminal control lease.
//!
//! Two feeds converge here, exactly like Swift:
//!
//! - File feed: the Rust Host writes `<unpeel-home>/remote/presence.json`
//!   whenever remote viewers change:
//!   `{"version":1,"updated_at":ms,"sessions":{"<id>":[{"ip",
//!   "kind":"ws"|"poll","device":{t("presence.name_id")}|null,"last_seen":ms}]}}`.
//!   Poll viewers have a 15s TTL server-side; entries whose `last_seen` is
//!   older than [`FILE_ENTRY_TTL_MS`] are treated as stale on read.
//! - Mobile feed: the workspace worker publishes authenticated Direct/Link
//!   output leases beside it as `mobile-presence.json` (same shape,
//!   [`MOBILE_ENTRY_TTL_MS`] TTL).
//!
//! The launcher drives [`PresenceStore::refresh`] on its own poll loop
//! (Swift uses a directory watcher plus a 5s fallback timer; a poll loop
//! is the portable equivalent).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::i18n::t;
use dioxus::prelude::*;

/// File-feed staleness cutoff. The remote server prunes poll viewers after
/// 15s; anything older than this on disk is a leftover from a dead server.
pub const FILE_ENTRY_TTL_MS: u64 = 20_000;
/// Host Direct/Link output lease TTL. The legacy filename is mobile, but
/// this feed also carries paired Mac Controllers.
pub const MOBILE_ENTRY_TTL_MS: u64 = 15_000;
/// How often the launcher should re-read the files (Swift's fallback).
pub const POLL_INTERVAL_MS: u64 = 5_000;

/// One device currently viewing a session's terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerInfo {
    pub id: String,
    /// Stable paired-device id when the viewer is an authenticated
    /// Controller. `None` for IP-only/legacy viewers, which therefore
    /// cannot suppress a particular phone's APNs target.
    pub device_id: Option<String>,
    pub display_name: String,
    pub last_seen_ms: u64,
}

/// The two presence files beside each other under the Host's home dir.
pub fn presence_file_paths(unpeel_home: &Path) -> (PathBuf, PathBuf) {
    let remote = unpeel_home.join("remote");
    (
        remote.join("presence.json"),
        remote.join("mobile-presence.json"),
    )
}

#[derive(Debug, serde::Deserialize)]
struct PresenceFile {
    sessions: Option<HashMap<String, Vec<PresenceEntry>>>,
}

#[derive(Debug, serde::Deserialize)]
struct PresenceEntry {
    ip: Option<String>,
    device: Option<String>,
    last_seen: Option<i64>,
}

/// Parse one presence file. `source` labels IP-only viewer ids
/// (`"legacy:<source>:<identity>"`), mirroring Swift.
pub fn parse_presence(data: &[u8], source: &str) -> HashMap<String, Vec<ViewerInfo>> {
    let file: PresenceFile = match serde_json::from_slice(data) {
        Ok(f) => f,
        Err(_) => return HashMap::new(),
    };
    let mut result: HashMap<String, Vec<ViewerInfo>> = HashMap::new();
    for (session_id, entries) in file.sessions.unwrap_or_default() {
        let mut list = Vec::new();
        for entry in entries {
            let identity = entry
                .device
                .clone()
                .filter(|d| !d.is_empty())
                .or_else(|| entry.ip.clone())
                .unwrap_or_else(|| "remote".to_string());
            let device_id = device_id_from_device(entry.device.as_deref());
            let id = match &device_id {
                Some(d) => format!("device:{d}"),
                None => format!("legacy:{source}:{identity}"),
            };
            // Keep duplicates until the timestamp-aware merge: the first
            // connection in the file may be older than another live one.
            list.push(ViewerInfo {
                id,
                device_id,
                display_name: display_name_from_device(
                    entry.device.as_deref(),
                    entry.ip.as_deref(),
                ),
                last_seen_ms: entry.last_seen.unwrap_or(0).max(0) as u64,
            });
        }
        if !list.is_empty() {
            result.insert(session_id, list);
        }
    }
    result
}

/// The remote server records `device` as {t("presence.name_id")}; show just the name.
pub fn display_name_from_device(device: Option<&str>, ip: Option<&str>) -> String {
    if let Some(device) = device.filter(|d| !d.is_empty()) {
        if device.ends_with(')') {
            if let Some(open) = device.rfind(" (") {
                let name = &device[..open];
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
        return device.to_string();
    }
    ip.map(|s| s.to_string())
        .unwrap_or_else(|| t("presence.remote_viewer"))
}

/// The remote server records authenticated Controllers as {t("presence.name_id")}.
fn device_id_from_device(device: Option<&str>) -> Option<String> {
    let device = device?;
    if !device.ends_with(')') {
        return None;
    }
    let open = device.rfind(" (")?;
    let id = &device[open + 2..device.len() - 1];
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Merge the feeds: expire each source by its TTL *before* merging (a stale
/// lease in one transport must not hide the same device's live lease in the
/// other), newest-wins per viewer id, sorted by display name then id.
pub fn merge_presence(
    feeds: &[(&HashMap<String, Vec<ViewerInfo>>, u64)],
    now_ms: u64,
) -> HashMap<String, Vec<ViewerInfo>> {
    let mut by_session: HashMap<String, HashMap<String, ViewerInfo>> = HashMap::new();
    for (feed, ttl) in feeds {
        for (session_id, entries) in *feed {
            for entry in entries {
                if now_ms.saturating_sub(entry.last_seen_ms) > *ttl {
                    continue;
                }
                let slot = by_session.entry(session_id.clone()).or_default();
                match slot.get(&entry.id) {
                    Some(prev) if prev.last_seen_ms >= entry.last_seen_ms => {}
                    _ => {
                        slot.insert(entry.id.clone(), entry.clone());
                    }
                }
            }
        }
    }
    by_session
        .into_iter()
        .map(|(session_id, entries)| {
            let mut viewers: Vec<ViewerInfo> = entries.into_values().collect();
            viewers.sort_by(|a, b| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
                    .then_with(|| a.id.cmp(&b.id))
            });
            (session_id, viewers)
        })
        .collect()
}

/// Poll-driven presence store. The launcher calls [`PresenceStore::refresh`]
/// every [`POLL_INTERVAL_MS`]; new arrivals are returned as display names
/// so the launcher can toast them (Swift shows {t("presence.name_connected")}).
#[derive(Debug, Clone)]
pub struct PresenceStore {
    // On wasm32 there is no filesystem, so `refresh()` never reads these;
    // the paths are kept so the constructor signature stays uniform.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    presence_path: PathBuf,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    mobile_presence_path: PathBuf,
    file_viewers: HashMap<String, Vec<ViewerInfo>>,
    mobile_file_viewers: HashMap<String, Vec<ViewerInfo>>,
    viewers: HashMap<String, Vec<ViewerInfo>>,
    announced_ids: HashSet<String>,
    seeded: bool,
}

impl PresenceStore {
    pub fn new(presence_path: PathBuf, mobile_presence_path: PathBuf) -> Self {
        Self {
            presence_path,
            mobile_presence_path,
            file_viewers: HashMap::new(),
            mobile_file_viewers: HashMap::new(),
            viewers: HashMap::new(),
            announced_ids: HashSet::new(),
            seeded: false,
        }
    }

    /// Session id → current viewers, already de-staled and sorted.
    pub fn viewers(&self) -> &HashMap<String, Vec<ViewerInfo>> {
        &self.viewers
    }

    pub fn viewers_for(&self, session_id: &str) -> &[ViewerInfo] {
        self.viewers
            .get(session_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn has_viewers(&self, session_id: &str) -> bool {
        self.viewers.get(session_id).is_some_and(|v| !v.is_empty())
    }

    /// Re-read both files and rebuild. Missing/unreadable files simply mean
    /// {t("presence.no_viewers")}. Returns display names of genuinely new arrivals for
    /// toasting (the initial population is seeded silently).
    ///
    /// On `wasm32` there is no filesystem: the file-backed feeds stay
    /// empty and only explicitly injected viewers appear.
    pub fn refresh(&mut self, now_ms: u64) -> Vec<String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.file_viewers = std::fs::read(&self.presence_path)
                .ok()
                .map(|data| parse_presence(&data, "terminal"))
                .unwrap_or_default();
            self.mobile_file_viewers = std::fs::read(&self.mobile_presence_path)
                .ok()
                .map(|data| parse_presence(&data, "direct-link"))
                .unwrap_or_default();
        }
        let merged = merge_presence(
            &[
                (&self.file_viewers, FILE_ENTRY_TTL_MS),
                (&self.mobile_file_viewers, MOBILE_ENTRY_TTL_MS),
            ],
            now_ms,
        );
        // Latch candidacy on sight is a pane-grid concern the Dioxus
        // launchers don't have (no shared-PTY resize arbitration); the
        // merged viewer map is the whole contract here.
        let mut live: HashMap<&str, &str> = HashMap::new();
        for viewers in merged.values() {
            for v in viewers {
                live.insert(v.id.as_str(), v.display_name.as_str());
            }
        }
        let live_ids: HashSet<&str> = live.keys().copied().collect();
        let mut arrivals = Vec::new();
        if !self.seeded {
            self.seeded = true;
        } else {
            let mut new_ids: Vec<&str> = live_ids
                .iter()
                .filter(|id| !self.announced_ids.contains(**id))
                .copied()
                .collect();
            new_ids.sort_unstable();
            for id in new_ids {
                arrivals.push(live[id].to_string());
            }
        }
        self.announced_ids = live_ids.into_iter().map(str::to_string).collect();
        self.viewers = merged;
        arrivals
    }
}

/// Presence chips for a session row/header — port of `ViewerAvatarsView`.
///
/// One header surface for device observation: a chip per viewer showing
/// the device initial, with the full names in the tooltip.
#[component]
pub fn ViewerAvatars(viewers: Vec<ViewerInfo>) -> Element {
    if viewers.is_empty() {
        return rsx! {};
    }
    let names: String = viewers
        .iter()
        .map(|v| v.display_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    rsx! {
        div { class: "viewer-avatars", title: "{names} watching",
            for v in viewers {
                span { key: "{v.id}", class: "viewer-avatar",
                    "{v.display_name.chars().next().unwrap_or('?')}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(device: &str, last_seen: i64) -> Vec<u8> {
        serde_json::json!({
            "version": 1,
            "sessions": { "s1": [{ "ip": "10.0.0.2", "device": device, "last_seen": last_seen }] }
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn parses_device_name_and_id() {
        let parsed = parse_presence(&entry("Osman's iPhone (abc123)", 1_000_000), "terminal");
        let v = &parsed["s1"][0];
        assert_eq!(v.id, "device:abc123");
        assert_eq!(v.device_id.as_deref(), Some("abc123"));
        assert_eq!(v.display_name, "Osman's iPhone");
    }

    #[test]
    fn ip_only_viewer_gets_legacy_id() {
        let data = serde_json::json!({
            "sessions": { "s1": [{ "ip": "10.0.0.9", "last_seen": 1_000_000 }] }
        })
        .to_string()
        .into_bytes();
        let parsed = parse_presence(&data, "terminal");
        let v = &parsed["s1"][0];
        assert_eq!(v.id, "legacy:terminal:10.0.0.9");
        assert_eq!(v.device_id, None);
        assert_eq!(v.display_name, "10.0.0.9");
    }

    #[test]
    fn stale_entries_expire_per_feed() {
        let now = 10_000_000u64;
        let fresh = parse_presence(&entry("Phone (a1)", (now - 5_000) as i64), "terminal");
        let stale = parse_presence(&entry("Phone (a1)", (now - 60_000) as i64), "terminal");
        let merged = merge_presence(&[(&fresh, FILE_ENTRY_TTL_MS)], now);
        assert!(merged["s1"].iter().any(|v| v.id == "device:a1"));
        let merged = merge_presence(&[(&stale, FILE_ENTRY_TTL_MS)], now);
        assert!(!merged.contains_key("s1"));
    }

    #[test]
    fn stale_feed_does_not_hide_live_feed() {
        let now = 10_000_000u64;
        let stale_file = parse_presence(&entry("Phone (a1)", (now - 60_000) as i64), "terminal");
        let live_mobile = parse_presence(&entry("Phone (a1)", (now - 5_000) as i64), "direct-link");
        let merged = merge_presence(
            &[
                (&stale_file, FILE_ENTRY_TTL_MS),
                (&live_mobile, MOBILE_ENTRY_TTL_MS),
            ],
            now,
        );
        let v = &merged["s1"][0];
        assert_eq!(v.last_seen_ms, now - 5_000);
    }

    #[test]
    fn newest_wins_and_sorts_by_name() {
        let now = 10_000_000u64;
        let data = serde_json::json!({
            "sessions": { "s1": [
                { "device": "Zed (z)", "last_seen": now as i64 - 1000 },
                { "device": "Amy (a)", "last_seen": now as i64 - 2000 },
                { "device": "Amy (a)", "last_seen": now as i64 - 9000 },
            ]}
        })
        .to_string()
        .into_bytes();
        let feed = parse_presence(&data, "terminal");
        let merged = merge_presence(&[(&feed, FILE_ENTRY_TTL_MS)], now);
        let viewers = &merged["s1"];
        assert_eq!(viewers.len(), 2);
        assert_eq!(viewers[0].display_name, "Amy");
        assert_eq!(viewers[0].last_seen_ms, now - 2000);
        assert_eq!(viewers[1].display_name, "Zed");
    }

    #[test]
    fn refresh_announces_only_new_arrivals() {
        let dir = std::env::temp_dir().join(format!("presence-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("remote")).unwrap();
        let (p, m) = presence_file_paths(&dir);
        std::fs::write(&p, entry("Phone (a1)", 10_000_000)).unwrap();
        let mut store = PresenceStore::new(p.clone(), m.clone());
        // Initial population is seeded silently.
        assert!(store.refresh(10_005_000).is_empty());
        assert!(store.has_viewers("s1"));
        // A new device arrives → announced once.
        std::fs::write(
            &p,
            serde_json::json!({
                "sessions": { "s1": [
                    { "device": "Phone (a1)", "last_seen": 10_006_000 },
                    { "device": "Mac (m2)", "last_seen": 10_006_000 },
                ]}
            })
            .to_string(),
        )
        .unwrap();
        let arrivals = store.refresh(10_007_000);
        assert_eq!(arrivals, vec!["Mac".to_string()]);
        // Same population again → silent.
        assert!(store.refresh(10_008_000).is_empty());
        // Missing file → no viewers, no panic.
        std::fs::remove_file(&p).unwrap();
        assert!(store.refresh(10_009_000).is_empty());
        assert!(!store.has_viewers("s1"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn display_name_falls_back_to_ip_then_remote() {
        assert_eq!(display_name_from_device(None, Some("1.2.3.4")), "1.2.3.4");
        assert_eq!(display_name_from_device(None, None), "Remote viewer");
        assert_eq!(
            display_name_from_device(Some("JustAName"), None),
            "JustAName"
        );
    }
}
