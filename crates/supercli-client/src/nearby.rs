//! Local-network host discovery catalog.
//!
//! Ported from `clients/legacy/native/SupercliNative/Sources/SupercliNative/NearbyHostBrowser.swift`.
//! Bonjour is only a hint: choosing a row never grants access, and the sealed
//! one-time pairing code still authenticates the Host identity and endpoint.
//!
//! Only the pure catalog logic lives here (candidate parsing, dedup, sort).
//! The actual `NWBrowser` is AppKit-specific and stays on the Swift side.

use std::collections::HashMap;

/// A nearby Host candidate discovered via Bonjour (`_supercli-remote._tcp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyHostCandidate {
    pub host_id: String,
    pub name: String,
}

/// Pure catalog logic for nearby Host candidates.
pub struct NearbyHostCatalog;

impl NearbyHostCatalog {
    /// Build a candidate from a Bonjour service advertisement.
    ///
    /// Requires the stable `macid` TXT record; falls back to "Supercli Host"
    /// for an empty service name.
    pub fn candidate(
        service_name: &str,
        txt: &HashMap<String, String>,
    ) -> Option<NearbyHostCandidate> {
        let host_id = txt
            .get("macid")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let name = service_name.trim().to_string();
        Some(NearbyHostCandidate {
            host_id,
            name: if name.is_empty() {
                "Supercli Host".to_string()
            } else {
                name
            },
        })
    }

    /// Merge candidates, deduplicating by host ID (first wins) and sorting
    /// by name for the picker. Optionally excludes one host ID
    /// (case-insensitive), e.g. this machine.
    pub fn merging(
        candidates: Vec<NearbyHostCandidate>,
        excluding_host_id: Option<&str>,
    ) -> Vec<NearbyHostCandidate> {
        let excluded = excluding_host_id
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        let mut by_id: HashMap<String, NearbyHostCandidate> = HashMap::new();
        for candidate in candidates {
            let key = candidate.host_id.to_lowercase();
            if let Some(ex) = &excluded {
                if &key == ex {
                    continue;
                }
            }
            by_id.entry(key).or_insert(candidate);
        }
        let mut out: Vec<NearbyHostCandidate> = by_id.into_values().collect();
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.host_id.cmp(&b.host_id))
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // Ported from NearbyHostBrowserTests.swift.

    #[test]
    fn advertisement_requires_stable_host_identity() {
        assert_eq!(NearbyHostCatalog::candidate("Studio", &txt(&[])), None);
        assert_eq!(
            NearbyHostCatalog::candidate("Studio", &txt(&[("macid", " ")])),
            None
        );
        assert_eq!(
            NearbyHostCatalog::candidate("  Studio Mac  ", &txt(&[("macid", "host-1")])),
            Some(NearbyHostCandidate {
                host_id: "host-1".to_string(),
                name: "Studio Mac".to_string(),
            })
        );
    }

    #[test]
    fn catalog_deduplicates_host_identity_and_sorts_for_picker() {
        let merged = NearbyHostCatalog::merging(
            vec![
                NearbyHostCandidate {
                    host_id: "b".to_string(),
                    name: "Zulu".to_string(),
                },
                NearbyHostCandidate {
                    host_id: "a".to_string(),
                    name: "Alpha".to_string(),
                },
                NearbyHostCandidate {
                    host_id: "a".to_string(),
                    name: "Spoofed duplicate".to_string(),
                },
            ],
            None,
        );
        assert_eq!(
            merged
                .iter()
                .map(|c| c.host_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(
            merged.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["Alpha", "Zulu"]
        );
    }

    #[test]
    fn catalog_excludes_only_this_logical_host() {
        let merged = NearbyHostCatalog::merging(
            vec![
                NearbyHostCandidate {
                    host_id: "CURRENT-HOST".to_string(),
                    name: "This Mac".to_string(),
                },
                NearbyHostCandidate {
                    host_id: "other-workspace".to_string(),
                    name: "Other Workspace".to_string(),
                },
                NearbyHostCandidate {
                    host_id: "remote-host".to_string(),
                    name: "Remote Mac".to_string(),
                },
            ],
            Some("current-host"),
        );
        assert_eq!(
            merged
                .iter()
                .map(|c| c.host_id.as_str())
                .collect::<Vec<_>>(),
            vec!["other-workspace", "remote-host"]
        );
    }

    #[test]
    fn empty_service_name_falls_back_to_default() {
        assert_eq!(
            NearbyHostCatalog::candidate("   ", &txt(&[("macid", "h1")])),
            Some(NearbyHostCandidate {
                host_id: "h1".to_string(),
                name: "Supercli Host".to_string(),
            })
        );
    }
}
