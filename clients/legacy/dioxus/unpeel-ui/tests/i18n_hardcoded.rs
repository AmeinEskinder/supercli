//! Q6 — Static test: fail on hardcoded user-facing strings.
//!
//! Scans the `supercli-ui` source for string literals that look like
//! user-facing text in RSX. Any literal that is not routed through the
//! i18n catalog (`t("...")`) must be on the `ALLOWLIST` below — a
//! hardcoded string added without an allowlist entry fails the test.
//!
//! The allowlist is the migration backlog: strings that predate the i18n
//! scaffolding. New UI text must use `t()` from day one.

use std::collections::HashSet;
use std::path::PathBuf;

/// String literals that predate the i18n scaffolding. Each entry is
/// `filename:line:literal`. Do not add new entries for new UI text —
/// route it through `t()` instead.
const ALLOWLIST: &[&str] = &[
    // False positives: test heuristic flags doc comments and SVG paths.
    // These are not user-facing UI strings.
    "activity.rs:27:\"Using a web browser\"", // doc comment, not UI text
    "app_lock.rs:428:\"M8 10V7a4 4 0 0 1 8 0v3\"", // SVG path data, not UI text
    "banner.rs:6:\"Remote content\"",         // doc comment, not UI text
    "banner.rs:38:\"Remote content\"",        // doc comment, not UI text
    "command_palette.rs:4:\"jump to anything\"", // doc comment, not UI text
    "components.rs:35:\"dot ok\"",            // CSS class name, not UI text
    "components.rs:35:\"dot bad\"",           // CSS class name, not UI text
    // False positives: doc comments and test assertions, not user-facing UI.
    "find.rs:21:\"No results\"", // doc comment
    "find.rs:21:\"3 of 17\"",    // doc comment
    "notifier.rs:33:\"Mac notifies when the desktop isn't already showing the session\"", // doc comment
    "project_tree.rs:89:\"Move to ▸\"", // doc comment
    // Const contexts: AppFeature::new is const fn requiring &'static str, cannot use t().
    "settings.rs:68:\"Git worktrees\"", // const, not migratable
    "settings.rs:77:\"Sessions use\"",  // const, not migratable
    "settings.rs:88:\"Workspaces\"",    // const, not migratable
    "settings.rs:97:\"Browser use\"",   // const, not migratable
    "settings.rs:106:\"Remote workspaces\"", // const, not migratable
];

/// Heuristic: does this string literal look like user-facing text?
/// We flag literals with letters and spaces/punctuation that are longer
/// than a couple characters, excluding obvious technical strings.
fn looks_like_ui_text(literal: &str) -> bool {
    let inner = literal.trim_matches('"');
    if inner.len() < 3 {
        return false;
    }
    // Must contain at least one letter.
    if !inner.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    // Exclude technical strings: paths, URLs, IDs, class names, formats.
    let technical = inner.contains('/')
        || inner.contains('\\')
        || inner.contains('.')
        || inner.contains('_')
        || inner.contains('-')
        || inner.contains(':')
        || inner.contains('{')
        || inner.contains('%');
    if technical {
        return false;
    }
    // User-facing text usually has spaces or is a capitalized word.
    inner.contains(' ')
        || (inner
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
            && inner.chars().all(|c| c.is_alphabetic()))
}

fn find_ui_src() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("src");
    dir
}

/// Remove `#[cfg(test)] mod tests { ... }` blocks so test fixtures
/// don't count as hardcoded UI strings.
fn strip_test_modules(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].contains("#[cfg(test)]")
            && i + 1 < lines.len()
            && lines[i + 1].contains("mod tests")
        {
            // Skip to the matching closing brace.
            let mut depth = 0i32;
            let mut j = i + 1;
            while j < lines.len() {
                depth +=
                    lines[j].matches('{').count() as i32 - lines[j].matches('}').count() as i32;
                if depth == 0 && lines[j].contains('{') {
                    break;
                }
                j += 1;
            }
            j += 1;
            while j < lines.len() && depth > 0 {
                depth +=
                    lines[j].matches('{').count() as i32 - lines[j].matches('}').count() as i32;
                j += 1;
            }
            i = j;
        } else {
            out.push(lines[i]);
            i += 1;
        }
    }
    out.join("\n")
}

#[test]
fn no_hardcoded_ui_strings() {
    let src_dir = find_ui_src();
    let mut violations = Vec::new();
    let allowlist: HashSet<&str> = ALLOWLIST.iter().copied().collect();

    let mut files: Vec<PathBuf> = std::fs::read_dir(&src_dir)
        .expect("read src dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "rs").unwrap_or(false))
        .collect();
    files.sort();

    for path in files {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        // Skip the i18n module itself (it defines the catalog).
        if filename == "i18n.rs" {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("read file");
        let content = strip_test_modules(&content);
        for (idx, line) in content.lines().enumerate() {
            let lineno = idx + 1;
            // Skip lines that already use the i18n function.
            if line.contains("t(\"") || line.contains("t_with_locale") {
                continue;
            }
            // Find string literals on this line.
            let mut in_str = false;
            let mut start = 0;
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
                    if !in_str {
                        in_str = true;
                        start = i;
                    } else {
                        in_str = false;
                        let literal: String = chars[start..=i].iter().collect();
                        if looks_like_ui_text(&literal) {
                            let key = format!("{}:{}:{}", filename, lineno, literal);
                            if !allowlist.contains(key.as_str()) {
                                violations.push(key);
                            }
                        }
                    }
                }
                i += 1;
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Hardcoded user-facing strings found (route through i18n::t() or add to ALLOWLIST with justification):\n{}",
        violations.join("\n")
    );
}

#[test]
fn allowlist_entries_are_still_valid() {
    // Every allowlist entry must match an actual hardcoded string.
    // If the string was migrated to t(), remove it from the allowlist.
    let src_dir = find_ui_src();
    let mut found = HashSet::new();

    for path in std::fs::read_dir(&src_dir)
        .expect("read src dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "rs").unwrap_or(false))
    {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        if filename == "i18n.rs" {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("read file");
        let content = strip_test_modules(&content);
        for (idx, line) in content.lines().enumerate() {
            let lineno = idx + 1;
            if line.contains("t(\"") {
                continue;
            }
            // Simple check: does the line contain the literal?
            for entry in ALLOWLIST {
                let parts: Vec<&str> = entry.splitn(3, ':').collect();
                if parts.len() == 3
                    && parts[0] == filename
                    && parts[1] == lineno.to_string()
                    && line.contains(parts[2].trim_matches('"'))
                {
                    found.insert(*entry);
                }
            }
        }
    }

    let missing: Vec<&&str> = ALLOWLIST.iter().filter(|e| !found.contains(*e)).collect();
    assert!(
        missing.is_empty(),
        "Allowlist entries no longer match (string was migrated? remove from ALLOWLIST):\n{:?}",
        missing
    );
}
