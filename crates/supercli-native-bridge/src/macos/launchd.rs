//! Port of `HostServiceAgent.swift` — launchd service management.
//!
//! Starts the bundled Host service through launchd instead of forking it.
//! A launchd job is a child of launchd with its own coalition, so the
//! service chain survives any termination of the app.
//!
//! The plist rendering here mirrors the app-specific shape (app label, no
//! KeepAlive, AssociatedBundleIdentifiers). The Host's own
//! `supercli-serve/src/service_install.rs` implements the CLI's parallel
//! `render_unit`/`install` for `supercli serve install`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Release label for the app's LaunchAgent.
pub const RELEASE_LABEL: &str = "com.supercli.native.serve";
/// Dev builds share the release bundle id but get their own label.
pub const DEVELOPMENT_LABEL: &str = "com.supercli.native.dev.serve";
/// Bundle identifier for AssociatedBundleIdentifiers.
pub const BUNDLE_IDENTIFIER: &str = "com.supercli.native";
/// `SUPERCLI_NATIVE_SERVICE_LAUNCHER=direct` restores the fork for diagnostics.
pub const LAUNCHER_OVERRIDE_ENV_VAR: &str = "SUPERCLI_NATIVE_SERVICE_LAUNCHER";

/// Result of running launchctl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub status: i32,
    pub output: String,
}

/// Outcome of ensuring the service is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The job is loaded and was asked to run. `rewrote` says the plist
    /// on disk changed (first install, moved bundle, or a new argv).
    Running { label: String, rewrote: bool },
    /// launchd refused; the caller falls back to forking the service.
    Failed(String),
}

/// Returns the label for the given build type.
pub fn label(development_build: bool) -> &'static str {
    if development_build {
        DEVELOPMENT_LABEL
    } else {
        RELEASE_LABEL
    }
}

/// `SUPERCLI_NATIVE_SERVICE_LAUNCHER=direct` (case-insensitive, trimmed)
/// opts into the direct fork for diagnostics.
pub fn uses_direct_launch(environment: &std::collections::HashMap<String, String>) -> bool {
    environment
        .get(LAUNCHER_OVERRIDE_ENV_VAR)
        .map(|v| v.trim().to_lowercase() == "direct")
        .unwrap_or(false)
}

/// `~/Library/LaunchAgents` for the real user home.
pub fn launch_agents_directory(user_home: &Path) -> PathBuf {
    user_home.join("Library").join("LaunchAgents")
}

/// Path to the plist file for a label.
pub fn plist_url(label: &str, agents_directory: &Path) -> PathBuf {
    agents_directory.join(format!("{label}.plist"))
}

/// Render the launchd plist. Deterministic so an unchanged install is a
/// byte-equal file and never re-bootstrapped.
///
/// Mirrors `HostServiceAgent.renderPlist`: Label, ProgramArguments
/// `[hostBinary, "__serve__"]`, RunAtLoad=true, KeepAlive=false (the
/// machine lease makes a second service exit at once; KeepAlive would
/// have launchd respawn that loser forever), ProcessType=Interactive,
/// AssociatedBundleIdentifiers=[bundleIdentifier].
pub fn render_plist(label: &str, host_binary: &str) -> Result<Vec<u8>, String> {
    let mut plist = plist::Dictionary::new();
    plist.insert("Label".into(), plist::Value::String(label.to_string()));
    plist.insert(
        "ProgramArguments".into(),
        plist::Value::Array(vec![
            plist::Value::String(host_binary.to_string()),
            plist::Value::String("__serve__".to_string()),
        ]),
    );
    plist.insert("RunAtLoad".into(), plist::Value::Boolean(true));
    // No KeepAlive: see module docs. `ensure_running` kickstarts.
    plist.insert("KeepAlive".into(), plist::Value::Boolean(false));
    plist.insert(
        "ProcessType".into(),
        plist::Value::String("Interactive".to_string()),
    );
    plist.insert(
        "AssociatedBundleIdentifiers".into(),
        plist::Value::Array(vec![plist::Value::String(BUNDLE_IDENTIFIER.to_string())]),
    );

    let mut buf = Vec::new();
    plist::to_writer_xml(&mut buf, &plist).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Install (or refresh) the unit and make sure launchd runs it.
///
/// Pure apart from the injected `launchctl` and the plist write, so tests
/// drive it with a temp directory and a fake tool.
pub fn ensure_running(
    label: &str,
    host_binary: &str,
    agents_directory: &Path,
    uid: u32,
    launchctl: &dyn Fn(&[String]) -> CommandResult,
) -> Outcome {
    let plist_path = plist_url(label, agents_directory);
    let desired = match render_plist(label, host_binary) {
        Ok(d) => d,
        Err(e) => {
            return Outcome::Failed(format!(
                "could not render {}: {e}",
                plist_path.file_name().unwrap_or_default().to_string_lossy()
            ))
        }
    };

    let existing = std::fs::read(&plist_path).ok();
    let rewrote = existing.as_deref() != Some(desired.as_slice());
    if rewrote {
        if let Some(parent) = plist_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Outcome::Failed(format!("could not write {}: {e}", plist_path.display()));
            }
        }
        // Atomic write: temp file + rename.
        let tmp = plist_path.with_extension("tmp");
        if std::fs::write(&tmp, &desired).is_err() || std::fs::rename(&tmp, &plist_path).is_err() {
            return Outcome::Failed(format!("could not write {}", plist_path.display()));
        }
    }

    let domain = format!("gui/{uid}");
    let target = format!("{domain}/{label}");
    if rewrote {
        // A loaded job keeps the argv it was bootstrapped with; unload it
        // so the rewritten file takes effect. Fails harmlessly when it
        // was never loaded.
        launchctl(&["bootout".to_string(), target.clone()]);
    }
    let bootstrap = launchctl(&[
        "bootstrap".to_string(),
        domain.clone(),
        plist_path.to_string_lossy().to_string(),
    ]);
    if bootstrap.status != 0 {
        // Already loaded is the common case on every launch after the
        // first; anything else must show up in `print`.
        let loaded = launchctl(&["print".to_string(), target.clone()]);
        if loaded.status != 0 {
            return Outcome::Failed(format!(
                "launchctl bootstrap {target} failed ({}): {}",
                bootstrap.status,
                bootstrap.output.trim()
            ));
        }
    }
    let kickstart = launchctl(&["kickstart".to_string(), target.clone()]);
    if kickstart.status != 0 {
        return Outcome::Failed(format!(
            "launchctl kickstart {target} failed ({}): {}",
            kickstart.status,
            kickstart.output.trim()
        ));
    }
    Outcome::Running {
        label: label.to_string(),
        rewrote,
    }
}

/// The real launchctl tool. Synchronous; every call returns in milliseconds.
pub fn run_launchctl(args: &[String]) -> CommandResult {
    let output = Command::new("/bin/launchctl").args(args).output();
    match output {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&o.stderr));
            CommandResult {
                status: o.status.code().unwrap_or(-1),
                output: text,
            }
        }
        Err(e) => CommandResult {
            status: -1,
            output: e.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Records every launchctl call and answers from a script keyed by the
    /// subcommand; unscripted calls succeed.
    struct FakeLaunchctl {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
        answers: HashMap<String, CommandResult>,
    }

    impl FakeLaunchctl {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                answers: HashMap::new(),
            }
        }

        fn run(&self, args: &[String]) -> CommandResult {
            self.calls.lock().unwrap().push(args.to_vec());
            let key = args.first().cloned().unwrap_or_default();
            self.answers.get(&key).cloned().unwrap_or(CommandResult {
                status: 0,
                output: String::new(),
            })
        }

        fn subcommands(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter_map(|c| c.first().cloned())
                .collect()
        }

        fn clear_calls(&self) {
            self.calls.lock().unwrap().clear();
        }
    }

    fn temp_agents_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let agents = dir.path().join("LaunchAgents");
        (dir, agents)
    }

    fn ensure(tool: &FakeLaunchctl, agents: &Path, host_binary: &str, label: &str) -> Outcome {
        ensure_running(label, host_binary, agents, 501, &|args| tool.run(args))
    }

    #[test]
    fn labels_keep_dev_builds_off_the_release_unit() {
        assert_eq!(label(false), "com.supercli.native.serve");
        assert_eq!(label(true), "com.supercli.native.dev.serve");
        assert_ne!(
            RELEASE_LABEL, "com.supercli.serve",
            "the CLI's unit label is reserved"
        );
    }

    #[test]
    fn direct_launch_override_is_explicit_opt_in() {
        assert!(!uses_direct_launch(&HashMap::new()));
        let mut env = HashMap::new();
        env.insert(LAUNCHER_OVERRIDE_ENV_VAR.to_string(), "launchd".to_string());
        assert!(!uses_direct_launch(&env));
        env.insert(
            LAUNCHER_OVERRIDE_ENV_VAR.to_string(),
            " Direct\n".to_string(),
        );
        assert!(uses_direct_launch(&env));
    }

    #[test]
    fn rendered_unit_runs_the_machine_service_without_keep_alive() {
        let data = render_plist(
            "com.supercli.native.serve",
            "/Applications/Supercli.app/Contents/MacOS/supercli-host",
        )
        .unwrap();
        let plist = plist::from_bytes::<plist::Dictionary>(&data).unwrap();
        assert_eq!(
            plist.get("Label").and_then(|v| v.as_string()),
            Some("com.supercli.native.serve")
        );
        let args: Vec<String> = plist
            .get("ProgramArguments")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|v| v.as_string().map(|s| s.to_string()))
            .collect();
        assert_eq!(
            args,
            vec![
                "/Applications/Supercli.app/Contents/MacOS/supercli-host".to_string(),
                "__serve__".to_string()
            ]
        );
        assert_eq!(
            plist.get("RunAtLoad").and_then(|v| v.as_boolean()),
            Some(true)
        );
        // The machine lease makes a second service exit at once; KeepAlive
        // would have launchd respawn that loser forever.
        assert_eq!(
            plist.get("KeepAlive").and_then(|v| v.as_boolean()),
            Some(false)
        );
        assert_eq!(
            plist.get("ProcessType").and_then(|v| v.as_string()),
            Some("Interactive")
        );
        let bundles: Vec<String> = plist
            .get("AssociatedBundleIdentifiers")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|v| v.as_string().map(|s| s.to_string()))
            .collect();
        assert_eq!(bundles, vec!["com.supercli.native".to_string()]);
        assert!(
            plist.get("EnvironmentVariables").is_none(),
            "the machine service resolves its own homes"
        );
    }

    #[test]
    fn first_run_writes_the_unit_then_bootstraps_and_kickstarts() {
        let (_dir, agents) = temp_agents_dir();
        let tool = FakeLaunchctl::new();
        assert_eq!(
            ensure(
                &tool,
                &agents,
                "/Applications/Supercli.app/Contents/MacOS/supercli-host",
                RELEASE_LABEL
            ),
            Outcome::Running {
                label: RELEASE_LABEL.to_string(),
                rewrote: true
            }
        );
        let plist = plist_url(RELEASE_LABEL, &agents);
        assert!(plist.exists());
        let calls = tool.calls.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec![
                vec![
                    "bootout".to_string(),
                    "gui/501/com.supercli.native.serve".to_string()
                ],
                vec![
                    "bootstrap".to_string(),
                    "gui/501".to_string(),
                    plist.to_string_lossy().to_string()
                ],
                vec![
                    "kickstart".to_string(),
                    "gui/501/com.supercli.native.serve".to_string()
                ],
            ]
        );
    }

    #[test]
    fn unchanged_unit_is_never_reloaded() {
        let (_dir, agents) = temp_agents_dir();
        let mut tool = FakeLaunchctl::new();
        let _ = ensure(
            &tool,
            &agents,
            "/Applications/Supercli.app/Contents/MacOS/supercli-host",
            RELEASE_LABEL,
        );
        tool.clear_calls();
        // Every launch after the first: the job is already loaded, so
        // bootstrap reports an error that `print` disproves.
        tool.answers.insert(
            "bootstrap".to_string(),
            CommandResult {
                status: 5,
                output: "Bootstrap failed: 5: Input/output error".to_string(),
            },
        );
        assert_eq!(
            ensure(
                &tool,
                &agents,
                "/Applications/Supercli.app/Contents/MacOS/supercli-host",
                RELEASE_LABEL
            ),
            Outcome::Running {
                label: RELEASE_LABEL.to_string(),
                rewrote: false
            }
        );
        assert_eq!(tool.subcommands(), vec!["bootstrap", "print", "kickstart"]);
    }

    #[test]
    fn moved_bundle_rewrites_and_reloads_the_unit() {
        let (_dir, agents) = temp_agents_dir();
        let tool = FakeLaunchctl::new();
        let _ = ensure(
            &tool,
            &agents,
            "/Applications/Supercli.app/Contents/MacOS/supercli-host",
            RELEASE_LABEL,
        );
        tool.clear_calls();
        assert_eq!(
            ensure(
                &tool,
                &agents,
                "/Users/me/Applications/Supercli.app/Contents/MacOS/supercli-host",
                RELEASE_LABEL
            ),
            Outcome::Running {
                label: RELEASE_LABEL.to_string(),
                rewrote: true
            }
        );
        assert_eq!(
            tool.subcommands(),
            vec!["bootout", "bootstrap", "kickstart"]
        );
    }

    #[test]
    fn bootstrap_failure_without_a_loaded_job_falls_back() {
        let (_dir, agents) = temp_agents_dir();
        let mut tool = FakeLaunchctl::new();
        tool.answers.insert(
            "bootstrap".to_string(),
            CommandResult {
                status: 37,
                output: "Bootstrap failed: 37: Operation already in progress".to_string(),
            },
        );
        tool.answers.insert(
            "print".to_string(),
            CommandResult {
                status: 113,
                output: "Could not find service".to_string(),
            },
        );
        match ensure(
            &tool,
            &agents,
            "/Applications/Supercli.app/Contents/MacOS/supercli-host",
            RELEASE_LABEL,
        ) {
            Outcome::Failed(reason) => {
                assert!(reason.contains("bootstrap"), "{reason}");
            }
            other => {
                panic!("expected a failure so the app forks the service instead, got {other:?}")
            }
        }
        assert!(!tool.subcommands().contains(&"kickstart".to_string()));
    }

    #[test]
    fn kickstart_failure_falls_back() {
        let (_dir, agents) = temp_agents_dir();
        let mut tool = FakeLaunchctl::new();
        tool.answers.insert(
            "kickstart".to_string(),
            CommandResult {
                status: 1,
                output: "Could not kickstart service".to_string(),
            },
        );
        match ensure(
            &tool,
            &agents,
            "/Applications/Supercli.app/Contents/MacOS/supercli-host",
            RELEASE_LABEL,
        ) {
            Outcome::Failed(reason) => {
                assert!(reason.contains("kickstart"), "{reason}");
            }
            other => {
                panic!("expected a failure so the app forks the service instead, got {other:?}")
            }
        }
    }

    #[test]
    fn development_label_writes_its_own_file() {
        let (_dir, agents) = temp_agents_dir();
        let tool = FakeLaunchctl::new();
        assert_eq!(
            ensure(
                &tool,
                &agents,
                "/Applications/Supercli.app/Contents/MacOS/supercli-host",
                DEVELOPMENT_LABEL
            ),
            Outcome::Running {
                label: DEVELOPMENT_LABEL.to_string(),
                rewrote: true
            }
        );
        assert!(agents.join("com.supercli.native.dev.serve.plist").exists());
        assert!(!agents.join("com.supercli.native.serve.plist").exists());
    }
}
