//! Live terminal pane: a real VT emulator view over a session's PTY.
//!
//! Architectural rule (see repo AGENTS.md): the iOS session detail is a
//! **live terminal, never semantic chat**. This module is how the Dioxus
//! clients honor that — the same way the Swift client feeds the Host's raw
//! PTY byte stream into Ghostty, we feed it into a `vt100` parser and
//! render the resulting cell grid.
//!
//! Data flow (I/O stays in the app shell, per the crate convention):
//! - the shell long-polls `HostClient::output_chunk(offset, wait_ms)`,
//! - feeds each chunk into [`TerminalModel`],
//! - passes [`TerminalModel::snapshot`] down to [`TerminalView`],
//! - receives keystrokes up through `on_key` and forwards them to
//!   `HostClient::write`.

use dioxus::html::point_interaction::ModifiersInteraction;
use dioxus::html::Key;
use dioxus::prelude::*;
use std::borrow::Cow;
use vt100::Parser;

use super::clickable_path::PathClickRequest;
use super::find::FindSplitRuns;

/// Default viewport size for the mobile terminal.
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;
/// Scrollback lines kept in the parser.
pub const SCROLLBACK: usize = 1000;

/// Strips terminal *query request* sequences from remote output before it is
/// fed to the local `vt100` parser — a direct port of the iOS client's
/// `TerminalQueryFilter`.
///
/// The Host's real terminal already answered any queries embedded in the
/// output; if our local surface answered them too, its replies would be
/// routed upstream as spurious *input* (the Swift client's motivating bug:
/// a `XTVERSION` query's reply typed into the user's prompt on every
/// focus-driven replay). Queries carry no display content, so removing them
/// is invisible.
///
/// The filter is **stateful across chunks** (one instance per
/// [`TerminalModel`], like Swift's one-per-renderer): a query split across a
/// chunk boundary would otherwise pass through in two innocent-looking
/// halves, reassemble inside the parser, and get answered. An incomplete
/// trailing sequence that could still become a query is withheld and
/// prepended to the next chunk — display-equivalent, since the parser would
/// buffer the same bytes without rendering anything.
///
/// Stripped: CSI DA (`…c`), DSR (`…n`), XTVERSION (`CSI > … q`), DECRQM
/// (`CSI … $ p`), and DCS XTGETTCAP/DECRQSS requests (`ESC P +q…` / `$q…`).
/// `ESC c` (RIS) and DECSCUSR (`CSI … SP q`) are deliberately preserved.
#[derive(Debug, Default)]
pub struct TerminalQueryFilter {
    /// Withheld bytes that may still complete a query.
    carry: Vec<u8>,
    /// Inside an unterminated DCS query: discard bytes until ST/BEL.
    discarding_dcs_query: bool,
}

impl TerminalQueryFilter {
    /// A withheld (unterminated) CSI prefix longer than this is not a real
    /// query — drop it entirely rather than emitting a reassembly hazard.
    const MAX_CARRY_BYTES: usize = 96;

    /// Clears carried state. Call wherever the byte stream restarts from
    /// scratch (reset/clear replays), alongside [`TerminalModel::reset`].
    pub fn reset(&mut self) {
        self.carry.clear();
        self.discarding_dcs_query = false;
    }

    /// Strip query requests from one output chunk.
    pub fn strip_requests<'a>(&mut self, input: &'a [u8]) -> Cow<'a, [u8]> {
        if self.carry.is_empty() && !self.discarding_dcs_query && !input.contains(&0x1B) {
            return Cow::Borrowed(input);
        }
        let mut bytes = std::mem::take(&mut self.carry);
        bytes.extend_from_slice(input);
        let n = bytes.len();
        let mut out: Vec<u8> = Vec::with_capacity(n);
        let mut i = 0;
        if self.discarding_dcs_query {
            match Self::scan_dcs_terminator(&bytes, 0) {
                DcsScan::Terminated(end) => {
                    self.discarding_dcs_query = false;
                    i = end;
                }
                DcsScan::TrailingEsc => {
                    self.carry = vec![0x1B]; // split ST — hold the ESC for the next chunk
                    return Cow::Owned(Vec::new());
                }
                DcsScan::Exhausted => return Cow::Owned(Vec::new()), // whole chunk is query payload
            }
        }
        while i < n {
            let b = bytes[i];
            if b != 0x1B {
                out.push(b);
                i += 1;
                continue;
            }
            if i + 1 >= n {
                self.carry = vec![0x1B]; // lone trailing ESC — may begin a query
                break;
            }
            let next = bytes[i + 1];
            if next == 0x5B {
                // CSI: ESC [
                let mut j = i + 2;
                let mut priv_byte: Option<u8> = None;
                if j < n && (0x3C..=0x3F).contains(&bytes[j]) {
                    priv_byte = Some(bytes[j]);
                    j += 1;
                }
                let mut has_dollar = false;
                while j < n && !(0x40..=0x7E).contains(&bytes[j]) {
                    if bytes[j] == 0x24 {
                        has_dollar = true;
                    }
                    j += 1;
                }
                if j >= n {
                    // No final byte yet: withhold so a split query cannot
                    // reassemble in the surface. Oversized ⇒ not a query; drop.
                    if n - i <= Self::MAX_CARRY_BYTES {
                        self.carry = bytes[i..n].to_vec();
                    }
                    break;
                }
                let strip = match bytes[j] {
                    0x63 => true,                    // 'c' — Device Attributes
                    0x6E => true,                    // 'n' — Device Status Report
                    0x71 => priv_byte == Some(0x3E), // '>…q' — XTVERSION (not DECSCUSR)
                    0x70 => has_dollar,              // '…$p' — DECRQM (not '!p' DECSTR)
                    _ => false,
                };
                if strip {
                    i = j + 1;
                } else {
                    out.extend_from_slice(&bytes[i..=j]);
                    i = j + 1;
                }
            } else if next == 0x50 {
                // DCS: ESC P
                if i + 3 >= n {
                    // Too short to classify as '+q'/'$q' — withhold the tail.
                    self.carry = bytes[i..n].to_vec();
                    break;
                }
                let d0 = bytes[i + 2];
                let d1 = bytes[i + 3];
                let is_query = (d0 == 0x2B || d0 == 0x24) && d1 == 0x71; // '+q' / '$q'
                match Self::scan_dcs_terminator(&bytes, i + 2) {
                    DcsScan::Terminated(end) => {
                        if is_query {
                            i = end;
                        } else {
                            out.extend_from_slice(&bytes[i..end]);
                            i = end;
                        }
                    }
                    DcsScan::TrailingEsc => {
                        if is_query {
                            self.discarding_dcs_query = true;
                            self.carry = vec![0x1B]; // split ST — hold the ESC for the next chunk
                        } else {
                            out.extend_from_slice(&bytes[i..n]);
                        }
                        i = n;
                    }
                    DcsScan::Exhausted => {
                        if is_query {
                            self.discarding_dcs_query = true; // swallow payload chunk-by-chunk
                        } else {
                            out.extend_from_slice(&bytes[i..n]);
                        }
                        i = n;
                    }
                }
            } else {
                out.push(b); // ESC + other (e.g. RIS 'ESC c') — preserve
                i += 1;
            }
        }
        Cow::Owned(out)
    }

    fn scan_dcs_terminator(bytes: &[u8], from: usize) -> DcsScan {
        let n = bytes.len();
        let mut j = from;
        while j < n {
            if bytes[j] == 0x07 {
                return DcsScan::Terminated(j + 1); // BEL
            }
            if bytes[j] == 0x1B {
                if j + 1 >= n {
                    return DcsScan::TrailingEsc;
                }
                if bytes[j + 1] == 0x5C {
                    return DcsScan::Terminated(j + 2); // ESC \
                }
            }
            j += 1;
        }
        DcsScan::Exhausted
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DcsScan {
    Terminated(usize), // index just past BEL / ESC \
    TrailingEsc,       // chunk ends with a lone ESC (maybe split ST)
    Exhausted,         // no terminator in this chunk
}

/// A terminal color, mapped to CSS by [`TermColor::to_css`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermColor {
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

impl TermColor {
    fn from_vt(color: vt100::Color) -> Self {
        match color {
            vt100::Color::Default => Self::Default,
            vt100::Color::Idx(i) => Self::Idx(i),
            vt100::Color::Rgb(r, g, b) => Self::Rgb(r, g, b),
        }
    }

    /// CSS color value. `None` means "inherit the terminal default".
    pub fn to_css(self) -> Option<String> {
        match self {
            Self::Default => None,
            // Standard xterm 16-color palette.
            Self::Idx(i) => Some(
                [
                    "#000000", "#cd0000", "#00cd00", "#cdcd00", "#0000ee", "#cd00cd", "#00cdcd",
                    "#e5e5e5", "#7f7f7f", "#ff0000", "#00ff00", "#ffff00", "#5c5cff", "#ff00ff",
                    "#00ffff", "#ffffff",
                ][i as usize % 16]
                    .to_string(),
            ),
            Self::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        }
    }
}

/// Terminal default colors (xterm-like dark theme). The view container
/// sets these explicitly; they are also what `TermColor::Default` resolves
/// to when inverse video needs concrete colors to swap.
const DEFAULT_FG_CSS: &str = "#e5e5e5";
const DEFAULT_BG_CSS: &str = "#000000";

/// Adjacent cells with identical attributes, coalesced into one span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleRun {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl StyleRun {
    fn style_attr(&self) -> String {
        // Inverse video swaps foreground and background. It is NOT a
        // pixel inversion: `filter:invert(1)` would map red-on-black to
        // cyan-on-white instead of black-on-red.
        let (fg, bg) = if self.inverse {
            (self.bg, self.fg)
        } else {
            (self.fg, self.bg)
        };
        let mut parts = Vec::new();
        if self.inverse {
            // A `Default` side must resolve to the concrete theme color —
            // inheriting it would make the swap invisible (or wrong, when
            // only one side is explicit).
            let fg_css = fg.to_css().unwrap_or_else(|| DEFAULT_FG_CSS.to_string());
            let bg_css = bg.to_css().unwrap_or_else(|| DEFAULT_BG_CSS.to_string());
            parts.push(format!("color:{fg_css}"));
            parts.push(format!("background-color:{bg_css}"));
        } else {
            if let Some(css) = fg.to_css() {
                parts.push(format!("color:{css}"));
            }
            if let Some(css) = bg.to_css() {
                parts.push(format!("background-color:{css}"));
            }
        }
        if self.bold {
            parts.push("font-weight:bold".to_string());
        }
        if self.dim {
            parts.push("opacity:0.6".to_string());
        }
        if self.italic {
            parts.push("font-style:italic".to_string());
        }
        if self.underline {
            parts.push("text-decoration:underline".to_string());
        }
        parts.join(";")
    }
}

/// One rendered terminal row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalRow {
    pub runs: Vec<StyleRun>,
}

/// A renderable snapshot of the terminal screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSnapshot {
    pub rows: Vec<TerminalRow>,
    pub cursor: (u16, u16),
    pub cols: u16,
    pub rows_count: u16,
}

/// The emulator state owned by the app shell.
pub struct TerminalModel {
    parser: Parser,
    query_filter: TerminalQueryFilter,
}

impl TerminalModel {
    pub fn new(cols: u16, rows: u16) -> Self {
        let mut parser = Parser::new(rows, cols, SCROLLBACK);
        // Start with a sane default mode set; the PTY stream overrides.
        let _ = &mut parser;
        Self {
            parser,
            query_filter: TerminalQueryFilter::default(),
        }
    }

    /// Feed raw PTY bytes from an `output_chunk` into the emulator.
    /// Terminal query requests (DA/DSR/XTVERSION/DECRQM/DCS) are stripped
    /// first — the Host's real terminal already answered them, and our
    /// surface's replies would otherwise loop back as spurious input.
    pub fn feed(&mut self, data: &[u8]) {
        let filtered = self.query_filter.strip_requests(data);
        self.parser.process(&filtered);
    }

    /// Reset after a truncated chunk (the Host rotated the output log).
    pub fn reset(&mut self) {
        let (rows, cols) = self.parser.screen().size();
        self.parser = Parser::new(rows, cols, SCROLLBACK);
        self.query_filter.reset();
    }

    pub fn size(&self) -> (u16, u16) {
        self.parser.screen().size()
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let screen = self.parser.screen();
        let (rows_count, cols) = screen.size();
        let mut rows = Vec::with_capacity(rows_count as usize);
        for r in 0..rows_count {
            let mut runs: Vec<StyleRun> = Vec::new();
            let mut col: u16 = 0;
            while col < cols {
                let Some(cell) = screen.cell(r, col) else {
                    col += 1;
                    continue;
                };
                // Wide-character continuation cells carry no content and
                // take no extra width — skip only these. Every other cell
                // emits exactly its width, including content-less cells
                // (as a space), so column alignment is preserved and
                // background colors paint the full grid.
                if cell.is_wide_continuation() {
                    col += 1;
                    continue;
                }
                let run = StyleRun {
                    text: if cell.has_contents() {
                        cell.contents().to_string()
                    } else {
                        " ".to_string()
                    },
                    fg: TermColor::from_vt(cell.fgcolor()),
                    bg: TermColor::from_vt(cell.bgcolor()),
                    bold: cell.bold(),
                    dim: cell.dim(),
                    italic: cell.italic(),
                    underline: cell.underline(),
                    inverse: cell.inverse(),
                };
                let width = if cell.is_wide() { 2 } else { 1 };
                match runs.last_mut() {
                    Some(last)
                        if last.fg == run.fg
                            && last.bg == run.bg
                            && last.bold == run.bold
                            && last.dim == run.dim
                            && last.italic == run.italic
                            && last.underline == run.underline
                            && last.inverse == run.inverse =>
                    {
                        last.text.push_str(&run.text)
                    }
                    _ => runs.push(run),
                }
                col += width;
            }
            rows.push(TerminalRow { runs });
        }
        TerminalSnapshot {
            rows,
            cursor: screen.cursor_position(),
            cols,
            rows_count,
        }
    }
}

/// Map a keyboard event to the byte sequence the PTY expects.
pub fn key_to_sequence(key: &Key) -> Option<String> {
    match key {
        Key::Character(s) => Some(s.clone()),
        Key::Enter => Some("\r".to_string()),
        Key::Backspace => Some("\x7f".to_string()),
        Key::Tab => Some("\t".to_string()),
        Key::Escape => Some("\x1b".to_string()),
        Key::Delete => Some("\x1b[3~".to_string()),
        Key::ArrowUp => Some("\x1b[A".to_string()),
        Key::ArrowDown => Some("\x1b[B".to_string()),
        Key::ArrowRight => Some("\x1b[C".to_string()),
        Key::ArrowLeft => Some("\x1b[D".to_string()),
        Key::Home => Some("\x1b[H".to_string()),
        Key::End => Some("\x1b[F".to_string()),
        Key::PageUp => Some("\x1b[5~".to_string()),
        Key::PageDown => Some("\x1b[6~".to_string()),
        _ => None,
    }
}

/// Plain-text dump of the viewport: run texts concatenated per row, rows
/// joined with `\n`. Pure so selection/copy logic is unit-testable without
/// a renderer. This is what the text-selection sheet shows.
pub fn viewport_text(snapshot: &TerminalSnapshot) -> String {
    snapshot
        .rows
        .iter()
        .map(|row| {
            row.runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The "Copy All" text: like [`viewport_text`] but with trailing padding
/// stripped per row. The raw grid pads every row to the full column count;
/// copying that padding noise into a paste is never what the user wants.
/// Mirrors the iOS `TerminalTextSelectionSheet` Copy All behavior.
pub fn trimmed_viewport_text(snapshot: &TerminalSnapshot) -> String {
    snapshot
        .rows
        .iter()
        .map(|row| {
            let line: String = row.runs.iter().map(|run| run.text.as_str()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Word characters for long-press word anchoring: identifiers plus the
/// token characters terminals conventionally include, so paths, URLs,
/// `file:line` refs, and `KEY=value` pairs select whole. The anchor is a
/// starting point — the sheet's native handles stay adjustable.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '~' | '+' | '=' | '$')
}

/// UTF-16 offsets of the word under a viewport cell, for pre-selecting
/// the anchor word in the selection sheet's textarea (DOM selection
/// offsets are UTF-16 code units, like the Swift client's `NSRange`
/// anchor).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WordAnchor {
    pub start: usize,
    pub end: usize,
}

/// Long-press selection request: the word under the finger (`None` ⇒ the
/// press landed on blank space, so the sheet selects everything — the
/// Swift `anchorRange == nil` case), plus a nonce so repeat presses
/// re-open the sheet even with an identical anchor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SelectionRequest {
    pub anchor: Option<WordAnchor>,
    pub nonce: u64,
}

/// Resolve the word under the 0-based viewport cell `(row, col)` in
/// `text` (rows joined with `\n`, exactly as shown in the sheet).
/// Returns `None` when the cell is out of bounds or on a non-word
/// character. Wide cells may misalign `col` by one; the anchor stays
/// adjustable in the sheet.
pub fn word_anchor_at(text: &str, row: usize, col: usize) -> Option<WordAnchor> {
    let line = text.split('\n').nth(row)?;
    let chars: Vec<char> = line.chars().collect();
    if !chars.get(col).is_some_and(|c| is_word_char(*c)) {
        return None;
    }
    let mut start = col;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col + 1;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    // UTF-16 offsets into the full text: the row's byte offset plus the
    // char-boundary offsets within the row, counted in UTF-16 units.
    let row_byte: usize = text.split('\n').take(row).map(|l| l.len() + 1).sum();
    let row_utf16: usize = text[..row_byte].encode_utf16().count();
    let line_utf16 = |char_idx: usize| {
        line.chars()
            .take(char_idx)
            .map(|c| c.len_utf16())
            .sum::<usize>()
    };
    Some(WordAnchor {
        start: row_utf16 + line_utf16(start),
        end: row_utf16 + line_utf16(end),
    })
}

/// Text-selection sheet over a live terminal.
///
/// The terminal grid itself can't host native selection (the iOS client
/// re-materializes the viewport as a `UITextView` for the same reason), so
/// the viewport text is shown here in a readonly textarea — the webview
/// supplies the selection handles, magnifier, and edit menu. Copying goes
/// through `on_copy_all` (trimmed text) because clipboard access needs the
/// launcher's JS eval; the sheet stays portable.
#[component]
pub fn TextSelectionSheet(
    text: String,
    #[props(default)] anchor: Option<WordAnchor>,
    on_close: EventHandler<()>,
    on_copy_all: EventHandler<String>,
) -> Element {
    let trimmed = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    // Apply the anchor selection once the textarea is mounted: the webview
    // supplies the selection handles, magnifier, and edit menu from here.
    // `None` (long-press on blank space, or the manual Select button)
    // selects everything — the Swift `anchorRange == nil` case.
    let mut anchor_applied = use_signal(|| false);
    use_effect(move || {
        if *anchor_applied.read() {
            return;
        }
        anchor_applied.set(true);
        let selection = match anchor {
            Some(a) => format!("ta.setSelectionRange({}, {});", a.start, a.end),
            None => "ta.select();".to_string(),
        };
        let js = format!(
            r#"(function() {{
                var ta = document.querySelector('.text-selection-sheet .text-selection-text');
                if (!ta) return;
                ta.focus({{ preventScroll: true }});
                {selection}
            }})()"#
        );
        let _ = dioxus::document::eval(&js);
    });
    rsx! {
        div { class: "text-selection-sheet",
            div { class: "text-selection-bar",
                span { class: "text-selection-title", "Select text" }
                button {
                    class: "text-selection-copy",
                    onclick: move |_| on_copy_all.call(trimmed.clone()),
                    "Copy All"
                }
                button {
                    class: "text-selection-close",
                    onclick: move |_| on_close.call(()),
                    "Close"
                }
            }
            textarea {
                class: "text-selection-text",
                readonly: true,
                value: "{text}",
            }
        }
    }
}

/// Provisional keystrokes to render over the terminal grid, mosh-style:
/// underlined and cell-aligned at the anchor cell, with the terminal
/// background masking the stale frame underneath. Built by the launcher
/// from [`KeystrokePredictionEngine`] — `None` while the confidence gate
/// is closed. Mirrors the Swift client's prediction overlay view.
#[derive(Clone, PartialEq, Debug)]
pub struct PredictionOverlay {
    /// 0-based viewport row of the first provisional character.
    pub row: i32,
    /// 0-based viewport column of the first provisional character.
    pub col: i32,
    /// The provisional characters (from the engine's `displayed_text`).
    pub text: String,
}

/// The live terminal view: renders the emulator grid, captures keystrokes.
///
/// This is the iOS session detail. It is deliberately not a chat UI —
/// what you see is the session's actual PTY, the same bytes Ghostty
/// renders in the Swift client.
#[component]
pub fn TerminalView(
    snapshot: TerminalSnapshot,
    on_key: EventHandler<String>,
    on_copy_all: Option<EventHandler<String>>,
    #[props(default)] prediction: Option<PredictionOverlay>,
    /// Long-press selection request from the launcher: when the nonce
    /// changes, the sheet opens with the anchored word pre-selected.
    #[props(default)]
    selection_request: Option<SelectionRequest>,
    /// Find highlight (see `find.rs`): matches to highlight in the grid plus
    /// the current one. `None` (default) renders plain text exactly as
    /// before.
    #[props(default)]
    find: Option<super::find::FindHighlight>,
    /// Modifier-click (Cmd/Ctrl) on a terminal row (see `clickable_path.rs`).
    /// Reports the row index; the launcher matches the row text and acts
    /// only on exactly one path-like token.
    #[props(default)]
    on_path_click: Option<EventHandler<PathClickRequest>>,
) -> Element {
    let (cursor_row, cursor_col) = snapshot.cursor;
    let mut selecting = use_signal(|| false);
    let mut sheet_anchor = use_signal(|| None::<WordAnchor>);
    let mut applied_nonce = use_signal(|| 0u64);
    use_effect(move || {
        if let Some(req) = selection_request {
            if req.nonce != *applied_nonce.read() {
                applied_nonce.set(req.nonce);
                sheet_anchor.set(req.anchor);
                selecting.set(true);
            }
        }
    });
    let viewport = viewport_text(&snapshot);
    // Find highlight: per-row (start, end, is_current) char ranges, so the
    // renderer can split runs at match boundaries.
    let find_rows: Vec<Vec<(usize, usize, bool)>> = match find.as_ref() {
        Some(hl) => {
            let mut per_row = vec![Vec::new(); snapshot.rows.len()];
            for (mi, m) in hl.matches.iter().enumerate() {
                if let Some(row_ranges) = per_row.get_mut(m.row) {
                    row_ranges.push((m.col, m.col + m.len, Some(mi) == hl.selected));
                }
            }
            per_row
        }
        None => Vec::new(),
    };
    // Pre-split every run at match boundaries — only when a find is active.
    // The inactive hot path renders exactly as before with no extra clones.
    let find_split: FindSplitRuns = match find.as_ref() {
        Some(_) => snapshot
            .rows
            .iter()
            .enumerate()
            .map(|(r, row)| {
                let row_ranges: &[(usize, usize, bool)] =
                    find_rows.get(r).map(Vec::as_slice).unwrap_or(&[]);
                let mut run_start = 0usize;
                row.runs
                    .iter()
                    .map(|run| {
                        let segs =
                            super::find::split_run_for_find(&run.text, run_start, row_ranges);
                        run_start += run.text.chars().count();
                        (run.style_attr(), segs)
                    })
                    .collect()
            })
            .collect(),
        None => Vec::new(),
    };
    let find_active = find.is_some();
    let path_handler = on_path_click;
    rsx! {
        div {
            class: "terminal-view",
            "data-testid": "terminal-view",
            // Explicit xterm-like dark theme: Default fg/bg resolve against
            // these, so inverse spans (which pin the same values) match.
            style: "color:{DEFAULT_FG_CSS};background-color:{DEFAULT_BG_CSS};",
            tabindex: 0,
            // Keep focus so keystrokes reach the PTY without tapping first.
            autofocus: true,
            onkeydown: move |e| {
                if let Some(seq) = key_to_sequence(&e.key()) {
                    e.prevent_default();
                    on_key.call(seq);
                }
            },
            if on_copy_all.is_some() {
                button {
                    class: "terminal-select-btn",
                    onclick: move |_| {
                        // Manual open: no anchor, so the sheet selects
                        // everything (the Swift nil-anchor case).
                        sheet_anchor.set(None);
                        selecting.set(true);
                    },
                    "Select"
                }
            }
            for (r, row) in snapshot.rows.iter().enumerate() {
                div {
                    key: "{r}",
                    class: "terminal-row",
                    style: "position:relative;",
                    onclick: move |e: MouseEvent| {
                        let Some(handler) = path_handler else { return };
                        let mods = e.modifiers();
                        if mods.meta() || mods.ctrl() {
                            // The launcher owns the snapshot: it matches
                            // the row text and fires only on exactly one
                            // path-like token.
                            handler.call(PathClickRequest { row: r });
                        }
                    },
                    if find_active {
                        // Highlight spans flow inline with the text, so no
                        // column math is needed even for wide chars.
                        if let Some(splits) = find_split.get(r) {
                            for (i, (style, segs)) in splits.iter().enumerate() {
                                for (j, (seg_text, hl)) in segs.iter().enumerate() {
                                    span {
                                        key: "{i}-{j}",
                                        class: match hl {
                                            Some(true) => "find-match-current",
                                            Some(false) => "find-match",
                                            None => "",
                                        },
                                        style: "{style}",
                                        "{seg_text}"
                                    }
                                }
                            }
                        }
                    } else {
                        for (i, run) in row.runs.iter().enumerate() {
                            span {
                                key: "{i}",
                                style: "{run.style_attr()}",
                                "{run.text}"
                            }
                        }
                    }
                    // Keep empty rows at full height.
                    if row.runs.is_empty() {
                        span { " " }
                    }
                    // Block cursor, absolutely positioned at the cursor cell.
                    if r as u16 == cursor_row {
                        span {
                            class: "terminal-cursor",
                            style: "position:absolute;top:0;left:{cursor_col}ch;width:1ch;",
                            " "
                        }
                    }
                    // Predictive echo overlay: provisional keystrokes,
                    // underlined per the Swift client, masking the stale
                    // cells beneath with the terminal background.
                    if let Some(overlay) = prediction.as_ref() {
                        if r as i32 == overlay.row {
                            span {
                                class: "terminal-prediction",
                                style: "position:absolute;top:0;left:{overlay.col}ch;text-decoration:underline;background-color:{DEFAULT_BG_CSS};",
                                "{overlay.text}"
                            }
                        }
                    }
                }
            }
            if *selecting.read() {
                if let Some(on_copy) = on_copy_all {
                    TextSelectionSheet {
                        text: viewport,
                        anchor: *sheet_anchor.read(),
                        on_close: move |_| selecting.set(false),
                        on_copy_all: move |t: String| on_copy.call(t),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_anchor_selects_word_under_finger() {
        let text = "hello world";
        let a = word_anchor_at(text, 0, 7).unwrap();
        assert_eq!((a.start, a.end), (6, 11));
        assert_eq!(&text[a.start..a.end], "world");
    }

    #[test]
    fn word_anchor_expands_over_path_chars() {
        let text = "open /usr/local/bin now";
        let a = word_anchor_at(text, 0, 8).unwrap();
        assert_eq!(&text[a.start..a.end], "/usr/local/bin");
        let b = word_anchor_at(text, 0, 2).unwrap();
        assert_eq!(&text[b.start..b.end], "open");
    }

    #[test]
    fn word_anchor_covers_file_line_refs() {
        let text = "error in main.rs:123";
        let a = word_anchor_at(text, 0, 12).unwrap();
        assert_eq!(&text[a.start..a.end], "main.rs:123");
    }

    #[test]
    fn word_anchor_none_on_blank_or_out_of_bounds() {
        let text = "hi there";
        assert_eq!(word_anchor_at(text, 0, 2), None); // the space
        assert_eq!(word_anchor_at(text, 0, 40), None); // past the row
        assert_eq!(word_anchor_at(text, 5, 0), None); // past the text
        assert_eq!(word_anchor_at("", 0, 0), None);
    }

    #[test]
    fn word_anchor_offsets_span_rows_in_utf16() {
        // Row prefix "ab\n" is 3 UTF-16 units; the emoji is 2.
        let text = "ab\n😀xy z";
        let a = word_anchor_at(text, 1, 2).unwrap();
        // chars: 😀(0) x(1) y(2); word "xy" starts at char 1 → UTF-16 3+2.
        assert_eq!((a.start, a.end), (5, 7));
        let b = word_anchor_at(text, 0, 0).unwrap();
        assert_eq!((b.start, b.end), (0, 2));
        // The emoji itself is not a word char.
        assert_eq!(word_anchor_at(text, 1, 0), None);
    }

    #[test]
    fn renders_text_and_sgr_colors() {
        let mut model = TerminalModel::new(20, 4);
        model.feed(b"hello \x1b[31mred\x1b[0m world");
        let snap = model.snapshot();
        assert_eq!(snap.rows_count, 4);
        let row = &snap.rows[0];
        let texts: Vec<&str> = row.runs.iter().map(|r| r.text.as_str()).collect();
        // The full grid paints: trailing blank cells are trailing spaces.
        assert_eq!(texts, vec!["hello ", "red", " world     "]);
        assert_eq!(row.runs[0].fg, TermColor::Default);
        assert_eq!(row.runs[1].fg, TermColor::Idx(1));
    }

    #[test]
    fn cursor_tracking() {
        let mut model = TerminalModel::new(20, 4);
        model.feed(b"\x1b[2;5H");
        let snap = model.snapshot();
        assert_eq!(snap.cursor, (1, 4));
    }

    #[test]
    fn key_mapping() {
        assert_eq!(key_to_sequence(&Key::Enter), Some("\r".to_string()));
        assert_eq!(key_to_sequence(&Key::Backspace), Some("\x7f".to_string()));
        assert_eq!(key_to_sequence(&Key::ArrowUp), Some("\x1b[A".to_string()));
        assert_eq!(
            key_to_sequence(&Key::Character("a".to_string())),
            Some("a".to_string())
        );
        assert_eq!(key_to_sequence(&Key::Shift), None);
    }

    #[test]
    fn reset_clears_screen() {
        let mut model = TerminalModel::new(20, 4);
        model.feed(b"hello");
        model.reset();
        let snap = model.snapshot();
        assert!(snap
            .rows
            .iter()
            .all(|r| r.runs.iter().all(|run| run.text.trim().is_empty())));
    }

    #[test]
    fn viewport_text_joins_rows() {
        let mut model = TerminalModel::new(10, 3);
        model.feed(b"hi\r\nbye");
        let snap = model.snapshot();
        let text = viewport_text(&snap);
        let lines: Vec<&str> = text.lines().collect();
        // Full grid rows keep their padding in the raw dump.
        assert_eq!(lines[0], "hi        ");
        assert_eq!(lines[1], "bye       ");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn trimmed_viewport_text_strips_padding() {
        let mut model = TerminalModel::new(10, 3);
        model.feed(b"hi\r\nbye");
        let snap = model.snapshot();
        let text = trimmed_viewport_text(&snap);
        // Trailing padding stripped per row — the Copy All semantics.
        assert_eq!(text, "hi\nbye\n");
    }

    #[test]
    fn blank_cells_preserve_columns() {
        let mut model = TerminalModel::new(20, 4);
        // `a`, cursor 5 right, `b` — the gap cells were never written.
        model.feed(b"a\x1b[5Cb");
        let snap = model.snapshot();
        let text: String = snap.rows[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text.len(), 20);
        assert_eq!(&text[..7], "a     b");
    }

    #[test]
    fn background_paints_empty_cells() {
        let mut model = TerminalModel::new(20, 4);
        model.feed(b"\x1b[41m\x1b[2J");
        let snap = model.snapshot();
        let row = &snap.rows[0];
        assert!(!row.runs.is_empty(), "cleared screen still paints bg");
        assert_eq!(row.runs[0].bg, TermColor::Idx(1));
        assert!(row.runs[0].text.trim().is_empty());
    }

    #[test]
    fn wide_chars_occupy_two_columns() {
        let mut model = TerminalModel::new(20, 4);
        model.feed("あa".as_bytes());
        let snap = model.snapshot();
        let text: String = snap.rows[0].runs.iter().map(|r| r.text.as_str()).collect();
        // No phantom space from the wide continuation cell; `a` follows
        // the wide char directly, then trailing blanks fill the grid.
        assert!(text.starts_with("あa"));
        assert_eq!(text.chars().count(), 19); // あ + a + 17 spaces
                                              // Wide char took columns 0-1, `a` is at column 2, cursor at 3.
        assert_eq!(snap.cursor, (0, 3));
    }

    #[test]
    fn combining_chars_stay_in_one_cell() {
        let mut model = TerminalModel::new(20, 4);
        model.feed("e\u{301}".as_bytes());
        let snap = model.snapshot();
        let text: String = snap.rows[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert!(text.starts_with("e\u{301}"));
        assert_eq!(snap.cursor, (0, 1));
    }

    #[test]
    fn escape_split_across_feeds() {
        let mut model = TerminalModel::new(20, 4);
        model.feed(b"\x1b[");
        model.feed(b"31mX");
        let snap = model.snapshot();
        let row = &snap.rows[0];
        assert_eq!(row.runs[0].text, "X");
        assert_eq!(row.runs[0].fg, TermColor::Idx(1));
    }

    #[test]
    fn scrolling_keeps_last_rows() {
        let mut model = TerminalModel::new(20, 4);
        for i in 0..10 {
            model.feed(format!("l{i}\n").as_bytes());
        }
        let snap = model.snapshot();
        assert_eq!(snap.rows_count, 4);
        // The trailing newline scrolled once more: l9 sits on row 2.
        let text: String = snap.rows[2].runs.iter().map(|r| r.text.as_str()).collect();
        assert!(text.contains("l9"), "l9 visible after scroll: {text:?}");
        let first: String = snap.rows[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert!(first.contains("l7"), "l0..l6 scrolled off: {first:?}");
    }

    #[test]
    fn inverse_swaps_fg_bg() {
        let mut model = TerminalModel::new(20, 4);
        model.feed(b"\x1b[31;7mX");
        let snap = model.snapshot();
        let run = &snap.rows[0].runs[0];
        assert!(run.inverse);
        assert_eq!(run.fg, TermColor::Idx(1));
        // Rendered: red fg becomes the background, default fg becomes text.
        let style = run.style_attr();
        assert!(
            style.contains("background-color:#cd0000"),
            "red moved to bg: {style}"
        );
        assert!(
            style.contains("color:#e5e5e5"),
            "default fg resolved to theme: {style}"
        );
        assert!(!style.contains("invert"), "no CSS filter hack: {style}");
    }
}

#[cfg(test)]
mod query_filter_tests {
    use super::*;

    fn strip(f: &mut TerminalQueryFilter, chunks: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for c in chunks {
            out.extend_from_slice(&f.strip_requests(c));
        }
        out
    }

    #[test]
    fn plain_text_passes_through_untouched() {
        let mut f = TerminalQueryFilter::default();
        let data = b"hello world\r\n$ ";
        assert_eq!(strip(&mut f, &[data]), data);
    }

    #[test]
    fn device_attributes_stripped() {
        let mut f = TerminalQueryFilter::default();
        // ESC[c (DA) — the query whose reply Swift saw typed into prompts.
        assert_eq!(strip(&mut f, &[b"a\x1b[c" as &[u8], b"b"]), b"ab");
    }

    #[test]
    fn dsr_stripped() {
        let mut f = TerminalQueryFilter::default();
        assert_eq!(strip(&mut f, &[b"\x1b[0n" as &[u8]]), b"");
    }

    #[test]
    fn xtversion_stripped_but_decscusr_preserved() {
        let mut f = TerminalQueryFilter::default();
        // CSI > 0 q — XTVERSION request: strip.
        assert_eq!(strip(&mut f, &[b"\x1b[>0q" as &[u8]]), b"");
        // CSI 0 SP q — DECSCUSR (cursor style): preserve.
        assert_eq!(strip(&mut f, &[b"_\x1b[0 q+" as &[u8]]), b"_\x1b[0 q+");
    }

    #[test]
    fn decrqm_stripped_but_decstr_preserved() {
        let mut f = TerminalQueryFilter::default();
        // CSI ? 1 $ p — DECRQM: strip.
        assert_eq!(strip(&mut f, &[b"\x1b[?1$p" as &[u8]]), b"");
        // CSI ! p — DECSTR: preserve (final 'p' without '$').
        assert_eq!(strip(&mut f, &[b"_\x1b[!p" as &[u8]]), b"_\x1b[!p");
    }

    #[test]
    fn ris_preserved() {
        let mut f = TerminalQueryFilter::default();
        assert_eq!(strip(&mut f, &[b"a\x1bcb" as &[u8]]), b"a\x1bcb");
    }

    #[test]
    fn dcs_xtgettcap_stripped_through_bel() {
        let mut f = TerminalQueryFilter::default();
        assert_eq!(strip(&mut f, &[b"x\x1bP+q544e\x07y" as &[u8]]), b"xy");
    }

    #[test]
    fn dcs_non_query_preserved() {
        let mut f = TerminalQueryFilter::default();
        // DCS without +q/$q is not a query — pass through.
        assert_eq!(
            strip(&mut f, &[b"\x1bP1$r0\x07" as &[u8]]),
            b"\x1bP1$r0\x07"
        );
    }

    #[test]
    fn split_query_across_chunks_cannot_reassemble() {
        let mut f = TerminalQueryFilter::default();
        // First chunk ends mid-CSI: withheld, nothing emitted.
        assert_eq!(strip(&mut f, &[b"a\x1b[" as &[u8]]), b"a");
        // Second chunk completes the DA query: stripped, never surfaced.
        assert_eq!(strip(&mut f, &[b"c" as &[u8]]), b"");
        // Stream continues normally afterwards.
        assert_eq!(strip(&mut f, &[b"b" as &[u8]]), b"b");
    }

    #[test]
    fn unterminated_dcs_query_swallowed_across_chunks() {
        let mut f = TerminalQueryFilter::default();
        assert_eq!(strip(&mut f, &[b"\x1bP+q" as &[u8]]), b"");
        assert_eq!(strip(&mut f, &[b"deadbeef" as &[u8]]), b"");
        // Terminator ends the discard; trailing content flows.
        assert_eq!(strip(&mut f, &[b"more\x07ok" as &[u8]]), b"ok");
    }

    #[test]
    fn oversized_carry_dropped_not_emitted() {
        let mut f = TerminalQueryFilter::default();
        let big = vec![b'0'; 200];
        let mut chunk = vec![0x1B, 0x5B];
        chunk.extend_from_slice(&big);
        // Withheld prefix exceeds MAX_CARRY_BYTES: dropped, not emitted.
        assert_eq!(strip(&mut f, &[&chunk]), b"");
        // Filter recovers: normal text flows again.
        assert_eq!(strip(&mut f, &[b"z" as &[u8]]), b"z");
    }

    #[test]
    fn reset_clears_withheld_state() {
        let mut f = TerminalQueryFilter::default();
        assert_eq!(strip(&mut f, &[b"\x1b[" as &[u8]]), b"");
        f.reset();
        // The withheld ESC [ is gone; a bare 'c' is now just text.
        assert_eq!(strip(&mut f, &[b"c" as &[u8]]), b"c");
    }

    #[test]
    fn model_feed_strips_queries_before_parse() {
        let mut m = TerminalModel::new(80, 24);
        m.feed(b"hi\x1b[c");
        let snap = m.snapshot();
        let first_row: String = snap.rows[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert!(
            first_row.starts_with("hi"),
            "query stripped, text kept: {first_row:?}"
        );
        assert!(
            !first_row.contains('\u{1b}'),
            "no escape residue in cells: {first_row:?}"
        );
    }
}
