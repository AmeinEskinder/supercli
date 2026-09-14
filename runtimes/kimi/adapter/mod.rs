use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/kimi/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/kimi/adapter/setup.rs"
    ));
}

/// Compatibility for Kimi MCP entries installed before grants moved to the
/// provider-neutral environment names. Keep these aliases runtime-local so
/// the shared gate does not learn provider variables.
fn legacy_mcp_gate_granted(kind: &str) -> bool {
    legacy_mcp_gate_granted_with(kind, |name| std::env::var(name).ok())
}

fn legacy_mcp_gate_granted_with(kind: &str, read_env: impl FnOnce(&str) -> Option<String>) -> bool {
    let name = match kind {
        crate::mcp_gate::SESSIONS_KIND => "UNPEEL_KIMI_SESSIONS_MCP_ENABLED",
        crate::mcp_gate::BROWSER_KIND => "UNPEEL_KIMI_BROWSER_MCP_ENABLED",
        crate::mcp_gate::COMPUTER_KIND => "UNPEEL_KIMI_COMPUTER_MCP_ENABLED",
        _ => return false,
    };
    read_env(name).as_deref() == Some("1")
}

pub(crate) const INTEGRATION: Integration = Integration::new(Some(setup::install_kimi_hooks))
    .with_resume_adapter(resume::ADAPTER)
    .with_legacy_mcp_gate_grant(legacy_mcp_gate_granted);

#[cfg(test)]
mod tests {
    use super::legacy_mcp_gate_granted_with;

    #[test]
    fn legacy_kimi_grant_aliases_stay_recognized() {
        assert!(legacy_mcp_gate_granted_with(
            crate::mcp_gate::SESSIONS_KIND,
            |name| (name == "UNPEEL_KIMI_SESSIONS_MCP_ENABLED").then(|| "1".to_string())
        ));
        assert!(!legacy_mcp_gate_granted_with(
            crate::mcp_gate::BROWSER_KIND,
            |_| Some("0".to_string())
        ));
    }
}
