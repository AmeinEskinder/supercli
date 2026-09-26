//! Clickable terminal paths — port of `ClickablePath.swift`.
//!
//! Pulls a file-path token out of a clicked terminal row. The Dioxus
//! terminal (like Ghostty) matches URLs/OSC 8 links natively; bare paths
//! (e.g. `src/Home.tsx:42` printed by an agent) are detected here. Pure
//! string logic, no surface dependency.
//!
//! Swift behavior is the spec. Column arguments are zero-based character
//! offsets into the row — the launcher converts grid cells to string
//! offsets before calling, exactly like the Swift terminal wrapper.

use crate::i18n::t;

/// A path token found under a click, with an optional `:line[:col]` suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathMatch {
    pub path: String,
    pub line: Option<i64>,
    pub column: Option<i64>,
}

/// A modifier-click (Cmd/Ctrl) on a terminal row. The launcher matches
/// the row's text with [`matches_in_row`] and acts only when exactly one
/// path-like token is found — rows with zero or several paths never fire,
/// so clicks are never stolen by a guess.
#[derive(Debug, Clone, Copy)]
pub struct PathClickRequest {
    pub row: usize,
}

/// The pane's working directory for resolving relative paths. A retained
/// pane's live OSC 7 directory wins over the bootstrap seed; the Dioxus
/// terminal does not track OSC 7 yet, so both are usually `None` and
/// relative paths stay unresolvable — the same outcome as Swift with
/// neither seed nor report.
#[derive(Debug, Clone, Default)]
pub struct TerminalWorkingDirectory {
    pub seed: Option<String>,
    pub reported: Option<String>,
}

impl TerminalWorkingDirectory {
    pub fn current(&self) -> Option<&str> {
        match self.reported.as_deref() {
            Some(r) if !r.trim().is_empty() => Some(r),
            _ => self.seed.as_deref(),
        }
    }
}

/// One contiguous run of path characters and the char columns it spans.
struct Token {
    text: String,
    start: usize,
    end: usize,
}

/// Finds the path under a zero-based character offset in the row.
/// Never chooses a nearby file: that would steal clicks on URLs and plain
/// text.
pub fn match_in_row(row: &str, column: usize) -> Option<PathMatch> {
    let token = tokenize(row)
        .into_iter()
        .find(|t| column >= t.start && column <= t.end)?;
    if token.text.contains("://") {
        return None;
    }
    if token.text.to_lowercase().starts_with("mailto:") {
        return None;
    }
    let m = parse(&token.text);
    if looks_like_path(&m.path) {
        Some(m)
    } else {
        None
    }
}

/// Every path-like token in the row with its match. Used by launchers that
/// resolve the click column themselves (e.g. via JS measurement).
pub fn matches_in_row(row: &str) -> Vec<(usize, usize, PathMatch)> {
    tokenize(row)
        .into_iter()
        .filter(|t| !t.text.contains("://") && !t.text.to_lowercase().starts_with("mailto:"))
        .filter_map(|t| {
            let m = parse(&t.text);
            if looks_like_path(&m.path) {
                Some((t.start, t.end, m))
            } else {
                None
            }
        })
        .collect()
}

fn tokenize(row: &str) -> Vec<Token> {
    const BOUNDARIES: &str = "`\"'<>[](),;|";
    let chars: Vec<char> = row.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let char = chars[index];
        // Quoted paths can contain spaces, Unicode, brackets and parens.
        let is_quote = char == '`' || char == '"' || char == '\'';
        if is_quote {
            if let Some(rel) = chars[index + 1..].iter().position(|&c| c == char) {
                let end = index + 1 + rel;
                tokens.push(Token {
                    text: chars[index + 1..end].iter().collect(),
                    start: index + 1,
                    end: end.saturating_sub(1),
                });
                index = end + 1;
                continue;
            }
        }
        if char.is_whitespace() || BOUNDARIES.contains(char) {
            index += 1;
            continue;
        }
        let start = index;
        let mut text = String::new();
        while index < chars.len() {
            let next = chars[index];
            if next == '\\' && index + 1 < chars.len() && chars[index + 1].is_whitespace() {
                text.push(chars[index + 1]);
                index += 2;
                continue;
            }
            if next.is_whitespace() || BOUNDARIES.contains(next) {
                break;
            }
            text.push(next);
            index += 1;
        }
        tokens.push(Token {
            text,
            start,
            end: index.saturating_sub(1),
        });
    }
    tokens
}

/// A token is a plausible file path if it isn't a URL and either contains
/// a directory separator or looks like `name.ext`. Avoids matching bare
/// words and plain numbers.
fn looks_like_path(token: &str) -> bool {
    if token.is_empty() || token.contains("://") {
        return false;
    }
    let (base, _, _) = strip_line_column(token);
    let base_chars: Vec<char> = base.chars().collect();
    if base_chars.len() < 2 {
        return false;
    }
    if base.contains('/') {
        return true;
    }
    if base.starts_with('.') && base.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    // `name.ext` with a short alphanumeric extension.
    let dot = match base.rfind('.') {
        Some(0) | None => return false,
        Some(i) => i,
    };
    let ext = &base[dot + 1..];
    !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_alphanumeric())
}

fn parse(token: &str) -> PathMatch {
    // Strip trailing punctuation that hugs a path in prose ("see foo.ts.").
    let trimmed = token.trim_end_matches(|c| ".,:;".contains(c));
    let (path, line, column) = strip_line_column(trimmed);
    PathMatch { path, line, column }
}

/// Splits a trailing `:line`, `:line:col`, or `#Lline` suffix off a path.
fn strip_line_column(token: &str) -> (String, Option<i64>, Option<i64>) {
    // Markdown/GitHub-style file references printed by CLI agents.
    if let Some(hash) = token.rfind('#') {
        let after = &token[hash + 1..];
        if let Some(num) = after.strip_prefix('L') {
            if let Ok(line) = num.parse::<i64>() {
                if line > 0 {
                    return (token[..hash].to_string(), Some(line), None);
                }
            }
        }
    }
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() < 2 {
        return (token.to_string(), None, None);
    }
    let n = parts.len();
    // Only treat the tail as line/col when every trailing part is a number.
    if n >= 3 && !parts[n - 3].is_empty() {
        if let Ok(line) = parts[n - 2].parse::<i64>() {
            if let Ok(column) = parts[n - 1].parse::<i64>() {
                return (parts[..n - 2].join(":"), Some(line), Some(column));
            }
        }
    }
    if !parts[n - 2].is_empty() {
        if let Ok(line) = parts[n - 1].parse::<i64>() {
            return (parts[..n - 1].join(":"), Some(line), None);
        }
    }
    (token.to_string(), None, None)
}

/// Turns a clicked path token into an absolute path to an existing file,
/// or `None`. Absolute and `~` paths are used as-is; relative paths join
/// the working directory and are unresolvable without one. `file_exists`
/// is injectable for tests.
pub fn resolve_file(
    raw: &str,
    working_directory: Option<&str>,
    file_exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let path = absolute_path(raw, working_directory, home_dir().as_deref())?;
    if file_exists(&path) {
        Some(path)
    } else {
        None
    }
}

/// Resolve syntax only. Remote Host paths must never be checked against
/// the Controller's filesystem; their existence is established by the
/// Host-side command that opens them.
pub fn absolute_path(
    raw: &str,
    working_directory: Option<&str>,
    home_directory: Option<&str>,
) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let mut path = raw.to_string();
    if path == "~" {
        let home = home_directory?;
        return Some(standardize_path(home.trim_end_matches('/')));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        let home = home_directory?;
        path = format!("{}/{}", home.trim_end_matches('/'), rest);
    } else if path.starts_with('~') {
        // Resolving another user's home requires Host-owned information.
        return None;
    }
    if !path.starts_with('/') {
        let cwd = working_directory.filter(|c| !c.is_empty())?;
        path = format!("{}/{}", cwd.trim_end_matches('/'), path);
    }
    Some(standardize_path(&path))
}

/// Lexical path standardization (`..`, `.`, `//`) without touching the fs
/// — the Rust equivalent of `NSString.standardizingPath`.
fn standardize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

fn home_dir() -> Option<String> {
    std::env::var({ t("clickable_path.home") }).ok()
}

/// Matches a `file://` URL string. `allow_remote_host` mirrors the Swift
/// parameter; the Dioxus desktop passes `false` (local files only).
pub fn file_url_match(raw: &str, allow_remote_host: bool) -> Option<PathMatch> {
    let rest = raw.strip_prefix("file://")?;
    let (host, path_with_frag) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if !allow_remote_host && !(host.is_empty() || host == "localhost") {
        return None;
    }
    let (path_part, fragment) = match path_with_frag.find('#') {
        Some(i) => (&path_with_frag[..i], Some(&path_with_frag[i + 1..])),
        None => (path_with_frag, None),
    };
    if !path_part.starts_with('/') || path_part.len() < 2 {
        return None;
    }
    let path = percent_decode(path_part);
    let (stripped, line, column) = strip_line_column(&path);
    let fragment_line = fragment.and_then(|f| {
        f.strip_prefix('L')
            .and_then(|n| n.parse::<i64>().ok())
            .filter(|&l| l > 0)
    });
    Some(PathMatch {
        path: stripped,
        line: fragment_line.or(line),
        column,
    })
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b as char);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_bare_path_under_click() {
        let row = "see src/Home.tsx:42 for details";
        let col = row.find("Home").unwrap();
        let m = match_in_row(row, col).expect("path matches");
        assert_eq!(m.path, "src/Home.tsx");
        assert_eq!(m.line, Some(42));
        assert_eq!(m.column, None);
    }

    #[test]
    fn matches_line_and_column() {
        let m = match_in_row("at src/a.ts:10:4", 6).unwrap();
        assert_eq!(m.path, "src/a.ts");
        assert_eq!(m.line, Some(10));
        assert_eq!(m.column, Some(4));
    }

    #[test]
    fn matches_github_style_fragment() {
        let m = match_in_row("see foo.ts#L42", 5).unwrap();
        assert_eq!(m.path, "foo.ts");
        assert_eq!(m.line, Some(42));
    }

    #[test]
    fn ignores_urls_and_mailto() {
        assert!(match_in_row("go to https://example.com/x.ts", 10).is_none());
        assert!(match_in_row("mail mailto:a@b.co now", 8).is_none());
    }

    #[test]
    fn ignores_plain_words_and_numbers() {
        assert!(match_in_row("hello world", 2).is_none());
        assert!(match_in_row("count 12345", 7).is_none());
    }

    #[test]
    fn quoted_path_with_spaces() {
        let row = r#"open "my dir/file name.ts" now"#;
        let col = row.find("file").unwrap();
        let m = match_in_row(row, col).expect("quoted path matches");
        assert_eq!(m.path, "my dir/file name.ts");
    }

    #[test]
    fn strips_trailing_prose_punctuation() {
        let m = match_in_row("see foo.ts.", 5).unwrap();
        assert_eq!(m.path, "foo.ts");
    }

    #[test]
    fn dotfile_counts_as_path() {
        assert!(match_in_row("edit .gitignore", 7).unwrap().path == ".gitignore");
    }

    #[test]
    fn absolute_path_resolution() {
        assert_eq!(
            absolute_path("/a/b/../c", None, Some("/home/u")),
            Some("/a/c".to_string())
        );
        assert_eq!(
            absolute_path("~/x.ts", None, Some("/home/u")),
            Some("/home/u/x.ts".to_string())
        );
        assert_eq!(
            absolute_path("rel/x.ts", Some("/work"), Some("/home/u")),
            Some("/work/rel/x.ts".to_string())
        );
        // No cwd: relative path unresolvable, like Swift without a seed.
        assert_eq!(absolute_path("rel/x.ts", None, Some("/home/u")), None);
        // Another user's home: unresolvable.
        assert_eq!(absolute_path("~other/x", None, Some("/home/u")), None);
    }

    #[test]
    fn resolve_file_checks_existence() {
        let m = resolve_file("/a/b.ts", None, |p| p == "/a/b.ts");
        assert_eq!(m, Some("/a/b.ts".to_string()));
        assert_eq!(resolve_file("/a/missing.ts", None, |_| false), None);
    }

    #[test]
    fn file_url_match_parses() {
        let m = file_url_match("file:///a/b.ts#L7", false).unwrap();
        assert_eq!(m.path, "/a/b.ts");
        assert_eq!(m.line, Some(7));
        assert!(file_url_match("file://remote/a/b.ts", false).is_none());
        assert!(file_url_match("file://remote/a/b.ts", true).unwrap().path == "/a/b.ts");
        assert!(file_url_match("https://x/a.ts", false).is_none());
    }

    #[test]
    fn working_directory_prefers_reported() {
        let wd = TerminalWorkingDirectory {
            seed: Some("/seed".to_string()),
            reported: Some("/live".to_string()),
        };
        assert_eq!(wd.current(), Some("/live"));
        let wd = TerminalWorkingDirectory {
            seed: Some("/seed".to_string()),
            reported: Some("   ".to_string()),
        };
        assert_eq!(wd.current(), Some("/seed"));
    }
}
