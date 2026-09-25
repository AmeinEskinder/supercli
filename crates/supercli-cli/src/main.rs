//! `unpeel` — the command-line client of the shared Unpeel contracts.
//!
//! One binary, three roles: `unpeel serve` runs the UI-free Host service
//! (`unpeel-serve`), the one-shot verbs script sessions, presets, projects,
//! workspaces, settings, pairing, and Unpeel Link against the same on-disk
//! contract the Mac app and phone use, and `--workspace NAME` re-homes any of
//! them into an isolated workspace. There is no interactive terminal UI:
//! bare `unpeel` prints usage. The Controllers are the Unpeel app, the phone,
//! and the web Controller — all clients of `unpeel serve`.

mod apps_cli;
mod backup_cli;
mod browser_cli;
mod cli;
mod computer_cli;
mod config_cli;
mod connectors_cli;
mod doctor_cli;
mod init_cli;
mod integrations_cli;
mod link_cli;
mod mcp_cli;
mod migrate_cli;
mod open_cli;
mod schedule_cli;
mod self_update_cli;
mod settings_cli;
mod state_cli;
mod workspaces;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--workspace` re-homes the whole process (UNPEEL_HOME), so it must be
    // claimed before any dispatch touches state — spawned hosts inherit it.
    match workspaces::claim_workspace_flag(&mut args) {
        Ok(Some(reference)) => {
            if let Err(error) = workspaces::enter(&reference) {
                eprintln!("unpeel: {error}");
                std::process::exit(2);
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("unpeel: {error}");
            std::process::exit(2);
        }
    }
    // `--log-level LEVEL` (R4): set the JSON log level for this process.
    // Claimed early, before any log output. Takes precedence over
    // UNPEEL_LOG_LEVEL env var.
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--log-level" && i + 1 < args.len() {
            let level_str = args[i + 1].clone();
            match unpeel_core::json_log::Level::parse(&level_str) {
                Some(level) => {
                    unpeel_core::json_log::set_level(level);
                }
                None => {
                    eprintln!(
                        "unpeel: invalid --log-level {level_str:?} (expected DEBUG, INFO, WARN, ERROR)"
                    );
                    std::process::exit(2);
                }
            }
            args.remove(i);
            args.remove(i);
        } else {
            i += 1;
        }
    }
    // The machine service and each workspace worker re-exec this executable
    // with these internal argv modes; they are not user-facing commands.
    if args.as_slice() == [unpeel_serve::service::SERVICE_ARG] {
        let result = unpeel_serve::service::run(|event| {
            println!("{event}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
        });
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.as_slice() == [unpeel_serve::service::WORKSPACE_WORKER_ARG] {
        let result = unpeel_serve::service::run_workspace_worker(|event| {
            println!("{event}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
        });
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    let code = cli::run(&args);
    // One-shot verbs exit immediately; wait for their change pings to
    // actually reach the other frontends first.
    unpeel_core::state_bus::flush();
    std::process::exit(code);
}
