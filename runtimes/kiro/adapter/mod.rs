use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/kiro/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/kiro/adapter/setup.rs"
    ));
}

/// Kiro configs installed by older Unpeel builds invoke this argv directly.
/// The shared Host asks every compiled adapter for compatibility aliases, so
/// the provider spelling remains here instead of becoming a central case.
fn legacy_mcp_gate_kind(argument: &str) -> Option<&'static str> {
    (argument == "__kiro_mcp__").then_some(crate::mcp_gate::UNIFIED_KIND)
}

/// Those older configs also expand Kiro-specific grant aliases into their MCP
/// subprocess. Match the historical truthy forms while the shared gate keeps
/// enforcing a valid hosted Session identity.
fn legacy_mcp_gate_granted(kind: &str) -> bool {
    legacy_mcp_gate_granted_with(kind, |name| std::env::var(name).ok())
}

fn legacy_mcp_gate_granted_with(kind: &str, read_env: impl FnOnce(&str) -> Option<String>) -> bool {
    let name = match kind {
        crate::mcp_gate::SESSIONS_KIND => "UNPEEL_KIRO_SESSIONS_MCP_ENABLED",
        crate::mcp_gate::BROWSER_KIND => "UNPEEL_KIRO_BROWSER_MCP_ENABLED",
        _ => return false,
    };
    read_env(name).is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )
    })
}

pub(crate) const INTEGRATION: Integration = Integration::new(Some(setup::install_kiro_hooks))
    .with_resume_adapter(resume::ADAPTER)
    .with_legacy_mcp_gate_kind(legacy_mcp_gate_kind)
    .with_legacy_mcp_gate_grant(legacy_mcp_gate_granted);

#[cfg(test)]
mod tests {
    use super::{legacy_mcp_gate_granted_with, legacy_mcp_gate_kind};

    #[test]
    fn legacy_argv_and_env_aliases_stay_recognized() {
        assert_eq!(
            legacy_mcp_gate_kind("__kiro_mcp__"),
            Some(crate::mcp_gate::UNIFIED_KIND)
        );
        assert_eq!(legacy_mcp_gate_kind("__unknown__"), None);
        assert!(legacy_mcp_gate_granted_with(
            crate::mcp_gate::SESSIONS_KIND,
            |name| (name == "UNPEEL_KIRO_SESSIONS_MCP_ENABLED").then(|| "true".to_string())
        ));
        assert!(legacy_mcp_gate_granted_with(
            crate::mcp_gate::BROWSER_KIND,
            |name| (name == "UNPEEL_KIRO_BROWSER_MCP_ENABLED").then(|| "1".to_string())
        ));
        assert!(!legacy_mcp_gate_granted_with(
            crate::mcp_gate::SESSIONS_KIND,
            |_| Some("0".to_string())
        ));
    }
}
