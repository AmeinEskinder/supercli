//! Clickable file-path extraction from terminal rows.
//!
//! Ports `ClickablePath.swift` from the native app. Ghostty only matches
//! URLs/OSC 8 links natively, so bare paths (e.g. `src/Home.tsx:42` printed
//! by an agent) are detected here. Pure string logic — no surface
//! dependency.
//!
//! Also ports `TerminalWorkingDirectory`: a retained pane's live OSC 7
//! directory wins over repeated bootstrap seeds.

/// A retained pane's working directory: the live OSC 7-reported directory
/// wins over the bootstrap seed so SwiftUI refreshes never reset a shell
/// that changed directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalWorkingDirectory {
    pub seed: Option<String>,
    pub reported: Option<String>,
}

impl TerminalWorkingDirectory {
    /// The effective directory: the reported one when it is non-blank,
    /// otherwise the seed.
    pub fn current(&self) -> Option<&str> {
        if let Some(reported) = &self.reported {
            if !reported.trim().is_empty() {
                return Some(reported.as_str());
            }
        }
        self.seed.as_deref()
    }
}

/// A file-path token found in a terminal row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickablePathMatch {
    pub path: String,
    pub line: Option<i64>,
    pub column: Option<i64>,
}

/// One contiguous run of path characters and the char columns it spans.
struct Token {
    text: String,
    start: usize,
    end: usize,
}

/// Finds the path under a zero-based char offset in the row. Never choose a
/// nearby file: that steals clicks on URLs and plain text.
pub fn match_in_row(row: &str, column: usize) -> Option<ClickablePathMatch> {
    let token = tokenize(row)
        .into_iter()
        .find(|token| column >= token.start && column <= token.end)?;
    if token.text.contains("://") || token.text.to_lowercase().starts_with("mailto:") {
        return None;
    }
    let matched = parse(&token.text);
    if looks_like_path(&matched.path) {
        Some(matched)
    } else {
        None
    }
}

fn tokenize(row: &str) -> Vec<Token> {
    let chars: Vec<char> = row.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    let boundaries = "`\"'<>[](),;|";
    while index < chars.len() {
        let ch = chars[index];
        // Quoted paths can contain spaces, Unicode, brackets and parens.
        if ch == '"' || ch == '\'' || ch == '`' {
            if let Some(offset) = chars[index + 1..].iter().position(|&c| c == ch) {
                let end = index + 1 + offset;
                let text: String = chars[index + 1..end].iter().collect();
                tokens.push(Token {
                    text,
                    start: index + 1,
                    end: end.saturating_sub(1),
                });
                index = end + 1;
                continue;
            }
        }
        if ch.is_whitespace() || boundaries.contains(ch) {
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
            if next.is_whitespace() || boundaries.contains(next) {
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

/// A token is a plausible file path if it isn't a URL and either contains a
/// directory separator or looks like `name.ext`. Avoids matching bare words
/// and plain numbers.
fn looks_like_path(token: &str) -> bool {
    if token.is_empty() || token.contains("://") {
        return false;
    }
    let (base, _, _) = stripping_line_column(token);
    if base.chars().count() < 2 {
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
        Some(dot) => dot,
        None => return false,
    };
    if dot == 0 {
        return false;
    }
    let ext: Vec<char> = base[dot + 1..].chars().collect();
    !ext.is_empty() && ext.len() <= 8 && ext.iter().all(|c| c.is_alphanumeric())
}

/// Turns a clicked path token into an absolute path to an existing file, or
/// None. Absolute and `~` paths are used as-is; relative paths join
/// `working_directory` (the pane's seeded or OSC 7-reported cwd) and are
/// unresolvable without one.
pub fn resolve_file(
    raw: &str,
    working_directory: Option<&str>,
    file_exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let home = std::env::var("HOME").ok();
    let path = absolute_path(raw, working_directory, home.as_deref())?;
    if file_exists(&path) {
        Some(path)
    } else {
        None
    }
}

/// Resolve syntax only. Remote Host paths must never be checked against the
/// Controller's filesystem; their existence is established by the Host-side
/// command that opens them.
pub fn absolute_path(
    raw: &str,
    working_directory: Option<&str>,
    home_directory: Option<&str>,
) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let mut path = raw.to_string();
    if path == "~" || path.starts_with("~/") {
        let home = home_directory?;
        let rest = if path == "~" { "" } else { &path[2..] };
        path = append_component(home, rest);
    } else if path.starts_with('~') {
        // Resolving another user's home requires Host-owned information.
        return None;
    }
    if !path.starts_with('/') {
        let cwd = working_directory.filter(|cwd| !cwd.is_empty())?;
        path = append_component(cwd, &path);
    }
    Some(standardize_path(&path))
}

/// Matches a `file://` URL. The `#L123` fragment carries the line number.
pub fn file_url_match(raw: &str, allow_remote_host: bool) -> Option<ClickablePathMatch> {
    let (host, path, fragment) = parse_file_url(raw)?;
    let host_ok = allow_remote_host
        || host
            .as_deref()
            .is_none_or(|h| h.is_empty() || h == "localhost");
    if !host_ok || !path.starts_with('/') || path.is_empty() {
        return None;
    }
    let (stripped, line, column) = stripping_line_column(&path);
    let fragment_line = fragment.as_deref().and_then(|fragment| {
        let rest = fragment.strip_prefix('L')?;
        let line: i64 = rest.parse().ok()?;
        if line > 0 {
            Some(line)
        } else {
            None
        }
    });
    Some(ClickablePathMatch {
        path: stripped,
        line: fragment_line.or(line),
        column,
    })
}

/// Minimal `file://` URL decomposition into `(host, path, fragment)`.
/// Returns None when `raw` is not a file URL.
fn parse_file_url(raw: &str) -> Option<(Option<String>, String, Option<String>)> {
    let after_scheme = raw.strip_prefix("file://").or_else(|| {
        // Tolerate a case-variant scheme the same way Foundation does.
        raw.get(..7)
            .filter(|prefix| prefix.eq_ignore_ascii_case("file://"))
            .map(|_| &raw[7..])
    })?;
    let (before_fragment, fragment) = match after_scheme.find('#') {
        Some(index) => (
            &after_scheme[..index],
            Some(after_scheme[index + 1..].to_string()),
        ),
        None => (after_scheme, None),
    };
    let (host, path) = match before_fragment.find('/') {
        Some(index) => {
            let host = &before_fragment[..index];
            let host = if host.is_empty() {
                None
            } else {
                Some(host.to_string())
            };
            (host, percent_decode(&before_fragment[index..]))
        }
        None => (Some(before_fragment.to_string()), String::new()),
    };
    Some((host, path, fragment))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                out.push(high << 4 | low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse(token: &str) -> ClickablePathMatch {
    let mut trimmed = token.to_string();
    // Strip trailing punctuation that hugs a path in prose ("see foo.ts.").
    while trimmed.ends_with(['.', ',', ':', ';']) {
        trimmed.pop();
    }
    let (path, line, column) = stripping_line_column(&trimmed);
    ClickablePathMatch { path, line, column }
}

/// Splits a trailing `:line`, `:line:col`, or `#L123` suffix off a path.
fn stripping_line_column(token: &str) -> (String, Option<i64>, Option<i64>) {
    // Markdown/GitHub-style file references printed by CLI agents.
    if let Some(hash) = token.rfind('#') {
        let after = &token[hash + '#'.len_utf8()..];
        if let Some(rest) = after.strip_prefix('L') {
            if let Ok(line) = rest.parse::<i64>() {
                if line > 0 {
                    return (token[..hash].to_string(), Some(line), None);
                }
            }
        }
    }
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() >= 2 {
        // Only treat the tail as line/col when every trailing part is a number.
        if parts.len() >= 3 {
            if let (Ok(line), Ok(column)) = (
                parts[parts.len() - 2].parse::<i64>(),
                parts[parts.len() - 1].parse::<i64>(),
            ) {
                if !parts[parts.len() - 3].is_empty() {
                    let path = parts[..parts.len() - 2].join(":");
                    return (path, Some(line), Some(column));
                }
            }
        }
        if let Ok(line) = parts[parts.len() - 1].parse::<i64>() {
            if !parts[parts.len() - 2].is_empty() {
                let path = parts[..parts.len() - 1].join(":");
                return (path, Some(line), None);
            }
        }
    }
    (token.to_string(), None, None)
}

/// `NSString.appendingPathComponent` semantics: join with `/`.
fn append_component(base: &str, component: &str) -> String {
    if base.is_empty() {
        return component.to_string();
    }
    if base.ends_with('/') {
        format!("{base}{component}")
    } else {
        format!("{base}/{component}")
    }
}

/// Lexical equivalent of `NSString.standardizingPath`: resolve `.`/`..`,
/// collapse duplicate slashes, drop a trailing slash. Does not touch the
/// filesystem (no symlink resolution).
fn standardize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut components: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.last().is_some_and(|last| *last != "..") {
                    components.pop();
                } else if !absolute {
                    components.push("..");
                }
            }
            other => components.push(other),
        }
    }
    let mut result = components.join("/");
    if absolute {
        result.insert(0, '/');
    }
    if result.is_empty() {
        result.push(if absolute { '/' } else { '.' });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matched(row: &str, column: usize) -> Option<ClickablePathMatch> {
        match_in_row(row, column)
    }

    fn expected(path: &str, line: Option<i64>, column: Option<i64>) -> ClickablePathMatch {
        ClickablePathMatch {
            path: path.to_string(),
            line,
            column,
        }
    }

    #[test]
    fn relative_path_with_line_and_column() {
        let m = matched("  edited src/app/Home.tsx:42:5 ok", 12);
        assert_eq!(m, Some(expected("src/app/Home.tsx", Some(42), Some(5))));
    }

    #[test]
    fn relative_path_with_line_only() {
        let m = matched("at src/main.rs:120", 6);
        assert_eq!(m, Some(expected("src/main.rs", Some(120), None)));
    }

    #[test]
    fn plain_path_no_line() {
        let m = matched("see components/Footer.tsx here", 6);
        assert_eq!(m, Some(expected("components/Footer.tsx", None, None)));
    }

    #[test]
    fn absolute_path_match() {
        let m = matched("/Users/x/Dev/supercli/AGENTS.md:3", 4);
        assert_eq!(
            m,
            Some(expected("/Users/x/Dev/supercli/AGENTS.md", Some(3), None))
        );
    }

    #[test]
    fn tilde_path_stays_unexpanded() {
        // Expansion happens at resolve time, not extraction.
        let m = matched("~/.supercli/app-state.json", 2);
        assert_eq!(m, Some(expected("~/.supercli/app-state.json", None, None)));
    }

    #[test]
    fn picks_token_under_column_when_multiple() {
        let row = "a/one.ts and b/two.ts";
        assert_eq!(
            matched(row, 1).as_ref().map(|m| m.path.as_str()),
            Some("a/one.ts")
        );
        assert_eq!(
            matched(row, 16).as_ref().map(|m| m.path.as_str()),
            Some("b/two.ts")
        );
    }

    #[test]
    fn whitespace_does_not_activate_the_only_path_on_the_row() {
        assert_eq!(matched("   src/lib/state.swift:9   ", 0), None);
    }

    #[test]
    fn ignores_urls() {
        assert_eq!(matched("visit https://example.com/path here", 10), None);
    }

    #[test]
    fn ignores_plain_words() {
        assert_eq!(matched("just some words here", 5), None);
    }

    #[test]
    fn ignores_bare_numbers_and_timestamps() {
        assert_eq!(matched("12:34:56 build done", 2), None);
    }

    #[test]
    fn strips_trailing_sentence_punctuation() {
        let m = matched("edited foo/bar.ts.", 8);
        assert_eq!(m, Some(expected("foo/bar.ts", None, None)));
    }

    #[test]
    fn paren_wrapped_path() {
        let m = matched("(src/Home.tsx:7)", 3);
        assert_eq!(m, Some(expected("src/Home.tsx", Some(7), None)));
    }

    #[test]
    fn filename_with_extension_no_slash() {
        let m = matched("Footer.tsx changed", 2);
        assert_eq!(m, Some(expected("Footer.tsx", None, None)));
    }

    #[test]
    fn remote_resolution_does_not_require_controller_file() {
        assert_eq!(
            absolute_path("docs/readme.md", Some("/srv/worktree"), None),
            Some("/srv/worktree/docs/readme.md".to_string())
        );
    }

    #[test]
    fn working_directory_reported_wins_over_seed() {
        let dir = TerminalWorkingDirectory {
            seed: Some("/seed".to_string()),
            reported: Some("/reported".to_string()),
        };
        assert_eq!(dir.current(), Some("/reported"));
        let blank = TerminalWorkingDirectory {
            seed: Some("/seed".to_string()),
            reported: Some("   ".to_string()),
        };
        assert_eq!(blank.current(), Some("/seed"));
        let seed_only = TerminalWorkingDirectory {
            seed: Some("/seed".to_string()),
            reported: None,
        };
        assert_eq!(seed_only.current(), Some("/seed"));
    }

    #[test]
    fn absolute_path_tilde_and_dotdot() {
        assert_eq!(
            absolute_path("~/docs", None, Some("/home/user")),
            Some("/home/user/docs".to_string())
        );
        assert_eq!(
            absolute_path("a/../b", Some("/srv"), None),
            Some("/srv/b".to_string())
        );
        assert_eq!(absolute_path("rel", None, None), None);
        assert_eq!(absolute_path("", Some("/srv"), None), None);
    }

    #[test]
    fn file_url_match_parses_line_fragment() {
        let m = file_url_match("file:///tmp/a.ts#L12", false);
        assert_eq!(m, Some(expected("/tmp/a.ts", Some(12), None)));
        assert_eq!(file_url_match("file://other/tmp/a.ts", false), None);
        assert_eq!(
            file_url_match("file://other/tmp/a.ts", true).map(|m| m.path),
            Some("/tmp/a.ts".to_string())
        );
        assert_eq!(file_url_match("https://example.com/a.ts", false), None);
    }
}
