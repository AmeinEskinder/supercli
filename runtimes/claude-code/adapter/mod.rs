use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/claude-code/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/claude-code/adapter/setup.rs"
    ));
}

pub(crate) const INTEGRATION: Integration = Integration::new(Some(setup::install_claude_hooks))
    // https://code.claude.com/docs/en/interactive-mode: Escape interrupts
    // a response/tool call, but the Stop hook does not fire on interrupts.
    .with_escape_cancellation()
    .with_resume_adapter(resume::ADAPTER);

#[cfg(test)]
mod tests {
    #[test]
    fn inventory_selects_native_install_or_plain_shell_update() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().to_path_buf()];
        let inventory = || {
            crate::plugins::agents_wire_in_dirs(&dirs)
                .as_array().unwrap().iter()
                .find(|row| row["id"] == "com.anthropic.claude-code")
                .unwrap().clone()
        };
        let missing = inventory();
        assert_eq!(missing["installed"], false);
        assert_eq!(missing["installCommand"], "curl -fsSL https://claude.ai/install.sh | bash");

        let executable = root.path().join("claude");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let installed = inventory();
        assert_eq!(installed["installed"], true);
        let update = installed["installCommand"].as_str().unwrap();
        // The updater must not resolve to a managed Claude runtime or run
        // npm over an existing native installation.
        assert!(crate::integrations::runtime_for_command(update).is_none());
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", update])
            .env("PATH", root.path())
            .env("UNPEEL_HOME", root.path().join("state"))
            .output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "update\n");
    }
}
