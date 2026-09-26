//! `supercli device` — headless Android/iOS device control, setup verbs.
//!
//! Only the setup verbs are wired here; the backend surface
//! (list/boot/tap/stream/…) arrives with the native scrcpy client + web
//! panel work. Setup is wired first because developers and CI need it to
//! provision devices.
//!
//! Exit codes: 0 success · 1 tool failure · 2 missing tool / bad usage /
//! declined-or-non-interactive download (mirrors the platform-gating
//! convention in docs/device.md §1).

use std::io::{self, IsTerminal, Write};

use supercli_device::setup::{self, RealRunner, SetupError};

pub const HELP: &str = "\
supercli device — headless Android/iOS device control

  supercli device setup android [--yes] [--dry-run]
      install the Android SDK pieces (platform-tools, emulator, one system
      image) via sdkmanager and create the `supercli` AVD with avdmanager.
      The downloads go through an install prompt unless --yes is given.
      --dry-run prints the plan without running anything.

  supercli device setup ios
      check for baguette on PATH and verify it runs; prints
      `brew install baguette` and exits 2 when it is missing.

Backend verbs (list/boot/tap/stream/describe-ui/…) require the `device`
cargo feature and arrive with the native scrcpy client + web panel work.";

/// `args` are the raw words after `device`.
pub fn run(args: &[String]) -> i32 {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["setup", "android", flags @ ..] => setup_android(flags),
        ["setup", "ios"] => setup_ios(),
        ["setup", other, ..] => {
            eprintln!("unknown device setup target {other:?} (expected `android` or `ios`)");
            2
        }
        ["setup"] | [] => {
            println!("{HELP}");
            0
        }
        [other, ..] => {
            eprintln!("unknown device subcommand {other:?}\n{HELP}");
            2
        }
    }
}

fn setup_android(flags: &[&str]) -> i32 {
    let mut yes = false;
    let mut dry_run = false;
    for flag in flags {
        match *flag {
            "--yes" => yes = true,
            "--dry-run" => dry_run = true,
            other => {
                eprintln!("unknown flag {other:?} for `supercli device setup android`");
                return 2;
            }
        }
    }

    let plan = setup::android_plan();
    if dry_run {
        println!(
            "sdkmanager --install {}",
            plan.sdkmanager_packages.join(" ")
        );
        println!(
            "avdmanager create avd -n {} -k {} --device {}",
            plan.avd_name, plan.system_image, plan.device_profile
        );
        return 0;
    }

    if let Err(error) = setup::check_sdkmanager() {
        eprintln!("supercli device: {error}");
        return 2;
    }

    // Downloads are network fetches: approval-gated like `device install`
    // (docs/device.md §9.5), same prompt shape as apps install confirm.
    if !yes && !confirm_downloads(&plan) {
        return 2;
    }

    let mut progress = |s: &str| println!("supercli device: {s}");
    match setup::setup_android(&RealRunner, &mut progress) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("supercli device: setup android failed: {error}");
            1
        }
    }
}

fn setup_ios() -> i32 {
    match setup::setup_ios() {
        Ok(version) => {
            println!("baguette {version}");
            0
        }
        Err(SetupError::BaguetteMissing) => {
            // Exact string (docs/device.md §9.5).
            println!("brew install baguette");
            2
        }
        Err(error) => {
            eprintln!("supercli device: setup ios failed: {error}");
            1
        }
    }
}

/// Approval prompt for the SDK downloads. Non-interactive stdin refuses
/// rather than hanging (same rule as app installs).
fn confirm_downloads(plan: &supercli_device::setup::AndroidSetupPlan) -> bool {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        eprintln!(
            "supercli device: refusing to download Android SDK packages non-interactively. \
             Re-run in a terminal, or pass --yes from user-owned automation."
        );
        return false;
    }
    eprint!(
        "Download and install Android SDK packages ({})? [y/N] ",
        plan.sdkmanager_packages.join(", ")
    );
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}
