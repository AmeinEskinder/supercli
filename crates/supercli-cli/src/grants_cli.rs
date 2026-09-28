//! User-facing grant management over the sharded grant store.
//!
//! Grants are remembered approvals (v0.9 grant shard): approving a write pair
//! once lets future identical requests skip the prompt. They live in
//! `grants.json` with a tamper-evident audit chain (`grant-audit.jsonl`).
//! There was no user-facing way to revoke a remembered grant —
//! `supercli grants revoke` closes that product and security gap.

use supercli_core::grant_store;

pub const HELP: &str = "\
usage: supercli grants list [--json]
       supercli grants revoke --caller <id> [--target <id>] [--kind KIND] [--json]

Inspect and revoke remembered grants (the approvals this Host remembers,
so repeat requests skip the prompt):

  list                       show every remembered grant as a canonical key
  revoke --caller C --target T [--kind write]
                             revoke the remembered write pair C -> T
  revoke --caller C --kind browser|computer
                             revoke a remembered browser/computer approval
  revoke --caller C --target APP --kind app-open
                             revoke a remembered app-open approval

Kinds: write (default), browser, computer, app-open. Revocation edits
grants.json through the real grant store; the audit chain keeps the
creation entry as history (audit entry without a grant = revoked).
Exit 0 on success; exit 1 with an error when the grant does not exist.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrantKind {
    Write,
    Browser,
    Computer,
    AppOpen,
}

impl GrantKind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "write" => Some(Self::Write),
            "browser" => Some(Self::Browser),
            "computer" => Some(Self::Computer),
            "app-open" => Some(Self::AppOpen),
            _ => None,
        }
    }

    /// The grants.json object key for `grant_store::grant_exists`.
    fn json_key(self) -> &'static str {
        match self {
            Self::Write => "mcp_write_approvals",
            Self::Browser => "browser_approvals",
            Self::Computer => "computer_approvals",
            Self::AppOpen => "mcp_app_open_approvals",
        }
    }

    /// Whether this kind addresses a (caller, target) pair.
    fn needs_target(self) -> bool {
        matches!(self, Self::Write | Self::AppOpen)
    }

    /// Canonical audit key prefix used by `grant_store::remove_grants`.
    fn key_prefix(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Browser => "browser",
            Self::Computer => "computer",
            Self::AppOpen => "app-open",
        }
    }
}

fn canonical_key(kind: GrantKind, caller: &str, target: Option<&str>) -> String {
    match target {
        Some(t) => format!("{}:{caller}:{t}", kind.key_prefix()),
        None => format!("{}:{caller}", kind.key_prefix()),
    }
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

fn cmd_list(json: bool) -> Result<(), String> {
    let root = grant_store::load_grants_for_reconcile();
    let mut keys: Vec<String> = grant_store::flatten_grant_keys(&root).into_iter().collect();
    keys.sort();
    if json {
        println!("{}", serde_json::json!({ "grants": keys }));
    } else if keys.is_empty() {
        println!("no remembered grants");
    } else {
        for k in &keys {
            println!("{k}");
        }
    }
    Ok(())
}

fn cmd_revoke(args: &[String], json: bool) -> Result<(), String> {
    let caller = flag_value(args, "--caller")
        .ok_or_else(|| "usage: supercli grants revoke --caller <id> [--target <id>] [--kind KIND]".to_string())?;
    let kind = match flag_value(args, "--kind") {
        None => GrantKind::Write,
        Some(k) => GrantKind::parse(&k)
            .ok_or_else(|| "unknown --kind (expected write|browser|computer|app-open)".to_string())?,
    };
    let target = flag_value(args, "--target");
    if kind.needs_target() && target.is_none() {
        return Err(format!(
            "--target is required for --kind {}",
            kind.key_prefix()
        ));
    }

    // Fail closed on typos: do not create or rewrite grants.json when there
    // is nothing to revoke (avoids flipping already_granted's migrated-state
    // precedence on pre-migration homes).
    if !grant_store::grant_exists(kind.json_key(), &caller, target.as_deref()) {
        return Err(format!(
            "no such grant: {}",
            canonical_key(kind, &caller, target.as_deref())
        ));
    }

    let key = canonical_key(kind, &caller, target.as_deref());
    grant_store::remove_grants(std::slice::from_ref(&key))?;
    if json {
        println!("{}", serde_json::json!({ "revoked": key }));
    } else {
        println!("revoked {key}");
    }
    Ok(())
}

pub fn run(args: &[String], json: bool) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => cmd_list(json),
        Some("revoke") => cmd_revoke(&args[1..], json),
        Some("help" | "--help" | "-h") | None => {
            println!("{HELP}");
            Ok(())
        }
        Some(other) => Err(format!("unknown grants subcommand '{other}'; see `supercli grants help`")),
    }
}
