//! `unpeel init` — first-run setup for a fresh machine.
//!
//! 1. Creates the workspace home with mode 0700 (via
//!    `ensure_unpeel_home`, which also repairs a wrong mode).
//! 2. Seeds a blank home (builtin presets) and verifies the resulting
//!    config is valid under the typed schema.
//! 3. Ensures the Host is running and shows a pairing code + QR so a
//!    Controller can pair. It does not block waiting for the pair;
//!    `unpeel pair` does the blocking wait.
//! 4. Ends by running `unpeel doctor`; init's exit code is doctor's.

use unpeel_core::{app_paths, app_state, config, first_run};

pub const INIT_HELP: &str = "\
unpeel init — first-run setup

  unpeel init [--json]

Creates the workspace home (mode 0700), seeds defaults, shows a pairing
code/QR for a Controller, then runs `unpeel doctor`. Safe to re-run: it
never overwrites an existing home's presets, projects, or settings.\
";

fn begin_pairing_code() -> Result<String, String> {
    crate::cli::ensure_host_running()?;
    let home = app_paths::unpeel_home();
    unpeel_serve::local_gateway::begin_pairing(&home, None, None)
}

pub fn run(args: &[String], json: bool) -> i32 {
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("{INIT_HELP}");
        return 0;
    }
    if args.iter().any(|a| a != "--json" && a != "json") {
        eprintln!("unpeel init: unexpected argument\n{INIT_HELP}");
        return 2;
    }

    // 1. Home with mode 0700.
    let home = match app_paths::ensure_unpeel_home() {
        Ok(home) => home,
        Err(err) => {
            eprintln!("unpeel init: cannot create workspace home: {err}");
            return 1;
        }
    };

    // 2. Seed a blank home; never touch an existing one.
    let seeded = match app_state::load_for_edit() {
        Ok(state) if first_run::needs_seeding(&state) => match first_run::seed_app_state(&[]) {
            Ok((presets, _)) => {
                if !json {
                    println!(
                        "seeded {} builtin preset(s) into {}",
                        presets.len(),
                        home.display()
                    );
                }
                true
            }
            Err(err) => {
                eprintln!("unpeel init: seeding failed: {err}");
                return 1;
            }
        },
        Ok(_) => {
            if !json {
                println!("home already initialized at {}", home.display());
            }
            false
        }
        Err(err) => {
            eprintln!("unpeel init: cannot read app-state.json: {err}");
            return 1;
        }
    };

    // The seeded defaults must validate under the typed schema.
    match app_state::load() {
        Ok(doc) => {
            let report = config::check_document(&doc);
            if !report.is_valid() {
                eprintln!(
                    "unpeel init: seeded config failed validation:\n{}",
                    report.message()
                );
                return 1;
            }
            if !json {
                for warning in &report.warnings {
                    println!("warning: unknown key {warning}");
                }
                println!("default config valid");
            }
        }
        Err(err) => {
            eprintln!("unpeel init: cannot load app-state.json: {err}");
            return 1;
        }
    }

    // 3. Pairing code + QR (non-blocking).
    let pairing_code = match begin_pairing_code() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("unpeel init: pairing setup failed: {err}");
            return 1;
        }
    };
    if !json {
        for line in unpeel_serve::pairing::qr_lines(&pairing_code) {
            println!("{line}");
        }
        println!("\n{pairing_code}\n");
        println!("paste or scan in an Unpeel Controller — expires in 5 minutes");
        println!("(waiting for a pair is `unpeel pair`; init continues without blocking)");
        println!();
    }

    // 4. End with doctor; its exit code is ours. In JSON mode the doctor
    // report merges into init's single JSON object.
    let (doctor_home, checks) = crate::doctor_cli::run_checks();
    let passed = checks.iter().filter(|(_, ok, _)| *ok).count();
    let failed = checks.len() - passed;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": failed == 0,
                "home": home.to_string_lossy(),
                "seeded": seeded,
                "pairing_code": pairing_code,
                "doctor": {
                    "home": doctor_home.to_string_lossy(),
                    "passed": passed,
                    "failed": failed,
                    "checks": checks.iter().map(|(name, ok, detail)| {
                        serde_json::json!({"name": name, "ok": ok, "detail": detail})
                    }).collect::<Vec<_>>(),
                },
            })
        );
    } else {
        println!("unpeel doctor — home: {}", doctor_home.display());
        for (name, ok, detail) in &checks {
            let status = if *ok { "OK  " } else { "FAIL" };
            println!("  [{status}] {name}: {detail}");
        }
        println!("{passed} passed, {failed} failed");
        if failed > 0 {
            eprintln!("{failed} check(s) failed");
        }
    }
    if failed > 0 {
        1
    } else {
        0
    }
}
