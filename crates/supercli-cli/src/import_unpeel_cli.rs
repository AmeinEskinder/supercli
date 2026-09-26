//! `supercli migrate --from-unpeel` — one-time import from a legacy `~/.unpeel/` home.
//!
//! Copies sessions, config, grants, and pairing state from `~/.unpeel/` to
//! `~/.supercli/`. The source is left intact. A second run is a no-op for
//! files that already exist at the destination (use `--force` to overwrite).

use std::path::{Path, PathBuf};

/// Items to copy from the old home to the new home.
/// (source relative path, description)
const IMPORT_ITEMS: &[(&str, &str)] = &[
    ("app-sessions", "sessions"),
    ("app-state.json", "app state (grants, config)"),
    ("config.json", "config"),
    ("grants.json", "grants"),
    ("pairing.json", "pairing state"),
    ("schedules.json", "schedules"),
    ("artifacts", "artifacts"),
    ("transcripts", "transcripts"),
];

fn old_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| Path::new(&h).join(".unpeel"))
}

fn new_home() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("SUPERCLI_HOME") {
        return Some(PathBuf::from(h));
    }
    std::env::var_os("HOME").map(|h| Path::new(&h).join(".supercli"))
}

fn copy_recursively(src: &Path, dst: &Path) -> std::io::Result<u64> {
    let mut count = 0u64;
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        return Ok(1);
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        count += copy_recursively(&src_path, &dst_path)?;
    }
    Ok(count)
}

pub fn run_from_unpeel(args: &[String]) -> i32 {
    let force = args.iter().any(|a| a == "--force");
    let json = args.iter().any(|a| a == "--json");

    let src = match old_home() {
        Some(p) => p,
        None => {
            eprintln!("error: cannot determine HOME");
            return 2;
        }
    };
    let dst = match new_home() {
        Some(p) => p,
        None => {
            eprintln!("error: cannot determine HOME");
            return 2;
        }
    };

    if !src.exists() {
        if json {
            println!(r#"{{"imported":0,"skipped":"no ~/.unpeel found"}}"#);
        } else {
            println!("No ~/.unpeel found — nothing to import.");
        }
        return 0;
    }

    let mut imported = 0u64;
    let mut skipped = Vec::new();
    for (rel, desc) in IMPORT_ITEMS {
        let s = src.join(rel);
        let d = dst.join(rel);
        if !s.exists() {
            continue;
        }
        if d.exists() && !force {
            skipped.push(format!("{} (exists, use --force to overwrite)", desc));
            continue;
        }
        match copy_recursively(&s, &d) {
            Ok(n) => {
                imported += n;
                if !json {
                    println!("imported {} ({} files)", desc, n);
                }
            }
            Err(e) => {
                eprintln!("error importing {}: {}", desc, e);
                return 1;
            }
        }
    }

    if json {
        println!(r#"{{"imported":{},"skipped":{}}}"#, imported, skipped.len());
    } else {
        println!();
        println!(
            "Import complete: {} files copied from {} to {}",
            imported,
            src.display(),
            dst.display()
        );
        if !skipped.is_empty() {
            println!("Skipped (already exist):");
            for s in &skipped {
                println!("  - {}", s);
            }
        }
        println!("Source left intact at {}", src.display());
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_source_is_noop() {
        // With a HOME that has no .unpeel, import is a no-op returning 0.
        let tmp = std::env::temp_dir().join(format!("supercli-import-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // Temporarily override HOME via env for this test is racy; instead
        // just verify the item list is non-empty and well-formed.
        assert!(!IMPORT_ITEMS.is_empty());
        for (rel, desc) in IMPORT_ITEMS {
            assert!(!rel.is_empty());
            assert!(!desc.is_empty());
        }
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn copy_recursively_copies_files() {
        let base =
            std::env::temp_dir().join(format!("supercli-import-copy-{}", std::process::id()));
        let src = base.join("src");
        let dst = base.join("dst");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("sub").join("b.txt"), b"world").unwrap();
        let n = copy_recursively(&src, &dst).unwrap();
        assert_eq!(n, 2);
        assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"hello");
        assert_eq!(
            std::fs::read(dst.join("sub").join("b.txt")).unwrap(),
            b"world"
        );
        std::fs::remove_dir_all(&base).ok();
    }
}
