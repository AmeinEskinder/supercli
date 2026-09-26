//! `supercli ideas` — a small capture surface for ideas.
//!
//! Ideas are stored as JSONL at `<home>/ideas/ideas.jsonl`, one object per
//! line: `{id, text, created_at, done, done_at}`. IDs are sequential
//! (`idea-0001`, …) so they stay short and human-referenceable.
//!
//! This is intentionally dumb storage: no database, no sync. The point is a
//! frictionless place to park an idea from the terminal; richer surfacing
//! (web panel, search) can read the same JSONL later.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use supercli_core::app_paths;

pub const HELP: &str = "\
supercli ideas — capture and track ideas

  supercli ideas add <text...>     capture an idea
  supercli ideas list [--all] [--json]
                                   list open ideas (--all includes done)
  supercli ideas done <id>          mark an idea done

Ideas live in <home>/ideas/ideas.jsonl (SUPERCLI_HOME-aware).
";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Idea {
    pub id: String,
    pub text: String,
    pub created_at: u64,
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<u64>,
}

pub fn ideas_dir_for(home: &Path) -> PathBuf {
    home.join("ideas")
}

pub fn ideas_file_for(home: &Path) -> PathBuf {
    ideas_dir_for(home).join("ideas.jsonl")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_ideas(file: &Path) -> Result<Vec<Idea>, String> {
    if !file.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;
    let mut ideas = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let idea: Idea = serde_json::from_str(line)
            .map_err(|e| format!("parse {} line {}: {e}", file.display(), n + 1))?;
        ideas.push(idea);
    }
    Ok(ideas)
}

fn write_ideas(file: &Path, ideas: &[Idea]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    let mut out = String::new();
    for idea in ideas {
        let line = serde_json::to_string(idea).map_err(|e| format!("encode idea: {e}"))?;
        out.push_str(&line);
        out.push('\n');
    }
    fs::write(file, out).map_err(|e| format!("write {}: {e}", file.display()))
}

/// Append one idea; returns the assigned id.
pub fn add_idea(home: &Path, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("idea text is empty".to_string());
    }
    let file = ideas_file_for(home);
    let mut ideas = read_ideas(&file)?;
    let id = format!("idea-{:04}", ideas.len() + 1);
    ideas.push(Idea {
        id: id.clone(),
        text: text.to_string(),
        created_at: now_secs(),
        done: false,
        done_at: None,
    });
    write_ideas(&file, &ideas)?;
    Ok(id)
}

pub fn list_ideas(home: &Path, include_done: bool) -> Result<Vec<Idea>, String> {
    let ideas = read_ideas(&ideas_file_for(home))?;
    Ok(ideas
        .into_iter()
        .filter(|i| include_done || !i.done)
        .collect())
}

pub fn mark_done(home: &Path, id: &str) -> Result<(), String> {
    let file = ideas_file_for(home);
    let mut ideas = read_ideas(&file)?;
    let Some(idea) = ideas.iter_mut().find(|i| i.id == id) else {
        return Err(format!("no idea {id:?}"));
    };
    idea.done = true;
    idea.done_at = Some(now_secs());
    write_ideas(&file, &ideas)
}

pub fn run(args: &[String]) -> i32 {
    let home = app_paths::supercli_home();
    let result: Result<i32, String> = match args.first().map(String::as_str) {
        Some("add") => {
            let text = args[1..].join(" ");
            match add_idea(&home, &text) {
                Ok(id) => {
                    println!("{id}");
                    Ok(0)
                }
                Err(e) => Err(e),
            }
        }
        Some("list") => {
            let include_done = args.iter().any(|a| a == "--all");
            let json = args.iter().any(|a| a == "--json");
            match list_ideas(&home, include_done) {
                Ok(ideas) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&ideas).unwrap_or_else(|_| "[]".into())
                        );
                    } else if ideas.is_empty() {
                        println!("no ideas yet — `supercli ideas add <text>` to capture one");
                    } else {
                        for idea in &ideas {
                            let mark = if idea.done { "x" } else { " " };
                            println!("[{}] {}  {}", mark, idea.id, idea.text);
                        }
                    }
                    Ok(0)
                }
                Err(e) => Err(e),
            }
        }
        Some("done") => match args.get(1) {
            Some(id) => match mark_done(&home, id) {
                Ok(()) => {
                    println!("{id} done");
                    Ok(0)
                }
                Err(e) => Err(e),
            },
            None => Err("usage: supercli ideas done <id>".to_string()),
        },
        Some("--help" | "-h" | "help") | None => {
            println!("{HELP}");
            Ok(0)
        }
        Some(other) => Err(format!("unknown ideas subcommand {other:?}\n{HELP}")),
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("supercli ideas: {message}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU64, Ordering};
    static TEST_HOME_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_home() -> PathBuf {
        let n = TEST_HOME_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "supercli-ideas-test-{}-{}-{}",
            std::process::id(),
            now_secs(),
            n
        ));
        fs::create_dir_all(&dir).expect("make temp dir");
        dir
    }

    #[test]
    fn ideas_add_list_done_roundtrip() {
        let home = tmp_home();
        let id1 = add_idea(&home, "ship the gpuidart port").unwrap();
        let id2 = add_idea(&home, "fix the flaky UDP test").unwrap();
        assert_eq!(id1, "idea-0001");
        assert_eq!(id2, "idea-0002");

        let open = list_ideas(&home, false).unwrap();
        assert_eq!(open.len(), 2);
        assert_eq!(open[0].text, "ship the gpuidart port");
        assert!(!open[0].done);

        mark_done(&home, &id1).unwrap();
        let open = list_ideas(&home, false).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, "idea-0002");

        let all = list_ideas(&home, true).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().find(|i| i.id == id1).unwrap().done);
    }

    #[test]
    fn ideas_rejects_empty_text_and_unknown_id() {
        let home = tmp_home();
        assert!(add_idea(&home, "   ").is_err());
        assert!(mark_done(&home, "idea-9999").is_err());
        assert!(list_ideas(&home, false).unwrap().is_empty());
    }

    #[test]
    fn ideas_persist_as_jsonl() {
        let home = tmp_home();
        add_idea(&home, "persistent?").unwrap();
        let file = ideas_file_for(&home);
        let text = fs::read_to_string(&file).unwrap();
        assert_eq!(text.lines().count(), 1);
        let v: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(v["id"], "idea-0001");
        assert_eq!(v["done"], false);
    }
}
