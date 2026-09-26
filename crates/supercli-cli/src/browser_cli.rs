//! `supercli browser` — the Host-owned Browser MCP engine, scripted.
//!
//! The only browser verb the CLI carries: it delegates every decision to
//! `supercli_core::browser_engine` (the pinned manifest, hash verification,
//! the locked install, the shared resolution order), so the headless Host,
//! the workspace worker's start-time install, and the MCP server can never
//! disagree about which engine is "the" engine.
//!
//! Exit codes: 0 engine ready · 1 failure (download/hash/unsupported) ·
//! 3 `--check` found the engine absent or stale (nothing was installed) ·
//! 4 engine present but no system Chrome/Chromium on this Host.

use std::path::PathBuf;

use supercli_core::browser_engine as engine;

pub const HELP: &str = "\
supercli browser — Host-owned Browser MCP engine (agent-browser)

  supercli browser install [--check] [--json]
      install (or confirm) the pinned engine under ~/.supercli/browser/bin
      after sha256 verification against protocol/browser-engine-v1.json.
      --check only reports: exit 0 ready, 3 missing/stale, 4 no browser.

  supercli browser takeover --list [--endpoint WS_URL] [--json]
      list CDP targets (tabs) on a running browser.

  supercli browser takeover <target-id> [--frames N] [--interval-ms MS]
      [--out DIR] [--endpoint WS_URL] [--json]
      attach to a tab over CDP and stream screenshots (default 25 frames at
      200ms = 5fps for 5s) into <home>/browser/takeover/<ts>/.

The engine drives a system Chrome/Chromium; Supercli never installs one.
Override the engine with SUPERCLI_AGENT_BROWSER_BIN=<path>.
Takeover talks to any CDP endpoint: pass --endpoint ws://host:port/path
(from chrome --remote-debugging-port's /json/version webSocketDebuggerUrl).";

/// `args` are the raw words after `browser` (flags parsed here so this verb
/// owns its own `--check` / `--json` without touching the shared parser).
pub fn run(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let check = args.iter().any(|a| a == "--check");
    match args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(String::as_str)
    {
        Some("install") => install(check, json),
        Some("takeover") => takeover(&args[1..]),
        Some("--help" | "-h" | "help") | None => {
            println!("{HELP}");
            0
        }
        Some(other) => {
            eprintln!("unknown browser action: {other}\n{HELP}");
            1
        }
    }
}

fn install(check_only: bool, json: bool) -> i32 {
    let home = supercli_core::app_paths::supercli_home();
    let pinned = engine::pinned();
    let path_dirs = supercli_core::setup::search_dirs();
    let (status, code) = if check_only {
        match engine::resolve(&home) {
            Ok(path) => (engine::Status::ready(path), 0),
            Err(error) => (
                engine::Status {
                    state: "missing".into(),
                    version: pinned.version.clone(),
                    path: None,
                    error: Some(error),
                },
                3,
            ),
        }
    } else {
        match engine::ensure_installed(&home) {
            Ok(path) => (engine::Status::ready(path), 0),
            Err(error) => (engine::Status::failed(error), 1),
        }
    };
    let browser = engine::system_browser(&path_dirs);
    let code = if code == 0 && browser.is_none() {
        4
    } else {
        code
    };
    report(&status, browser, &path_dirs, json);
    code
}

fn report(status: &engine::Status, browser: Option<PathBuf>, path_dirs: &[PathBuf], json: bool) {
    if json {
        let mut value = serde_json::to_value(status).unwrap_or_default();
        value["browser"] = match &browser {
            Some(path) => serde_json::json!({ "path": path }),
            None => {
                serde_json::json!({ "path": null, "error": engine::missing_browser_message(path_dirs) })
            }
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return;
    }
    println!(
        "engine:  agent-browser {} — {}",
        status.version, status.state
    );
    if let Some(path) = &status.path {
        println!("path:    {}", path.display());
    }
    if let Some(error) = &status.error {
        println!("error:   {error}");
    }
    match browser {
        Some(path) => println!("browser: {}", path.display()),
        None => println!("browser: {}", engine::missing_browser_message(path_dirs)),
    }
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    let mut iter = args.iter().peekable();
    while let Some(a) = iter.next() {
        if a == name {
            return iter.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

/// `supercli browser takeover ...`: attach to a CDP target and stream
/// screenshots. No browser is launched here — the endpoint must already be
/// live (agent-browser remote-cdp binding or chrome --remote-debugging-port).
fn takeover(args: &[String]) -> i32 {
    use std::time::Duration;
    use supercli_core::browser_takeover::{CdpClient, DEFAULT_CDP_PORT};

    let json = args.iter().any(|a| a == "--json");
    let endpoint = flag_value(args, "--endpoint")
        .unwrap_or_else(|| format!("ws://127.0.0.1:{DEFAULT_CDP_PORT}"));
    let positional: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .skip(1) // skip "takeover"
        .map(String::as_str)
        .collect();

    if args.iter().any(|a| a == "--list") {
        let mut client = match CdpClient::connect(&endpoint) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("takeover: {e}");
                return 1;
            }
        };
        return match client.list_targets() {
            Ok(targets) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &targets
                                .iter()
                                .map(|t| serde_json::json!({
                                    "target_id": t.target_id,
                                    "title": t.title,
                                    "url": t.url,
                                    "type": t.kind,
                                }))
                                .collect::<Vec<_>>()
                        )
                        .unwrap_or_default()
                    );
                } else if targets.is_empty() {
                    println!("no targets on {endpoint}");
                } else {
                    for t in &targets {
                        println!("{}  [{}] {}  {}", t.target_id, t.kind, t.title, t.url);
                    }
                }
                0
            }
            Err(e) => {
                eprintln!("takeover: {e}");
                1
            }
        };
    }

    let Some(target_id) = positional.first() else {
        eprintln!("usage: supercli browser takeover --list | <target-id> [--frames N] [--interval-ms MS] [--out DIR] [--endpoint WS_URL]");
        return 1;
    };
    let frames: usize = flag_value(args, "--frames")
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    let interval_ms: u64 = flag_value(args, "--interval-ms")
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let home = supercli_core::app_paths::supercli_home();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let out_dir = flag_value(args, "--out")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join("browser").join("takeover").join(ts.to_string()));
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("takeover: mkdir {}: {e}", out_dir.display());
        return 1;
    }

    let mut client = match CdpClient::connect(&endpoint) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("takeover: {e}");
            return 1;
        }
    };
    let shots = match client.takeover_stream(target_id, frames, Duration::from_millis(interval_ms))
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("takeover: {e}");
            return 1;
        }
    };
    client.close();

    let mut saved = 0usize;
    let mut bytes = 0usize;
    for (i, png) in shots.iter().enumerate() {
        let path = out_dir.join(format!("frame-{i:04}.png"));
        match std::fs::write(&path, png) {
            Ok(()) => {
                saved += 1;
                bytes += png.len();
            }
            Err(e) => eprintln!("takeover: write {}: {e}", path.display()),
        }
    }
    if json {
        println!(
            "{}",
            serde_json::json!({
                "target_id": target_id,
                "frames": saved,
                "bytes": bytes,
                "out_dir": out_dir,
            })
        );
    } else {
        println!(
            "takeover: {saved} frames, {bytes} bytes -> {}",
            out_dir.display()
        );
    }
    0
}
