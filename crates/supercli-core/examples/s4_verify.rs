//! S4 verification: checks after a kill -9.
//!
//! Usage: s4_verify <supercli_home>
//!
//! Checks:
//! 1. Audit chain verifies (no fork, no tamper)
//! 2. Reconciliation passes (grants ⊆ chain)
//! 3. Grant lock is acquirable (no stuck lock)
//! 4. Grants file is valid JSON

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: s4_verify <supercli_home>");
        std::process::exit(2);
    }
    std::env::set_var("SUPERCLI_HOME", &args[1]);

    // 1. Audit chain verifies.
    match supercli_core::grant_audit::verify_grant_audit() {
        Ok(count) => println!("audit chain verifies: {count} entries"),
        Err(e) => {
            eprintln!("FAIL: audit chain verify: {e}");
            std::process::exit(1);
        }
    }

    // 2. Reconciliation passes.
    match supercli_core::grant_audit::reconcile_grants() {
        Ok(()) => println!("reconciliation passes"),
        Err(e) => {
            eprintln!("FAIL: reconciliation: {e}");
            std::process::exit(1);
        }
    }

    // 3. Doctor subset check.
    match supercli_core::grant_audit::doctor_check_grants_subset() {
        Ok(()) => println!("doctor grants ⊆ chain passes"),
        Err(e) => {
            eprintln!("FAIL: doctor subset: {e}");
            std::process::exit(1);
        }
    }

    // 4. Grant lock acquirable (no stuck lock).
    // Use a short timeout via try-lock semantics: edit_grants with no-op.
    match supercli_core::grant_store::edit_grants(|_| Ok::<(), String>(())) {
        Ok(()) => println!("grant lock acquirable"),
        Err(e) => {
            eprintln!("FAIL: grant lock stuck: {e}");
            std::process::exit(1);
        }
    }

    // 5. Grants file is valid JSON.
    let grants = supercli_core::grant_store::load_grants_for_reconcile();
    println!("grants file valid: {} top-level keys", grants.len());

    println!("s4_verify: ALL CHECKS PASS");
}
