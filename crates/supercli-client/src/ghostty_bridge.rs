//! Portable logic from the legacy macOS `GhosttyBridge.swift`
//! (`clients/legacy/native/SupercliNative/Sources/SupercliNative/GhosttyBridge.swift`).
//!
//! The Swift file is the AppKit boundary around libghostty: `NSView`
//! subclasses, the `GhosttyTerminal` wrapper's `TerminalController` /
//! `TerminalView`, Metal rendering, timers, and pasteboard plumbing. None of
//! that is portable to Rust. What IS portable — and lives here — is the
//! behaviour logic the bridge implements on top of the renderer:
//!
//! - the full-screen TUI "jump to bottom" hint matcher,
//! - drop-reference quoting / shortening (the shell-safe paste text),
//! - terminal URL sanitisation for cmd-clicked / hovered links,
//! - the local-file drag-link extractor,
//! - volatile screenshot-drop stabilisation naming,
//! - the remote-pane callback epoch (stale-transport discard),
//! - the retained-VT-state reset byte sequences,
//! - the remote pane retention LRU, plus the cache prune / remove-host plans,
//! - the pane ready-for-host-bytes gate,
//! - ghostty config emission (keybind clear-list, surface overlay, theme),
//! - the remote attachment size policy,
//! - the drop-operation policy and the app-drop hover throttle.
//!
//! Rendering itself goes through the existing `ghostty_vt` FFI bindings in
//! supercli-core to the rebuilt `libghostty-vt.a`; this module deliberately
//! does not duplicate them (it emits only configuration policy and value
//! types — no FFI declarations).

use std::collections::HashSet;

/// Maximum bytes the remote-drop upload path accepts for one file.
/// Mirrors `GhosttyTerminalPane.remoteFileBytes(at:)`: files must be
/// nonempty and no larger than 64 MiB; larger or empty files are refused.
pub const MAX_REMOTE_DROP_BYTES: u64 = 64 * 1024 * 1024;

/// Whether `size` passes the remote attachment policy (nonempty, <= 64 MiB).
pub fn attachment_size_acceptable(size: u64) -> bool {
    size > 0 && size <= MAX_REMOTE_DROP_BYTES
}

// ---------------------------------------------------------------------------
// TUI "jump to bottom" hint
// ---------------------------------------------------------------------------

/// Reports whether the rendered viewport text shows a full-screen TUI's
/// "jump to bottom" hint chip.
///
/// Matches the hint chip in its two shapes — `Jump to bottom (ctrl+End)`
/// and `<N> new message(s) (ctrl+End)` — and nothing looser, and only within
/// the bottom 15 rows of the viewport, where the TUI pins the real chip.
/// Quoted hint text higher up in the transcript must not match: a spurious
/// ctrl+End at a TUI that is not scrolled up lands as literal `[1;5F` junk in
/// the composer.
///
/// Mirrors `GhosttyTerminalPane.viewportHasTuiJumpHint(_:)`. Keep aligned
/// with the matcher's contract, not with any particular TUI's wording.
pub fn viewport_has_tui_jump_hint(text: &str) -> bool {
    let lines: Vec<&str> = text.split('\n').collect();
    let tail_start = lines.len().saturating_sub(15);
    let tail = lines[tail_start..].join("\n");
    if tail.contains("Jump to bottom (ctrl+End)") {
        return true;
    }
    has_new_messages_hint(&tail)
}

/// `\d+ new messages? \(ctrl\+End\)` — the count-shaped hint variant.
fn has_new_messages_hint(tail: &str) -> bool {
    let bytes = tail.as_bytes();
    let needle = b" (ctrl+End)";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // Walk back over " new message"/" new messages" then digits.
            let before = &tail[..i];
            for suffix in [" new messages", " new message"] {
                if let Some(rest) = before.strip_suffix(suffix) {
                    if !rest.is_empty() && rest.bytes().last().is_some_and(|b| b.is_ascii_digit()) {
                        // At least one digit immediately before the suffix.
                        let digits = rest
                            .bytes()
                            .rev()
                            .take_while(|b| b.is_ascii_digit())
                            .count();
                        if digits > 0 {
                            return true;
                        }
                    }
                }
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Drop references: quoting, shortening, image detection
// ---------------------------------------------------------------------------

/// Safe-character set shared with the legacy web drop handler:
/// word chars plus `@ % + = : , . / -`. Anything else forces quoting.
fn needs_quoting(reference: &str) -> bool {
    reference.chars().any(|c| {
        !(c.is_ascii_alphanumeric()
            || c == '_'
            || matches!(c, '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-'))
    })
}

/// Quote a dropped path for pasting into a terminal prompt. Agent CLIs
/// detect single-quoted or backslash-escaped paths as attachable files, but
/// not double-quoted ones.
///
/// Mirrors `GhosttyTerminalPane.quoteDropReference(_:)`.
pub fn quote_drop_reference(reference: &str) -> String {
    if !needs_quoting(reference) {
        return reference.to_string();
    }
    format!("'{}'", reference.replace('\'', "'\\''"))
}

/// Lexically normalise an absolute path the way `NSString.standardizingPath`
/// does (collapse `.` / `..` / duplicate slashes); returns `None` for
/// non-absolute input.
fn standardized_absolute(path: &str) -> Option<String> {
    if !path.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    let mut out = String::from("/");
    out.push_str(&parts.join("/"));
    Some(out)
}

/// `true` when `path` has an image extension. Image paths deliberately stay
/// absolute in shortened drop text: agent CLIs use that form to recognise a
/// local image attachment.
///
/// Mirrors `GhosttyTerminalPane.isImageDropReference(_:)`.
pub fn is_image_drop_reference(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    // Only treat the suffix as an extension when there is a real stem.
    if !path.contains('.') || ext.len() + 1 >= path.len() {
        return false;
    }
    matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "heic"
            | "heif"
            | "tif"
            | "tiff"
            | "bmp"
            | "avif"
            | "svg"
    )
}

/// Express `path` relative to `root`, or `None` when `path` is outside `root`.
/// The root boundary is component-aware: `/work/app` never shortens
/// `/work/application/file`. `Some("")` when `path == root`.
///
/// Mirrors `GhosttyTerminalPane.pathRelativeToRoot(_:root:)`.
pub fn path_relative_to_root(path: &str, root: &str) -> Option<String> {
    if root.is_empty() {
        return None;
    }
    if path == root {
        return Some(String::new());
    }
    let prefix = if root == "/" {
        "/".to_string()
    } else {
        format!("{root}/")
    };
    path.strip_prefix(&prefix).map(|s| s.to_string())
}

/// Make a dropped path human-sized for pasting into a local terminal:
/// project-relative first, then `~/…`, then the absolute path. Non-absolute
/// input passes through unchanged.
///
/// Mirrors `GhosttyTerminalPane.conciseDropReference(_:projectRoot:homeDirectory:)`.
pub fn concise_drop_reference(
    reference: &str,
    project_root: Option<&str>,
    home_directory: &str,
) -> String {
    let Some(path) = standardized_absolute(reference) else {
        return reference.to_string();
    };

    if is_image_drop_reference(&path) {
        return path;
    }

    if let Some(root) = project_root {
        if let Some(root) = standardized_absolute(root) {
            if let Some(relative) = path_relative_to_root(&path, &root) {
                return if relative.is_empty() {
                    ".".to_string()
                } else {
                    relative
                };
            }
        }
    }
    if let Some(home) = standardized_absolute(home_directory) {
        if let Some(relative) = path_relative_to_root(&path, &home) {
            return if relative.is_empty() {
                "~".to_string()
            } else {
                format!("~/{relative}")
            };
        }
    }
    path
}

/// Compose the exact text a file/image drop pastes into the terminal:
/// each reference shortened then quoted, space-separated, no trailing
/// newline so the user can keep typing.
///
/// Mirrors the `performDragOperation` text assembly in both pane classes.
pub fn drop_paste_text(
    references: &[&str],
    project_root: Option<&str>,
    home_directory: &str,
) -> String {
    references
        .iter()
        .map(|r| quote_drop_reference(&concise_drop_reference(r, project_root, home_directory)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract a local filesystem path from an OSC 8 / hovered `file://` link
/// for native drag-out. Returns `None` for non-file URLs and for file URLs
/// with a non-local host (a remote Host path must never be advertised as a
/// Controller-local file URL).
///
/// Mirrors `GhosttyTerminalPane.localDragPath(fromLink:)`.
pub fn local_drag_path(from_link: Option<&str>) -> Option<String> {
    let raw = from_link?;
    let rest = raw.strip_prefix("file://")?;
    // Split authority from path.
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let host = authority.to_ascii_lowercase();
    if !host.is_empty() && host != "localhost" {
        return None;
    }
    let decoded = percent_decode(path);
    if !decoded.starts_with('/') {
        return None;
    }
    standardized_absolute(&decoded)
}

/// Minimal percent-decoder for `file://` URL paths.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `true` when `path` lives in a volatile location whose contents the OS may
/// delete at any moment (screenshot thumbnails under `TemporaryItems/`,
/// sandbox temp dirs). Such drops are stabilised by copying into the
/// `dropped-images` dir before referencing them.
///
/// Mirrors `GhosttyTerminalPane.isVolatileLocation(_:)`; `temp_dir` is the
/// platform temporary directory (`NSTemporaryDirectory()` on macOS).
pub fn is_volatile_location(path: &str, temp_dir: &str) -> bool {
    if path.contains("/TemporaryItems/") {
        return true;
    }
    if path.to_ascii_lowercase().contains("screencaptureui") {
        return true;
    }
    if !temp_dir.is_empty() && path.starts_with(temp_dir) {
        return true;
    }
    // Per-user sandbox temp: /var/folders/<…>/T/
    if path.contains("/var/folders/") && path.contains("/T/") {
        return true;
    }
    false
}

/// File name used when stabilising a volatile drop:
/// `drop-<timestamp_ms>-<unique>.<ext>`, defaulting the extension to `png`
/// for extensionless sources (mirrors `url.pathExtension.isEmpty ? "png"`).
///
/// Mirrors the naming in `GhosttyTerminalPane.stableDropPath(_:home:)`.
pub fn stabilized_drop_filename(path: &str, timestamp_millis: u64, unique_suffix: &str) -> String {
    let file_name = path.rsplit('/').next().unwrap_or("");
    let ext = match file_name.rsplit_once('.') {
        // A leading dot is not an extension (matches NSString.pathExtension).
        Some((stem, e)) if !stem.is_empty() && !e.is_empty() => e,
        _ => "png",
    };
    format!("drop-{timestamp_millis}-{unique_suffix}.{ext}")
}

// ---------------------------------------------------------------------------
// Terminal URL sanitisation
// ---------------------------------------------------------------------------

/// Schemes the terminal is allowed to open.
const ALLOWED_URL_SCHEMES: &[&str] = &["http", "https", "mailto", "tel", "ftp", "ftps"];

/// Turn whatever the terminal handed us into something the OS can actually
/// open. Terminal-detected links routinely arrive wrapped across lines,
/// padded with whitespace, or fenced in markdown punctuation like
/// `(https://…)` / `<https://…>`; we strip the noise, re-encode if needed,
/// infer a scheme for bare hosts, and only allow safe schemes.
///
/// Mirrors `GhosttyTerminalPane.sanitizedURL(from:)`. Returns the sanitized
/// URL string (the caller opens it); `None` means "refuse to open".
pub fn sanitized_terminal_url(raw: &str) -> Option<String> {
    // Drop internal whitespace/newlines (a URL never legitimately contains
    // any — wrapped links pick these up from the terminal grid).
    let mut s: String = raw.split_whitespace().collect();
    if s.is_empty() {
        return None;
    }

    let wrappers: &[(char, char)] = &[
        ('(', ')'),
        ('[', ']'),
        ('{', '}'),
        ('<', '>'),
        ('"', '"'),
        ('\'', '\''),
        ('`', '`'),
    ];
    loop {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() >= 2 {
            let (first, last) = (chars[0], chars[chars.len() - 1]);
            if wrappers.contains(&(first, last)) {
                s = chars[1..chars.len() - 1].iter().collect();
                continue;
            }
        }
        let last = match s.chars().last() {
            Some(c) => c,
            None => break,
        };
        if ".,;\"'`>".contains(last) {
            s.pop();
            continue;
        }
        if let Some(&(open, close)) = wrappers.iter().find(|&&(_, c)| c == last) {
            let closes = s.chars().filter(|&c| c == close).count();
            let opens = s.chars().filter(|&c| c == open).count();
            if closes > opens {
                s.pop();
                continue;
            }
        }
        break;
    }
    if s.is_empty() {
        return None;
    }

    // Add a scheme for bare hosts so `www.example.com` / `example.com/x`
    // still open in the browser instead of being treated as a file path.
    let lower = s.to_ascii_lowercase();
    if !s.contains("://") && !lower.starts_with("mailto:") && !lower.starts_with("tel:") {
        let host = s.split('/').next().unwrap_or(&s);
        if host == "localhost" || host.starts_with("localhost:") {
            s = format!("http://{s}");
        } else if host.contains('.') {
            s = format!("https://{s}");
        }
    }

    // Re-encode leftover illegal characters when the strict parse fails.
    let reparsed = if looks_like_url(&s) {
        s.clone()
    } else {
        percent_encode_url(&s)?
    };

    let scheme = if let Some(i) = reparsed.find("://") {
        reparsed[..i].to_ascii_lowercase()
    } else if let Some(i) = reparsed.find(':') {
        // Schemeless but colon-bearing: only a real scheme (no '/' before
        // the colon) counts; a path like `/tmp/x:y` has no scheme at all.
        let pre = &reparsed[..i];
        if pre.is_empty() || pre.contains('/') {
            return None;
        }
        pre.to_ascii_lowercase()
    } else {
        return None;
    };
    if !ALLOWED_URL_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    if matches!(scheme.as_str(), "http" | "https" | "ftp" | "ftps") {
        let host = url_host(&reparsed)?;
        if host.is_empty() {
            return None;
        }
    }
    Some(reparsed)
}

/// Structural check: the string parses as scheme ":" something (or
/// scheme "://" something).
fn looks_like_url(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    let scheme = &s[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && s.len() > colon + 1
}

/// Percent-encode characters outside the URL query-allowed set, mirroring
/// `addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed)`.
fn percent_encode_url(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let allowed = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_'
                    | b'.'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b'?'
                    | b'/'
                    | b':'
                    | b'@'
                    | b'#'
                    | b'['
                    | b']'
            );
        if allowed {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Extract the authority host from a `scheme://...` URL.
fn url_host(url: &str) -> Option<String> {
    let after = url.split("://").nth(1)?;
    let authority = after
        .split('/')
        .next()
        .unwrap_or(after)
        .split('@')
        .next_back()
        .unwrap_or("");
    let host = authority
        .split(':')
        .next()
        .unwrap_or(authority)
        .trim_matches(|c| c == '[' || c == ']');
    Some(host.to_string())
}

// ---------------------------------------------------------------------------
// Remote pane value types
// ---------------------------------------------------------------------------

/// Ghostty-free viewport value delivered to the remote transport whenever
/// the Controller's pane changes size. The transport decides how and when to
/// send it to the Host.
///
/// Mirrors `RemoteTerminalViewport` (the wrapper-coupled init is replaced by
/// an explicit constructor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RemoteTerminalViewport {
    pub columns: u32,
    pub rows: u32,
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub cell_width_pixels: u32,
    pub cell_height_pixels: u32,
}

impl RemoteTerminalViewport {
    pub fn new(
        columns: u32,
        rows: u32,
        width_pixels: u32,
        height_pixels: u32,
        cell_width_pixels: u32,
        cell_height_pixels: u32,
    ) -> Self {
        Self {
            columns,
            rows,
            width_pixels,
            height_pixels,
            cell_width_pixels,
            cell_height_pixels,
        }
    }
}

/// Token captured when a terminal callback is enqueued. Rebinding or clearing
/// a pane advances the token, so already-queued input/resize is discarded
/// instead of crossing into the replacement transport.
///
/// Mirrors `RemoteTerminalCallbackEpoch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RemoteTerminalCallbackEpoch {
    revision: u64,
}

impl RemoteTerminalCallbackEpoch {
    pub fn new() -> Self {
        Self { revision: 0 }
    }

    /// Advance the epoch (wrapping); call when rebinding or clearing.
    pub fn advance(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Whether a queued callback captured under `queued` may still be
    /// delivered against the current epoch.
    pub fn accepts(&self, queued: RemoteTerminalCallbackEpoch) -> bool {
        *self == queued
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

impl Default for RemoteTerminalCallbackEpoch {
    fn default() -> Self {
        Self::new()
    }
}

/// Bytes injected only into the Controller's local VT parser — never routed
/// through the session's input path, so resetting a retained frame cannot
/// write escape sequences to the Host PTY.
///
/// Mirrors `RemoteTerminalLocalFeed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTerminalLocalFeed {
    bytes: Vec<u8>,
}

impl RemoteTerminalLocalFeed {
    /// CAN aborts an unterminated OSC/DCS before RIS. A retention rebase may
    /// deliberately cut such a pathological control string to keep an
    /// always-on journal bounded, so ESC c alone could be swallowed.
    const RESET: &'static [u8] = b"\x18\x1bc";
    const BEGIN_SYNCHRONIZED_OUTPUT: &'static [u8] = b"\x1b[?2026h";
    const CLEAR_DISPLAY_AND_SCROLLBACK: &'static [u8] = b"\x1b[3J\x1b[2J\x1b[H";
    const END_SYNCHRONIZED_OUTPUT: &'static [u8] = b"\x1b[?2026l";

    /// Standalone reset: RIS clears terminal modes; CSI 3J/2J/H clears the
    /// retained screen, scrollback, and cursor. The synchronized-output pair
    /// prevents an intermediate blank frame from presenting.
    pub fn reset_retained_state() -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(Self::RESET);
        bytes.extend_from_slice(Self::BEGIN_SYNCHRONIZED_OUTPUT);
        bytes.extend_from_slice(Self::CLEAR_DISPLAY_AND_SCROLLBACK);
        bytes.extend_from_slice(Self::END_SYNCHRONIZED_OUTPUT);
        Self { bytes }
    }

    /// Atomic reset + replacement output. RIS must precede DEC 2026 because
    /// RIS itself resets synchronized-output mode.
    pub fn resetting_before_feeding(payload: &[u8]) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(Self::RESET);
        bytes.extend_from_slice(Self::BEGIN_SYNCHRONIZED_OUTPUT);
        bytes.extend_from_slice(Self::CLEAR_DISPLAY_AND_SCROLLBACK);
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(Self::END_SYNCHRONIZED_OUTPUT);
        Self { bytes }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Stable identity for a retained remote pane. Session ids are Host-local,
/// so the Host id is part of every cache lookup.
///
/// Mirrors `RemoteTerminalPaneKey`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteTerminalPaneKey {
    pub host_id: String,
    pub session_id: String,
}

impl RemoteTerminalPaneKey {
    pub fn new(host_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            host_id: host_id.into(),
            session_id: session_id.into(),
        }
    }
}

/// Pure LRU bookkeeping for the remote pane cache. Protected entries
/// (normally the selected pane) are kept first and are never evicted out
/// from under the visible surface; the limit counts the whole kept set,
/// protected keys included.
///
/// Mirrors `RemoteTerminalPaneRetention`.
#[derive(Debug, Clone)]
pub struct RemoteTerminalPaneRetention {
    limit: usize,
    most_recent: Vec<RemoteTerminalPaneKey>,
}

impl RemoteTerminalPaneRetention {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            most_recent: Vec::new(),
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn note_used(&mut self, key: RemoteTerminalPaneKey) {
        self.most_recent.retain(|k| k != &key);
        self.most_recent.push(key);
    }

    pub fn remove(&mut self, key: &RemoteTerminalPaneKey) {
        self.most_recent.retain(|k| k != key);
    }

    /// The keys to keep: protected keys first (the selected pane is never
    /// evicted out from under the visible surface), then the
    /// most-recently-used keys until the limit is reached. The limit counts
    /// the whole kept set, protected keys included — mirrors the Swift
    /// `keep.count < limit` gate exactly.
    pub fn retained(
        &mut self,
        available: &HashSet<RemoteTerminalPaneKey>,
        protecting: &HashSet<RemoteTerminalPaneKey>,
    ) -> HashSet<RemoteTerminalPaneKey> {
        self.most_recent.retain(|k| available.contains(k));
        let mut keep: HashSet<RemoteTerminalPaneKey> =
            protecting.intersection(available).cloned().collect();
        for key in self.most_recent.iter().rev() {
            if keep.len() >= self.limit {
                break;
            }
            keep.insert(key.clone());
        }
        keep
    }
}

/// A remote output cursor may advance only after the pane accepts the
/// corresponding bytes. Detached panes deliberately report not-ready:
/// buffering pre-attach bytes would make acceptance invisible to the
/// runtime and could commit a Host cursor before any surface parsed the page.
///
/// Mirrors `RemoteGhosttyTerminalPane.isReadyForHostBytes` (the ready-gate
/// inside `receiveHostBytes(_:resetBeforeFeed:)` and `resetRetainedVTState()`).
pub fn pane_ready_for_host_bytes(surface_attached: bool) -> bool {
    surface_attached
}

/// Pure core of `RemoteGhosttyPaneCache.prune`: given the cached panes, the
/// live session keys, the selected key and extra protected keys, return the
/// keys to drop in deterministic sorted order.
///
/// The native cache then clears each dropped pane's callbacks, disables
/// presentation, and removes it from the view hierarchy on the next main
/// turn.
pub fn prune_plan(
    panes: &HashSet<RemoteTerminalPaneKey>,
    live_keys: &HashSet<RemoteTerminalPaneKey>,
    selected_key: Option<&RemoteTerminalPaneKey>,
    protected_keys: &HashSet<RemoteTerminalPaneKey>,
    retention: &mut RemoteTerminalPaneRetention,
) -> Vec<RemoteTerminalPaneKey> {
    let mut protected: HashSet<RemoteTerminalPaneKey> = protected_keys.clone();
    if let Some(key) = selected_key {
        protected.insert(key.clone());
    }
    let available: HashSet<RemoteTerminalPaneKey> =
        panes.intersection(live_keys).cloned().collect();
    let keep = retention.retained(&available, &protected);
    let mut drop: Vec<RemoteTerminalPaneKey> = panes
        .iter()
        .filter(|k| !keep.contains(*k))
        .cloned()
        .collect();
    drop.sort_by(|a, b| (&a.host_id, &a.session_id).cmp(&(&b.host_id, &b.session_id)));
    drop
}

/// Keys to drop when a Host disconnects: every pane belonging to that Host.
///
/// Mirrors `RemoteGhosttyPaneCache.removeHost(_:)`.
pub fn remove_host_plan(
    panes: &HashSet<RemoteTerminalPaneKey>,
    host_id: &str,
) -> Vec<RemoteTerminalPaneKey> {
    let mut drop: Vec<RemoteTerminalPaneKey> = panes
        .iter()
        .filter(|k| k.host_id == host_id)
        .cloned()
        .collect();
    drop.sort_by(|a, b| (&a.host_id, &a.session_id).cmp(&(&b.host_id, &b.session_id)));
    drop
}

// ---------------------------------------------------------------------------
// Remote drop payloads
// ---------------------------------------------------------------------------

/// A dropped item destined for a TRUE remote Host: image content that must
/// upload (Controller paths mean nothing there), or plain text (non-file
/// links) pasted as-is.
///
/// Mirrors `GhosttyTerminalPane.RemoteDropPayload`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteDropPayload {
    /// A file the Host can receive directly (Host advertises file support).
    File { path: String },
    /// Bytes to upload through `artifact.upload`; the returned Host path is
    /// pasted instead.
    Upload { content_type: String, data: Vec<u8> },
    /// Plain text (non-file links) pasted as-is.
    Text(String),
}

/// Routing decision for one dropped file URL against a remote Host,
/// mirroring `GhosttyTerminalPane.remoteDropPayloads(from:filesSupported:)`:
/// files pass through only when the Host supports them; PNG/JPEG bytes
/// upload as-is; other image formats convert to PNG; non-image files are
/// skipped — a remote Host has no way to receive them yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteFileDropDecision {
    SendFile,
    UploadPng,
    UploadJpeg,
    ConvertToPngThenUpload,
    Skip,
}

pub fn remote_file_drop_decision(file_name: &str, files_supported: bool) -> RemoteFileDropDecision {
    if files_supported {
        return RemoteFileDropDecision::SendFile;
    }
    let ext = file_name
        .rsplit('/')
        .next()
        .unwrap_or(file_name)
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => RemoteFileDropDecision::UploadPng,
        "jpg" | "jpeg" => RemoteFileDropDecision::UploadJpeg,
        ext if is_image_drop_reference(&format!("x.{ext}")) => {
            RemoteFileDropDecision::ConvertToPngThenUpload
        }
        _ => RemoteFileDropDecision::Skip,
    }
}

// ---------------------------------------------------------------------------
// Ghostty config emission
// ---------------------------------------------------------------------------

/// Explicit subset of Ghostty's macOS defaults retained after `keybind =
/// clear`. libghostty ships ~92 default keybinds and a focused surface
/// consumes any chord it has a binding for before menu key equivalents run,
/// so defaults like super+w=close_surface silently eat app chords. Clearing
/// them all makes the menu the single owner of app chords.
///
/// Mirrors `GhosttyTerminalPane.surfaceKeybinds`.
pub const SURFACE_KEYBINDS: &[&str] = &[
    "performable:super+c=copy_to_clipboard",
    "performable:super+v=paste_from_clipboard",
    // Scrollback navigation (same as Ghostty's macOS defaults).
    "super+home=scroll_to_top",
    "super+end=scroll_to_bottom",
    "super+page_up=scroll_page_up",
    "super+page_down=scroll_page_down",
];

/// Build the `keybind` config lines for a fresh ghostty config: `clear`
/// first, then the retained subset.
pub fn surface_keybind_config_lines() -> Vec<(String, String)> {
    let mut lines = vec![("keybind".to_string(), "clear".to_string())];
    lines.extend(
        SURFACE_KEYBINDS
            .iter()
            .map(|k| ("keybind".to_string(), k.to_string())),
    );
    lines
}

/// The per-pane config overlay pushed to a LIVE surface whenever one of its
/// values moves: canvas opacity plus the terminal font and line height.
/// `font-family` is a repeatable Ghostty key and the base config already
/// named a family at construction, so the overlay clears the list first (an
/// empty value resets it) and then names the current family — or leaves it
/// cleared for Ghostty's bundled default.
///
/// Mirrors `GhosttyTerminalPane.surfaceOverlayConfiguration(for:)`.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceOverlayConfig {
    pub background_opacity: f64,
    pub font_size: f64,
    pub font_family: Option<String>,
    pub line_height_percent: u32,
}

impl SurfaceOverlayConfig {
    pub fn config_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = vec![
            (
                "background-opacity".to_string(),
                format!("{}", self.background_opacity),
            ),
            ("font-size".to_string(), format!("{}", self.font_size)),
            // Clear the repeatable key first, then name the current family.
            ("font-family".to_string(), String::new()),
        ];
        if let Some(family) = &self.font_family {
            pairs.push(("font-family".to_string(), family.clone()));
        }
        pairs.push((
            "adjust-cell-height".to_string(),
            format!("{}%", self.line_height_percent),
        ));
        pairs
    }
}

/// One light/dark theme variant as plain hex strings (no AppKit types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalThemeVariant {
    pub background: String,
    pub foreground: String,
    pub selection_background: String,
    pub cursor_color: String,
    /// 16 ANSI palette entries, hex strings.
    pub palette: [String; 16],
}

/// Emit a ghostty theme config block from a plain-Swift theme variant.
///
/// Mirrors `GhosttyTerminalPane.themeConfiguration(_:)`.
pub fn theme_config_pairs(variant: &TerminalThemeVariant) -> Vec<(String, String)> {
    let mut pairs = vec![
        ("background".to_string(), variant.background.clone()),
        ("foreground".to_string(), variant.foreground.clone()),
        (
            "selection-background".to_string(),
            variant.selection_background.clone(),
        ),
        ("cursor-color".to_string(), variant.cursor_color.clone()),
    ];
    for (index, color) in variant.palette.iter().enumerate() {
        pairs.push((format!("palette={index}"), color.clone()));
    }
    pairs
}

/// Compute the letterboxed view size for a target grid: grid cells plus any
/// chrome (padding) the surface keeps around the grid.
///
/// Mirrors `GhosttyTerminalPane.letterboxSize(cols:rows:)`; `None` when the
/// surface has no usable cell metrics yet.
pub fn letterbox_size(
    cols: u32,
    rows: u32,
    cell: (f64, f64),
    bounds: (f64, f64),
    grid: (u32, u32),
) -> Option<(f64, f64)> {
    if cell.0 <= 0.0 || cell.1 <= 0.0 || grid.0 == 0 || grid.1 == 0 {
        return None;
    }
    if bounds.0 <= 1.0 || bounds.1 <= 1.0 {
        return None;
    }
    let chrome_w = (bounds.0 - grid.0 as f64 * cell.0).max(0.0);
    let chrome_h = (bounds.1 - grid.1 as f64 * cell.1).max(0.0);
    Some((
        cols as f64 * cell.0 + chrome_w,
        rows as f64 * cell.1 + chrome_h,
    ))
}

/// Configuration key/value pairs for a per-pane ghostty config, emitted as
/// data so any config writer can apply them. Mirrors the `builder.withCustom`
/// calls in `GhosttyTerminalPane.init(command:workingDirectory:…)`.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneConfig {
    pub command: String,
    pub window_padding_x: u32,
    pub window_padding_y: u32,
    pub window_padding_balanced: bool,
    pub mouse_scroll_multiplier: u32,
    pub font_size: f64,
    pub font_family: Option<String>,
    pub line_height_percent: u32,
    pub background_opacity: f64,
}

impl PaneConfig {
    pub fn config_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = vec![
            ("command".to_string(), self.command.clone()),
            // Don't keep dead surfaces around.
            ("wait-after-command".to_string(), "false".to_string()),
            ("shell-integration".to_string(), "detect".to_string()),
            (
                "window-padding-x".to_string(),
                self.window_padding_x.to_string(),
            ),
            (
                "window-padding-y".to_string(),
                self.window_padding_y.to_string(),
            ),
            (
                "window-padding-balance".to_string(),
                self.window_padding_balanced.to_string(),
            ),
            // Extend the terminal background into padding so empty rows /
            // residual padding match the TUI canvas.
            ("window-padding-color".to_string(), "extend".to_string()),
            // Discrete (wheel-tick) speed only; precision stays 1 so
            // mouse-captured TUIs don't jump multiple lines per tick.
            (
                "mouse-scroll-multiplier".to_string(),
                format!("precision:1,discrete:{}", self.mouse_scroll_multiplier),
            ),
            ("cursor-style".to_string(), "block".to_string()),
            ("cursor-style-blink".to_string(), "true".to_string()),
            ("font-size".to_string(), format!("{}", self.font_size)),
        ];
        if let Some(family) = &self.font_family {
            pairs.push(("font-family".to_string(), family.clone()));
        }
        pairs.push((
            "adjust-cell-height".to_string(),
            format!("{}%", self.line_height_percent),
        ));
        pairs.push((
            "background-opacity".to_string(),
            format!("{}", self.background_opacity),
        ));
        pairs
    }
}

/// Lookup helper for config pair lists (exact key match).
pub fn config_value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Drop operation and app-drop hover throttle
// ---------------------------------------------------------------------------

/// The drag operation the bridge reports for a drop over the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOperation {
    /// `.copy`: on a registered App drop target, or on a plain terminal
    /// that accepts plain-text drops.
    Copy,
    /// Nothing (the "not allowed" cursor): over an App pane with no
    /// registered drop target, where keystrokes would land in the App's UI.
    None,
}

/// `.copy` on a registered App drop target or on a plain terminal;
/// nothing over an App pane elsewhere.
///
/// Mirrors `GhosttyTerminalPane.dropOperation(_:)`.
pub fn drop_operation(
    accepts_plain_text_drops: bool,
    app_drop_target: Option<(u32, u32)>,
) -> DropOperation {
    if !accepts_plain_text_drops && app_drop_target.is_none() {
        DropOperation::None
    } else {
        DropOperation::Copy
    }
}

/// Throttle interval for app-drop hover writes (Swift `appDropHoverInterval`).
/// Hover writes intentionally repeat at a stationary edge so the TUI can
/// continue auto-scrolling.
pub const APP_DROP_HOVER_INTERVAL_S: f64 = 0.075;

/// Events the hover tracker emits for the session's drop-target map file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropHoverEvent {
    /// Write a hover event for this cell.
    Hover { row: u32, column: u32 },
    /// Write a leave event (drag left the drop target).
    Leave,
}

/// Pure state machine for the app-drop hover throttle.
///
/// Mirrors `GhosttyTerminalPane.updateAppDropHover(_:)` /
/// `finishAppDropHover()` / `resetAppDropHover()`: hover writes fire when
/// the cell changes or the throttle interval has elapsed, and a leave
/// event fires once when the drag leaves the target.
#[derive(Debug, Clone, Default)]
pub struct AppDropHover {
    active: bool,
    last_cell: Option<(u32, u32)>,
    last_at: f64,
}

impl AppDropHover {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the current drop-target cell (`None` when the pointer is over no
    /// registered target) and the current time. Returns the map event to
    /// write, if any.
    pub fn poll(&mut self, cell: Option<(u32, u32)>, now_s: f64) -> Option<DropHoverEvent> {
        let Some(cell) = cell else {
            return self.finish();
        };
        self.active = true;
        let changed = self.last_cell != Some(cell);
        if changed || now_s - self.last_at >= APP_DROP_HOVER_INTERVAL_S {
            self.last_cell = Some(cell);
            self.last_at = now_s;
            Some(DropHoverEvent::Hover {
                row: cell.0,
                column: cell.1,
            })
        } else {
            None
        }
    }

    /// The drag left the surface: emit `Leave` once if a hover was active.
    pub fn finish(&mut self) -> Option<DropHoverEvent> {
        if !self.active {
            return None;
        }
        self.reset();
        Some(DropHoverEvent::Leave)
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.last_cell = None;
        self.last_at = 0.0;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn key(host: &str, session: &str) -> RemoteTerminalPaneKey {
        RemoteTerminalPaneKey::new(host, session)
    }

    // --- TUI jump hint ------------------------------------------------------

    #[test]
    fn jump_hint_matches_plain_shape_in_tail() {
        let text = (0..30)
            .map(|i| format!("line {i}"))
            .chain(std::iter::once("Jump to bottom (ctrl+End)".to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(viewport_has_tui_jump_hint(&text));
    }

    #[test]
    fn jump_hint_matches_count_shape() {
        let text = "some output\n3 new messages (ctrl+End) ↓\nmore".to_string();
        assert!(viewport_has_tui_jump_hint(&text));
    }

    #[test]
    fn jump_hint_matches_singular_message() {
        let text = "x\n1 new message (ctrl+End)".to_string();
        assert!(viewport_has_tui_jump_hint(&text));
    }

    #[test]
    fn jump_hint_ignores_quoted_text_above_tail() {
        // Hint text quoted 20 lines up must not arm the button.
        let mut lines: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        lines.insert(2, "someone wrote: Jump to bottom (ctrl+End)".to_string());
        lines.extend((0..10).map(|i| format!("tail {i}")));
        let text = lines.join("\n");
        assert!(!viewport_has_tui_jump_hint(&text));
    }

    #[test]
    fn jump_hint_rejects_loose_matches() {
        assert!(!viewport_has_tui_jump_hint(
            "Jump to bottom\nwithout the key suffix"
        ));
        assert!(!viewport_has_tui_jump_hint("99 new things (ctrl+End)"));
        assert!(!viewport_has_tui_jump_hint("plain terminal output"));
    }

    #[test]
    fn jump_hint_empty_text_is_quiet() {
        assert!(!viewport_has_tui_jump_hint(""));
    }

    // --- Drop quoting / shortening ------------------------------------------

    #[test]
    fn quote_passes_safe_paths_through() {
        assert_eq!(
            quote_drop_reference("/Users/me/file-name_1.txt"),
            "/Users/me/file-name_1.txt"
        );
        assert_eq!(
            quote_drop_reference("a@b%c=d:e,f.g/h-i"),
            "a@b%c=d:e,f.g/h-i"
        );
    }

    #[test]
    fn quote_wraps_paths_with_spaces() {
        assert_eq!(
            quote_drop_reference("/Users/me/My File.txt"),
            "'/Users/me/My File.txt'"
        );
    }

    #[test]
    fn quote_escapes_embedded_single_quotes() {
        assert_eq!(quote_drop_reference("/a/b'c/d.txt"), "'/a/b'\\''c/d.txt'");
    }

    #[test]
    fn concise_shortens_under_project_root() {
        assert_eq!(
            concise_drop_reference("/work/app/src/main.rs", Some("/work/app"), "/Users/me"),
            "src/main.rs"
        );
    }

    #[test]
    fn concise_root_itself_becomes_dot() {
        assert_eq!(
            concise_drop_reference("/work/app", Some("/work/app"), "/Users/me"),
            "."
        );
    }

    #[test]
    fn concise_respects_component_boundaries() {
        // /work/app must not shorten /work/application/file.
        assert_eq!(
            concise_drop_reference("/work/application/file.txt", Some("/work/app"), "/Users/me"),
            "/work/application/file.txt"
        );
    }

    #[test]
    fn concise_falls_back_to_home_tilde() {
        assert_eq!(
            concise_drop_reference("/Users/me/docs/n.txt", None, "/Users/me"),
            "~/docs/n.txt"
        );
        assert_eq!(concise_drop_reference("/Users/me", None, "/Users/me"), "~");
    }

    #[test]
    fn concise_keeps_image_paths_absolute() {
        assert_eq!(
            concise_drop_reference("/work/app/assets/logo.png", Some("/work/app"), "/Users/me"),
            "/work/app/assets/logo.png"
        );
    }

    #[test]
    fn concise_passes_relative_references_through() {
        assert_eq!(
            concise_drop_reference("relative/path.txt", Some("/work/app"), "/Users/me"),
            "relative/path.txt"
        );
    }

    #[test]
    fn image_detection_covers_known_extensions() {
        for ext in [
            "png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "tif", "tiff", "bmp", "avif",
            "svg",
        ] {
            assert!(is_image_drop_reference(&format!("/a/b.{ext}")), "{ext}");
            assert!(
                is_image_drop_reference(&format!("/a/b.{ext}", ext = ext.to_ascii_uppercase())),
                "{ext} upper"
            );
        }
        assert!(!is_image_drop_reference("/a/b.txt"));
        assert!(!is_image_drop_reference("/a/b"));
    }

    #[test]
    fn drop_paste_text_quotes_and_joins() {
        let text = drop_paste_text(
            &["/work/app/src/a.rs", "/work/app/My Doc.txt"],
            Some("/work/app"),
            "/Users/me",
        );
        assert_eq!(text, "src/a.rs 'My Doc.txt'");
    }

    // --- Local drag path ----------------------------------------------------

    #[test]
    fn local_drag_path_accepts_file_urls() {
        assert_eq!(
            local_drag_path(Some("file:///tmp/a%20folder/hello.txt")),
            Some("/tmp/a folder/hello.txt".to_string())
        );
        assert_eq!(
            local_drag_path(Some("file:///Users/me/My%20File.txt")),
            Some("/Users/me/My File.txt".to_string())
        );
        assert_eq!(
            local_drag_path(Some("file://localhost/Users/me/x.txt")),
            Some("/Users/me/x.txt".to_string())
        );
    }

    #[test]
    fn local_drag_path_rejects_remote_hosts_and_schemes() {
        assert_eq!(
            local_drag_path(Some("file://otherhost/Users/me/x.txt")),
            None
        );
        assert_eq!(local_drag_path(Some("https://example.com/x.txt")), None);
        assert_eq!(local_drag_path(None), None);
    }

    // --- Volatile locations --------------------------------------------------

    #[test]
    fn volatile_locations_detected() {
        let tmp = "/var/folders/zz/T/";
        assert!(is_volatile_location(
            "/private/var/folders/x/TemporaryItems/NSIRD_screencaptureui_1.png",
            tmp
        ));
        assert!(is_volatile_location("/tmp/Screen Shot.png", "/tmp/"));
        assert!(is_volatile_location("/var/folders/zz/T/com.app/x.png", tmp));
        assert!(!is_volatile_location("/Users/me/Desktop/x.png", tmp));
    }

    #[test]
    fn stabilized_filename_scheme() {
        assert_eq!(
            stabilized_drop_filename("/tmp/shot.png", 1700000000123, "abc"),
            "drop-1700000000123-abc.png"
        );
        // Extensionless sources default to png.
        assert_eq!(
            stabilized_drop_filename("/tmp/shot", 1, "z"),
            "drop-1-z.png"
        );
    }

    // --- URL sanitisation ----------------------------------------------------
    // Mirrors OpenURLSanitizerTests and TerminalLinkRegressionTests.

    #[test]
    fn sanitized_url_plain_and_wrapped_links() {
        assert_eq!(
            sanitized_terminal_url("https://quiz.ai"),
            Some("https://quiz.ai".to_string())
        );
        assert_eq!(
            sanitized_terminal_url("(https://quiz.ai)"),
            Some("https://quiz.ai".to_string())
        );
        assert_eq!(
            sanitized_terminal_url("<https://quiz.ai>"),
            Some("https://quiz.ai".to_string())
        );
        // Nested backtick + angle wrappers peel inside-out.
        assert_eq!(
            sanitized_terminal_url("`<https://example.com>`"),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn sanitized_url_drops_trailing_punctuation_keeps_significant() {
        assert_eq!(
            sanitized_terminal_url("https://quiz.ai."),
            Some("https://quiz.ai".to_string())
        );
        assert_eq!(
            sanitized_terminal_url("https://quiz.ai,"),
            Some("https://quiz.ai".to_string())
        );
        for raw in [
            "https://en.wikipedia.org/wiki/Function_(mathematics)",
            "https://example.com/search?q=why?",
            "https://example.com/hello!",
        ] {
            assert_eq!(sanitized_terminal_url(raw), Some(raw.to_string()));
        }
        assert_eq!(
            sanitized_terminal_url("(https://example.com/a_(b))."),
            Some("https://example.com/a_(b)".to_string())
        );
    }

    #[test]
    fn sanitized_url_joins_wrapped_lines() {
        // Terminal-wrapped link: split across a line break with padding.
        assert_eq!(
            sanitized_terminal_url("https://search.google.com/\nsearch-console/"),
            Some("https://search.google.com/search-console/".to_string())
        );
        assert_eq!(
            sanitized_terminal_url("https://example.com/\n  a?x=1&y=2"),
            Some("https://example.com/a?x=1&y=2".to_string())
        );
    }

    #[test]
    fn sanitized_url_infers_scheme_for_bare_hosts() {
        assert_eq!(
            sanitized_terminal_url("www.example.com/path"),
            Some("https://www.example.com/path".to_string())
        );
        assert_eq!(
            sanitized_terminal_url("example.com"),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            sanitized_terminal_url("localhost:3000/path"),
            Some("http://localhost:3000/path".to_string())
        );
    }

    #[test]
    fn sanitized_url_keeps_allowed_schemes() {
        assert_eq!(
            sanitized_terminal_url("mailto:hi@quiz.ai"),
            Some("mailto:hi@quiz.ai".to_string())
        );
        assert_eq!(
            sanitized_terminal_url("ftp://files.example.com/x"),
            Some("ftp://files.example.com/x".to_string())
        );
    }

    #[test]
    fn sanitized_url_rejects_unsafe_or_hostless() {
        // LaunchServices must not get a scheme that triggers arbitrary apps.
        assert_eq!(
            sanitized_terminal_url("file:///Applications/Calculator.app"),
            None
        );
        assert_eq!(sanitized_terminal_url("javascript:alert(1)"), None);
        // webLinksRequireAHost.
        assert_eq!(sanitized_terminal_url("https://"), None);
        assert_eq!(sanitized_terminal_url("http:///report"), None);
        // Empty and junk.
        assert_eq!(sanitized_terminal_url(""), None);
        assert_eq!(sanitized_terminal_url("   \n  "), None);
        assert_eq!(sanitized_terminal_url("just some text"), None);
    }

    // --- Callback epoch ------------------------------------------------------

    #[test]
    fn epoch_advance_invalidates_queued_callbacks() {
        let mut epoch = RemoteTerminalCallbackEpoch::new();
        let queued = epoch;
        assert!(epoch.accepts(queued));
        epoch.advance();
        assert!(!epoch.accepts(queued));
        assert!(epoch.accepts(epoch));
        assert_eq!(epoch.revision(), 1);
    }

    #[test]
    fn epoch_wraps_without_panic() {
        let mut epoch = RemoteTerminalCallbackEpoch { revision: u64::MAX };
        epoch.advance();
        assert_eq!(epoch.revision(), 0);
    }

    // --- Local feed bytes ----------------------------------------------------

    #[test]
    fn local_feed_reset_bytes_are_exact() {
        let feed = RemoteTerminalLocalFeed::reset_retained_state();
        let expected: Vec<u8> = b"\x18\x1bc"
            .iter()
            .chain(b"\x1b[?2026h".iter())
            .chain(b"\x1b[3J\x1b[2J\x1b[H".iter())
            .chain(b"\x1b[?2026l".iter())
            .copied()
            .collect();
        assert_eq!(feed.bytes(), expected.as_slice());
    }

    #[test]
    fn local_feed_wraps_payload_atomically() {
        let feed = RemoteTerminalLocalFeed::resetting_before_feeding(b"hello");
        let reset = RemoteTerminalLocalFeed::reset_retained_state();
        // Payload sits between the clear and the end of synchronized output.
        let tail = b"\x1b[?2026l";
        let head_len = reset.bytes().len() - tail.len();
        assert_eq!(&feed.bytes()[..head_len], &reset.bytes()[..head_len]);
        assert_eq!(&feed.bytes()[head_len..head_len + 5], b"hello");
        assert_eq!(&feed.bytes()[head_len + 5..], tail);
    }

    // --- Pane retention LRU --------------------------------------------------

    #[test]
    fn retention_keeps_most_recent_up_to_limit() {
        let mut r = RemoteTerminalPaneRetention::new(2);
        let (a, b, c) = (key("h", "a"), key("h", "b"), key("h", "c"));
        r.note_used(a.clone());
        r.note_used(b.clone());
        r.note_used(c.clone());
        let available: HashSet<_> = [a, b.clone(), c.clone()].into_iter().collect();
        let keep = r.retained(&available, &HashSet::new());
        assert_eq!(keep, [b, c].into_iter().collect());
    }

    #[test]
    fn retention_note_used_refreshes_recency() {
        let mut r = RemoteTerminalPaneRetention::new(2);
        let (a, b, c) = (key("h", "a"), key("h", "b"), key("h", "c"));
        r.note_used(a.clone());
        r.note_used(b.clone());
        r.note_used(a.clone()); // a becomes most recent
        r.note_used(c.clone());
        let available: HashSet<_> = [a.clone(), b, c.clone()].into_iter().collect();
        let keep = r.retained(&available, &HashSet::new());
        assert_eq!(keep, [a, c].into_iter().collect());
    }

    #[test]
    fn retention_protects_selected_within_limit() {
        // Mirrors testSelectedPaneIsProtectedEvenWhenNotRecent: the protected
        // (selected) pane wins the slot even when it is not the most recent.
        let mut r = RemoteTerminalPaneRetention::new(1);
        let (a, b) = (key("h", "a"), key("h", "b"));
        r.note_used(a.clone());
        r.note_used(b.clone());
        let available: HashSet<_> = [a.clone(), b.clone()].into_iter().collect();
        let protecting: HashSet<_> = [a.clone()].into_iter().collect();
        let keep = r.retained(&available, &protecting);
        assert_eq!(keep, [a].into_iter().collect());
    }

    #[test]
    fn retention_protects_selected_even_when_not_recent() {
        // Exact port of Swift testSelectedPaneIsProtectedEvenWhenNotRecent.
        let mut r = RemoteTerminalPaneRetention::new(2);
        let (selected, recent, newest) = (
            key("host", "selected"),
            key("host", "recent"),
            key("host", "newest"),
        );
        r.note_used(selected.clone());
        r.note_used(recent.clone());
        r.note_used(newest.clone());
        let available: HashSet<_> = [selected.clone(), recent, newest.clone()]
            .into_iter()
            .collect();
        let keep = r.retained(&available, &[selected.clone()].into_iter().collect());
        assert_eq!(keep, [selected, newest].into_iter().collect());
    }

    #[test]
    fn retention_prunes_unavailable_and_removed() {
        let mut r = RemoteTerminalPaneRetention::new(8);
        let (a, b) = (key("h", "a"), key("h", "b"));
        r.note_used(a.clone());
        r.note_used(b.clone());
        r.remove(&a);
        let available: HashSet<_> = [b.clone()].into_iter().collect();
        let keep = r.retained(&available, &HashSet::new());
        assert_eq!(keep, [b].into_iter().collect());
    }

    #[test]
    fn retention_limit_floor_is_one() {
        assert_eq!(RemoteTerminalPaneRetention::new(0).limit(), 1);
    }

    // --- Remote drop decisions ------------------------------------------------

    #[test]
    fn remote_file_drop_routing() {
        assert_eq!(
            remote_file_drop_decision("a.png", false),
            RemoteFileDropDecision::UploadPng
        );
        assert_eq!(
            remote_file_drop_decision("a.JPG", false),
            RemoteFileDropDecision::UploadJpeg
        );
        assert_eq!(
            remote_file_drop_decision("a.heic", false),
            RemoteFileDropDecision::ConvertToPngThenUpload
        );
        assert_eq!(
            remote_file_drop_decision("a.zip", false),
            RemoteFileDropDecision::Skip
        );
        assert_eq!(
            remote_file_drop_decision("a.zip", true),
            RemoteFileDropDecision::SendFile
        );
    }

    #[test]
    fn attachment_size_policy() {
        assert!(!attachment_size_acceptable(0));
        assert!(attachment_size_acceptable(1));
        assert!(attachment_size_acceptable(MAX_REMOTE_DROP_BYTES));
        assert!(!attachment_size_acceptable(MAX_REMOTE_DROP_BYTES + 1));
    }

    // --- Config emission ------------------------------------------------------

    #[test]
    fn keybind_config_clears_first() {
        let lines = surface_keybind_config_lines();
        assert_eq!(lines[0], ("keybind".to_string(), "clear".to_string()));
        assert_eq!(lines.len(), 1 + SURFACE_KEYBINDS.len());
        assert!(lines.iter().any(|(_, v)| v == "super+end=scroll_to_bottom"));
        // Font zoom is deliberately NOT retained: the View menu owns those
        // chords (a surface-level increase_font_size would flip libghostty's
        // font_size_adjusted and break config reloads for that surface).
        assert!(!lines.iter().any(|(_, v)| v.contains("font")));
        assert!(lines
            .iter()
            .any(|(_, v)| v == "performable:super+c=copy_to_clipboard"));
    }

    #[test]
    fn overlay_config_clears_font_family_before_setting() {
        let cfg = SurfaceOverlayConfig {
            background_opacity: 0.9,
            font_size: 13.0,
            font_family: Some("Menlo".to_string()),
            line_height_percent: 110,
        };
        let pairs = cfg.config_pairs();
        let families: Vec<&str> = pairs
            .iter()
            .filter(|(k, _)| k == "font-family")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(families, ["", "Menlo"]);
        assert_eq!(config_value(&pairs, "adjust-cell-height"), Some("110%"));
    }

    #[test]
    fn overlay_config_without_family_leaves_it_cleared() {
        let cfg = SurfaceOverlayConfig {
            background_opacity: 1.0,
            font_size: 12.0,
            font_family: None,
            line_height_percent: 100,
        };
        let pairs = cfg.config_pairs();
        let families: Vec<&str> = pairs
            .iter()
            .filter(|(k, _)| k == "font-family")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(families, [""]);
    }

    fn sample_variant() -> TerminalThemeVariant {
        TerminalThemeVariant {
            background: "#ffffff".into(),
            foreground: "#000000".into(),
            selection_background: "#b5d5ff".into(),
            cursor_color: "#000000".into(),
            palette: std::array::from_fn(|i| format!("#{i:06x}")),
        }
    }

    #[test]
    fn theme_config_emits_all_keys() {
        let pairs = theme_config_pairs(&sample_variant());
        let map: HashMap<_, _> = pairs.into_iter().collect();
        assert_eq!(map["background"], "#ffffff");
        assert_eq!(map["palette=0"], "#000000");
        assert_eq!(map["palette=15"], "#00000f");
        assert_eq!(map.len(), 4 + 16);
    }

    #[test]
    fn letterbox_size_accounts_for_chrome() {
        // 10x5 grid of 8x16 cells in a 100x100 view: 20px horizontal chrome.
        let size = letterbox_size(12, 6, (8.0, 16.0), (100.0, 100.0), (10, 5));
        assert_eq!(size, Some((12.0 * 8.0 + 20.0, 6.0 * 16.0 + 20.0)));
    }

    #[test]
    fn letterbox_size_needs_cell_metrics() {
        assert_eq!(
            letterbox_size(12, 6, (0.0, 16.0), (100.0, 100.0), (10, 5)),
            None
        );
        assert_eq!(
            letterbox_size(12, 6, (8.0, 16.0), (1.0, 100.0), (10, 5)),
            None
        );
        assert_eq!(
            letterbox_size(12, 6, (8.0, 16.0), (100.0, 100.0), (0, 5)),
            None
        );
    }

    #[test]
    fn pane_config_emits_expected_keys() {
        let cfg = PaneConfig {
            command: "/bin/zsh --login".into(),
            window_padding_x: 8,
            window_padding_y: 4,
            window_padding_balanced: true,
            mouse_scroll_multiplier: 3,
            font_size: 13.0,
            font_family: Some("Menlo".into()),
            line_height_percent: 110,
            background_opacity: 1.0,
        };
        let pairs = cfg.config_pairs();
        assert_eq!(config_value(&pairs, "command"), Some("/bin/zsh --login"));
        assert_eq!(config_value(&pairs, "wait-after-command"), Some("false"));
        assert_eq!(
            config_value(&pairs, "mouse-scroll-multiplier"),
            Some("precision:1,discrete:3")
        );
        assert_eq!(config_value(&pairs, "window-padding-color"), Some("extend"));
        assert_eq!(config_value(&pairs, "font-family"), Some("Menlo"));
    }

    // --- prune plans, drop operation, hover throttle, readiness gate ------

    fn key_set(keys: &[RemoteTerminalPaneKey]) -> HashSet<RemoteTerminalPaneKey> {
        keys.iter().cloned().collect()
    }

    #[test]
    fn pane_key_identity_is_scoped_by_host() {
        // Mirrors RemoteGhosttyPaneRetentionTests.testSessionIdentityIsScopedByHost.
        let studio = key("studio", "same-session");
        let server = key("server", "same-session");
        assert_ne!(studio, server);
        let set: HashSet<_> = [studio, server].into_iter().collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn prune_plan_drops_dead_and_over_limit_panes() {
        let (a, b, c) = (key("h", "a"), key("h", "b"), key("h2", "c"));
        let mut r = RemoteTerminalPaneRetention::new(1);
        r.note_used(a.clone());
        r.note_used(b.clone());
        let panes = key_set(&[a.clone(), b.clone(), c.clone()]);
        // c is dead (not live); a is stale LRU under limit 1.
        let drop = prune_plan(
            &panes,
            &key_set(&[a.clone(), b.clone()]),
            None,
            &key_set(&[]),
            &mut r,
        );
        assert_eq!(drop, vec![a, c]);
    }

    #[test]
    fn prune_plan_never_evicts_selected() {
        let (a, b) = (key("h", "a"), key("h", "b"));
        let mut r = RemoteTerminalPaneRetention::new(1);
        r.note_used(b.clone());
        r.note_used(a.clone());
        let panes = key_set(&[a.clone(), b.clone()]);
        let drop = prune_plan(&panes, &panes, Some(&a), &key_set(&[]), &mut r);
        assert_eq!(drop, vec![b]);
    }

    #[test]
    fn remove_host_plan_targets_one_host() {
        let (a, b, c) = (key("h1", "a"), key("h2", "b"), key("h1", "c"));
        let panes = key_set(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(remove_host_plan(&panes, "h1"), vec![a, c]);
        assert!(remove_host_plan(&panes, "h3").is_empty());
    }

    #[test]
    fn drop_operation_policy() {
        assert_eq!(drop_operation(true, None), DropOperation::Copy);
        assert_eq!(drop_operation(false, Some((1, 2))), DropOperation::Copy);
        assert_eq!(drop_operation(false, None), DropOperation::None);
    }

    #[test]
    fn hover_throttle_emits_on_change_then_repeats_at_interval() {
        let mut hover = AppDropHover::new();
        // First hover writes.
        assert_eq!(
            hover.poll(Some((3, 4)), 1.0),
            Some(DropHoverEvent::Hover { row: 3, column: 4 })
        );
        // Stationary, within the interval: suppressed.
        assert_eq!(hover.poll(Some((3, 4)), 1.05), None);
        // Stationary but past the interval: repeats for TUI auto-scroll.
        assert_eq!(
            hover.poll(Some((3, 4)), 1.1),
            Some(DropHoverEvent::Hover { row: 3, column: 4 })
        );
        // Cell change always writes.
        assert_eq!(
            hover.poll(Some((3, 5)), 1.11),
            Some(DropHoverEvent::Hover { row: 3, column: 5 })
        );
    }

    #[test]
    fn hover_finish_emits_leave_once() {
        let mut hover = AppDropHover::new();
        assert_eq!(hover.poll(None, 1.0), None);
        hover.poll(Some((1, 1)), 1.0);
        assert_eq!(hover.poll(None, 1.1), Some(DropHoverEvent::Leave));
        assert_eq!(hover.poll(None, 1.2), None);
        assert!(!hover.is_active());
    }

    #[test]
    fn pane_ready_for_host_bytes_requires_attached_surface() {
        assert!(pane_ready_for_host_bytes(true));
        assert!(!pane_ready_for_host_bytes(false));
    }

    #[test]
    fn stabilized_drop_filename_extension_rule() {
        assert_eq!(
            stabilized_drop_filename("/tmp/x.png", 1700000000000, "abc"),
            "drop-1700000000000-abc.png"
        );
        // Extensionless source falls back to png (mirrors url.pathExtension).
        assert_eq!(
            stabilized_drop_filename("/tmp/screenshot", 1, "u"),
            "drop-1-u.png"
        );
        // Leading-dot file has no extension (matches NSString.pathExtension).
        assert_eq!(
            stabilized_drop_filename("/tmp/.hidden", 2, "u"),
            "drop-2-u.png"
        );
    }
}
