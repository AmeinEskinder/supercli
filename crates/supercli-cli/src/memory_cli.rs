//! `supercli memory` — persistent agent memory with scopes.
//!
//! Two scopes: `session` (working memory, dropped on restart) and
//! `longterm` (promoted facts that survive restarts, in `<home>/memory.json`).
//! Promotion is explicit: nothing is promoted automatically.

use std::path::PathBuf;

use supercli_core::{app_paths, memory};

pub const HELP: &str = "\
supercli memory — scoped agent memory

  supercli memory set <key> <value...> [--session <id>] [--longterm]
      store a fact (default scope: session, requires --session <id>;
      --longterm stores it as a long-term fact directly)
  supercli memory get <key>
      print a fact's value
  supercli memory promote <key>
      promote a session fact to long-term (explicit; nothing auto-promotes)
  supercli memory forget <key>
      forget a fact entirely
  supercli memory list [--session <id>] [--json]
      list keys visible to a session (long-term + that session's own)

Memory lives in <home>/memory.json (SUPERCLI_HOME-aware). Session-scoped
facts are never written to disk.
";

fn load() -> (PathBuf, memory::MemoryStore) {
    let home = app_paths::supercli_home();
    let store = memory::load_memory(&home);
    (home, store)
}

pub fn run(args: &[String]) -> i32 {
    let result: Result<i32, String> = match args.first().map(String::as_str) {
        Some("set") => {
            let key = match args.get(1) {
                Some(k) => k.clone(),
                None => return fail("usage: supercli memory set <key> <value...>"),
            };
            let value_parts: Vec<&str> = args[2..]
                .iter()
                .filter(|a| !a.starts_with("--"))
                .map(String::as_str)
                .collect();
            if value_parts.is_empty() {
                return fail(
                    "usage: supercli memory set <key> <value...> [--session <id>] [--longterm]",
                );
            }
            let value = value_parts.join(" ");
            let longterm = args.iter().any(|a| a == "--longterm");
            let session_id = args
                .iter()
                .position(|a| a == "--session")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);
            if !longterm && session_id.is_none() {
                return fail("session scope requires --session <id> (or pass --longterm)");
            }
            let (home, mut store) = load();
            let scope = if longterm {
                memory::Scope::LongTerm
            } else {
                memory::Scope::Session
            };
            store.set(&key, &value, scope, session_id);
            memory::save_memory(&home, &store);
            println!("{key}");
            Ok(0)
        }
        Some("get") => {
            let key = match args.get(1) {
                Some(k) => k.clone(),
                None => return fail("usage: supercli memory get <key>"),
            };
            let (_, store) = load();
            match store.get(&key) {
                Some(v) => {
                    println!("{v}");
                    Ok(0)
                }
                None => Err(format!("no memory {key:?}")),
            }
        }
        Some("promote") => {
            let key = match args.get(1) {
                Some(k) => k.clone(),
                None => return fail("usage: supercli memory promote <key>"),
            };
            let (home, mut store) = load();
            if !store.promote(&key) {
                return fail(&format!("no memory {key:?}"));
            }
            memory::save_memory(&home, &store);
            println!("{key} promoted to long-term");
            Ok(0)
        }
        Some("forget") => {
            let key = match args.get(1) {
                Some(k) => k.clone(),
                None => return fail("usage: supercli memory forget <key>"),
            };
            let (home, mut store) = load();
            if !store.forget(&key) {
                return fail(&format!("no memory {key:?}"));
            }
            memory::save_memory(&home, &store);
            println!("{key} forgotten");
            Ok(0)
        }
        Some("list") => {
            let session_id = args
                .iter()
                .position(|a| a == "--session")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);
            let json = args.iter().any(|a| a == "--json");
            let (_, store) = load();
            let keys: Vec<String> = match session_id {
                Some(sid) => store.keys_for_session(sid),
                None => {
                    // No session: only long-term facts are visible.
                    store.keys_for_session("")
                }
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&keys).unwrap_or_else(|_| "[]".into())
                );
            } else if keys.is_empty() {
                println!("no memory facts");
            } else {
                for k in &keys {
                    let scope = store
                        .scope_of(k)
                        .map(|s| match s {
                            memory::Scope::Session => "session",
                            memory::Scope::LongTerm => "longterm",
                        })
                        .unwrap_or("?");
                    println!("[{scope}] {k}");
                }
            }
            Ok(0)
        }
        Some("--help" | "-h" | "help") | None => {
            println!("{HELP}");
            Ok(0)
        }
        Some(other) => Err(format!("unknown memory subcommand {other:?}\n{HELP}")),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("supercli memory: {message}");
            1
        }
    }
}

fn fail(msg: &str) -> i32 {
    eprintln!("supercli memory: {msg}");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_cli_help_mentions_scopes() {
        assert!(HELP.contains("longterm"));
        assert!(HELP.contains("promote"));
    }
}
