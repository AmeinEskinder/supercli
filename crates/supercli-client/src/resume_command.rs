//! Port of `ResumeCommand.swift` (native, Foundation-only).
//!
//! Provider-neutral transport for Host-owned relaunch planning.
//!
//! All command rewriting, provider identity selection, and verified
//! resume-failure markers live in supercli-core runtime adapters. Native
//! deliberately fails closed when the bundled Host cannot produce a plan.

use serde::Deserialize;

/// A Host-produced relaunch plan for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelaunchPlan {
    pub command: String,
    pub failure_markers: Vec<String>,
}

#[derive(Deserialize)]
struct RelaunchPlanJson {
    command: Option<String>,
    #[serde(default)]
    failure_markers: Vec<String>,
}

/// Decode a Host `__resume__` JSON response.
///
/// Kept separate from process execution so the additive Host response is
/// covered without launching a binary in unit tests. Older Hosts that return
/// only `command` remain readable; missing markers mean the UI simply does
/// not offer provider-specific failure recovery.
pub fn decode_relaunch_plan(data: &[u8]) -> Option<RelaunchPlan> {
    let object: RelaunchPlanJson = serde_json::from_slice(data).ok()?;
    let command = object.command?;
    if command.is_empty() {
        return None;
    }
    let failure_markers = object
        .failure_markers
        .into_iter()
        .filter(|m| !m.is_empty())
        .collect();
    Some(RelaunchPlan {
        command,
        failure_markers,
    })
}

/// Ask the bundled Host for the relaunch plan of `session_id`.
///
/// Fails closed (`None`) when the Host binary cannot be launched, exits
/// non-zero, or returns an undecodable plan. Not available on wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub fn host_relaunch_plan(session_id: &str, force_fresh: bool) -> Option<RelaunchPlan> {
    let mut args = vec!["__resume__".to_string(), session_id.to_string()];
    if force_fresh {
        args.push("--fresh".to_string());
    }
    let output = std::process::Command::new(crate::launch_config::host_binary())
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    decode_relaunch_plan(&output.stdout)
}

/// Ask the bundled Host for the managed-storage path of `session_id`.
///
/// Fails closed (`None`) on any launch/exit/parse failure. Not available on
/// wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub fn host_managed_storage_path(session_id: &str) -> Option<String> {
    let output = std::process::Command::new(crate::launch_config::host_binary())
        .args(["__managed_storage__", session_id])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout);
    let trimmed = path.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_full_plan() {
        let data = br#"{"command": "supercli resume abc", "failure_markers": ["m1", "m2"]}"#;
        assert_eq!(
            decode_relaunch_plan(data),
            Some(RelaunchPlan {
                command: "supercli resume abc".into(),
                failure_markers: vec!["m1".into(), "m2".into()],
            })
        );
    }

    #[test]
    fn decodes_legacy_command_only_plan() {
        let data = br#"{"command": "supercli resume abc"}"#;
        assert_eq!(
            decode_relaunch_plan(data),
            Some(RelaunchPlan {
                command: "supercli resume abc".into(),
                failure_markers: vec![],
            })
        );
    }

    #[test]
    fn empty_markers_filtered_out() {
        let data = br#"{"command": "x", "failure_markers": ["", "m1", ""]}"#;
        assert_eq!(
            decode_relaunch_plan(data).unwrap().failure_markers,
            vec!["m1"]
        );
    }

    #[test]
    fn missing_command_is_none() {
        assert_eq!(
            decode_relaunch_plan(br#"{"failure_markers": ["m1"]}"#),
            None
        );
    }

    #[test]
    fn empty_command_is_none() {
        assert_eq!(decode_relaunch_plan(br#"{"command": ""}"#), None);
    }

    #[test]
    fn invalid_json_is_none() {
        assert_eq!(decode_relaunch_plan(b"not json"), None);
        assert_eq!(decode_relaunch_plan(b""), None);
    }

    #[test]
    fn non_string_command_is_none() {
        assert_eq!(decode_relaunch_plan(br#"{"command": 42}"#), None);
    }
}
