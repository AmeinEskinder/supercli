//! Client-side viewer presence: which devices are viewing session terminals.
//!
//! Moved from the Dart client (`clients/supercli-app/lib/viewer_presence.dart`,
//! branch `feat/port-viewer-presence`) per Amein's rule: one implementation, in
//! Rust. The Dart client keeps only UI bindings via `supercli-client-ffi`.
//!
//! Port of `ViewerPresence.swift`'s synchronous decision kernel. Two feeds
//! converge here, both written by the Rust Host (`supercli-serve/src/presence.rs`):
//! - File feed: `~/.supercli/remote/presence.json` — remote viewers.
//! - Mobile feed: `mobile-presence.json` beside it — authenticated Direct/Link
//!   output leases.
//!
//! This module ports the pure logic: presence-file parsing, per-feed TTL
//! expiry (20s file / 15s mobile — the same TTLs the Host publishes with),
//! cross-feed merge with newest-wins, and display-name sort order. File
//! watching, timers, and toast callbacks are platform runtime machinery and
//! stay on the caller side.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

/// File-feed staleness cutoff. The remote server prunes poll viewers after
/// 15s; anything older than this on disk is a leftover from a dead server.
pub const FILE_ENTRY_TTL_MS: u64 = 20_000;
/// Host Direct/Link output lease TTL.
pub const MOBILE_ENTRY_TTL_MS: u64 = 15_000;

/// One device currently viewing a session's terminal.
///
/// Presence is device-level observation, not human membership or a terminal
/// control lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerInfo {
    /// Stable viewer key: `device:<id>` for authenticated Controllers,
    /// `legacy:<source>:<identity>` for IP-only/legacy viewers.
    pub id: String,
    /// Stable paired-device id for authenticated Controllers.
    pub device_id: Option<String>,
    pub display_name: String,
    /// Milliseconds since epoch (UTC).
    pub last_seen_ms: u64,
}

#[derive(Debug, Deserialize)]
struct PresenceFile {
    #[serde(default)]
    sessions: HashMap<String, Vec<PresenceEntry>>,
}

#[derive(Debug, Deserialize)]
struct PresenceEntry {
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    last_seen: u64,
}

/// Parse one presence feed file. Malformed input yields no viewers (callers
/// treat it as "no remote viewers", never as an error).
pub fn parse_presence(data: &[u8], source: &str) -> HashMap<String, Vec<ViewerInfo>> {
    let Ok(file) = serde_json::from_slice::<PresenceFile>(data) else {
        return HashMap::new();
    };
    let mut result = HashMap::new();
    for (session_id, entries) in file.sessions {
        let mut list = Vec::new();
        for raw in entries {
            let device_id = raw.device.as_deref().and_then(|d| device_id_from_device(Some(d)));
            let identity = raw
                .device
                .as_deref()
                .or(raw.ip.as_deref())
                .unwrap_or("remote");
            // Keep duplicates until the timestamp-aware merge. The first
            // connection in the file may be older than another live one.
            let id = match &device_id {
                Some(d) => format!("device:{d}"),
                None => format!("legacy:{source}:{identity}"),
            };
            list.push(ViewerInfo {
                id,
                device_id: device_id.map(str::to_string),
                display_name: display_name_from_device(
                    raw.device.as_deref(),
                    raw.ip.as_deref(),
                ),
                last_seen_ms: raw.last_seen,
            });
        }
        if !list.is_empty() {
            result.insert(session_id, list);
        }
    }
    result
}

/// The remote server records `device` as "Name (id)"; show just the name.
pub fn display_name_from_device(device: Option<&str>, ip: Option<&str>) -> String {
    if let Some(device) = device {
        if !device.is_empty() {
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
    }
    ip.unwrap_or("Remote viewer").to_string()
}

/// The remote server records authenticated Controllers as "Name (id)".
/// An IP-only/legacy viewer has no stable device identity.
pub fn device_id_from_device(device: Option<&str>) -> Option<&str> {
    let device = device?;
    if !device.ends_with(')') {
        return None;
    }
    let open = device.rfind(" (")?;
    let start = open + 2;
    let end = device.len() - 1;
    if start >= end {
        return None;
    }
    let id = &device[start..end];
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Merge the two feeds into session id → current viewers.
///
/// Each feed is expired by its own TTL *before* merging: a stale lease in one
/// transport must not hide the same device's live lease in the other.
/// Newest-wins on duplicate viewer ids; output is sorted by display name
/// (case-insensitive) then id, mirroring the Swift sort.
pub fn merge_presence(
    file_viewers: &HashMap<String, Vec<ViewerInfo>>,
    mobile_viewers: &HashMap<String, Vec<ViewerInfo>>,
    now_ms: u64,
) -> BTreeMap<String, Vec<ViewerInfo>> {
    let mut by_session: HashMap<String, HashMap<String, ViewerInfo>> = HashMap::new();
    for (feed_viewers, ttl_ms) in [
        (file_viewers, FILE_ENTRY_TTL_MS),
        (mobile_viewers, MOBILE_ENTRY_TTL_MS),
    ] {
        for (session_id, viewers) in feed_viewers {
            for viewer in viewers {
                if now_ms.saturating_sub(viewer.last_seen_ms) > ttl_ms {
                    continue;
                }
                let session = by_session.entry(session_id.clone()).or_default();
                let replace = session
                    .get(&viewer.id)
                    .is_none_or(|prev| viewer.last_seen_ms > prev.last_seen_ms);
                if replace {
                    session.insert(viewer.id.clone(), viewer.clone());
                }
            }
        }
    }
    by_session
        .into_iter()
        .map(|(session_id, viewers)| {
            let mut list: Vec<ViewerInfo> = viewers.into_values().collect();
            list.sort_by(|a, b| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
                    .then_with(|| a.id.cmp(&b.id))
            });
            (session_id, list)
        })
        .collect()
}

/// Whether one exact paired Controller is currently rendering this session.
pub fn is_device_viewing(
    merged: &BTreeMap<String, Vec<ViewerInfo>>,
    session_id: &str,
    device_id: &str,
) -> bool {
    merged
        .get(session_id)
        .is_some_and(|viewers| viewers.iter().any(|v| v.device_id.as_deref() == Some(device_id)))
}

/// Computes newly-arrived viewer ids since `announced` (for one-shot
/// connection toasts). The first call seeds silently.
pub fn connection_arrivals(
    merged: &BTreeMap<String, Vec<ViewerInfo>>,
    announced: &mut std::collections::HashSet<String>,
    seeded: &mut bool,
) -> Vec<(String, String)> {
    let live: HashMap<&str, &str> = merged
        .values()
        .flatten()
        .map(|v| (v.id.as_str(), v.display_name.as_str()))
        .collect();
    if !*seeded {
        announced.clear();
        announced.extend(live.keys().map(|s| s.to_string()));
        *seeded = true;
        return Vec::new();
    }
    let mut arrivals: Vec<(String, String)> = live
        .iter()
        .filter(|(id, _)| !announced.contains(*id as &str))
        .map(|(id, name)| (id.to_string(), name.to_string()))
        .collect();
    arrivals.sort();
    announced.clear();
    announced.extend(live.keys().map(|s| s.to_string()));
    arrivals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presence_json() -> &'static [u8] {
        br#"{"version":1,"updated_at":100000,"sessions":{"s1":[{"device":"Alex (phone-1)","last_seen":99000},{"ip":"10.0.0.2","last_seen":99001},{"device":"Stale (old-9)","last_seen":1000}]}}"#
    }

    #[test]
    fn parse_presence_extracts_device_ids_and_names() {
        let parsed = parse_presence(presence_json(), "terminal");
        let viewers = &parsed["s1"];
        assert_eq!(viewers.len(), 3);
        let alex = viewers.iter().find(|v| v.device_id.as_deref() == Some("phone-1")).unwrap();
        assert_eq!(alex.id, "device:phone-1");
        assert_eq!(alex.display_name, "Alex");
        let legacy = viewers.iter().find(|v| v.device_id.is_none()).unwrap();
        assert!(legacy.id.starts_with("legacy:terminal:"));
        assert_eq!(legacy.display_name, "10.0.0.2");
        // Malformed -> empty
        assert!(parse_presence(b"nope", "terminal").is_empty());
        assert!(parse_presence(b"[]", "terminal").is_empty());
    }

    #[test]
    fn display_name_parsing() {
        assert_eq!(display_name_from_device(Some("Alex (phone-1)"), None), "Alex");
        assert_eq!(display_name_from_device(Some("NoParens"), None), "NoParens");
        assert_eq!(display_name_from_device(Some(""), Some("1.2.3.4")), "1.2.3.4");
        assert_eq!(display_name_from_device(None, None), "Remote viewer");
        // "Name ()" with empty name falls back to the full string
        assert_eq!(display_name_from_device(Some(" ()"), None), " ()");
    }

    #[test]
    fn device_id_parsing() {
        assert_eq!(device_id_from_device(Some("Alex (phone-1)")), Some("phone-1"));
        assert_eq!(device_id_from_device(Some("NoParens")), None);
        assert_eq!(device_id_from_device(Some("Bad ()")), None);
        assert_eq!(device_id_from_device(None), None);
        // Nested parens: the last parenthesised segment wins.
        assert_eq!(device_id_from_device(Some("Mac (a) (b)")), Some("b"));
    }

    #[test]
    fn merge_expires_per_feed_and_newest_wins() {
        let file = parse_presence(presence_json(), "terminal");
        // Mobile feed: same device with a NEWER lease
        let mobile_json = br#"{"version":1,"sessions":{"s1":[{"device":"Alex (phone-1)","last_seen":99500}]}}"#;
        let mobile = parse_presence(mobile_json, "direct-link");
        // now = 100000: file entries at 99000/99001 live (20s TTL), stale at 1000 expired
        let merged = merge_presence(&file, &mobile, 100_000);
        let viewers = &merged["s1"];
        // Stale file entry gone; Alex newest-wins from mobile feed
        assert_eq!(viewers.len(), 2);
        let alex = viewers.iter().find(|v| v.device_id.as_deref() == Some("phone-1")).unwrap();
        assert_eq!(alex.last_seen_ms, 99_500);
        // Sorted by display name: "10.0.0.2" < "alex" case-insensitively
        assert_eq!(viewers[0].display_name, "10.0.0.2");
    }

    #[test]
    fn merge_applies_mobile_ttl() {
        let mobile_json = br#"{"version":1,"sessions":{"s1":[{"device":"M (m-1)","last_seen":80000}]}}"#;
        let mobile = parse_presence(mobile_json, "direct-link");
        // 20s after last_seen: mobile 15s TTL expired, file feed empty
        let merged = merge_presence(&HashMap::new(), &mobile, 100_000);
        assert!(merged.get("s1").is_none_or(|v| v.is_empty()));
    }

    #[test]
    fn is_device_viewing_checks_exact_device() {
        let file = parse_presence(presence_json(), "terminal");
        let merged = merge_presence(&file, &HashMap::new(), 100_000);
        assert!(is_device_viewing(&merged, "s1", "phone-1"));
        assert!(!is_device_viewing(&merged, "s1", "other"));
        assert!(!is_device_viewing(&merged, "nope", "phone-1"));
    }

    #[test]
    fn connection_arrivals_seed_silently_then_announce() {
        let file = parse_presence(presence_json(), "terminal");
        let merged = merge_presence(&file, &HashMap::new(), 100_000);
        let mut announced = std::collections::HashSet::new();
        let mut seeded = false;
        // First call seeds silently
        assert!(connection_arrivals(&merged, &mut announced, &mut seeded).is_empty());
        assert!(seeded);
        // Same set: no arrivals
        assert!(connection_arrivals(&merged, &mut announced, &mut seeded).is_empty());
        // New device arrives
        let mut merged2 = merged.clone();
        merged2.get_mut("s1").unwrap().push(ViewerInfo {
            id: "device:new-1".into(),
            device_id: Some("new-1".into()),
            display_name: "New".into(),
            last_seen_ms: 100_000,
        });
        let arrivals = connection_arrivals(&merged2, &mut announced, &mut seeded);
        assert_eq!(arrivals.len(), 1);
        assert_eq!(arrivals[0].0, "device:new-1");
    }
}
