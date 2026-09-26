# gpuidart API Gaps (consolidated — for Amein)

**Maintained by:** docs parity worker (`track-b-parity-docs-worker`)
**Last updated:** 2026-09-26
**Source of truth for terminal gaps:** `docs/gpuidart-gaps-terminal.md` (incorporated below)

This file consolidates every gpuidart framework API gap that blocks supercli
desktop parity. Each gap lists: the missing API, the feature it blocks, and
the proposal-patch status.

**Proposal-patch pattern** (established by P0-8, 2026-09-26):
1. Framework change is developed against the `clients/gpuidart` submodule, then
   **reverted** — the submodule must always point at an upstream commit that
   exists on GitHub (currently `135d300`), so fresh recursive clones work.
2. The change is exported as `docs/internal/proposals/gpuidart-pNN-<name>.patch`
   via `git format-patch` against the upstream base.
3. supercli-app vendors any Dart wire types it needs under
   `clients/supercli-app/lib/` so it keeps building against upstream gpuidart.
4. supercli-app ships a fallback renderer until Amein merges the proposal.

**Rule:** never modify `clients/gpuidart/` in a feature branch; never push to
`ameineskinder/gpuidart`; never request access. Log the gap here instead.

---

## Terminal (P0-8) — 11 gaps, proposal patch shipped

**Proposal:** `docs/internal/proposals/gpuidart-p08-uiterminal.patch`
(git format-patch against upstream `135d300`; applies to `clients/gpuidart`)
**Vendored types:** `clients/supercli-app/lib/terminal/terminal_types.dart`
**Current rendering:** RLE fallback (`UiRow`/`UiText`) via `TerminalPane`
**Blocks:** [DESKTOP] rows 169 (terminal surfaces), 170 (remote-host panes),
177 (find bar), 179/180 (URL/OSC-8 links, ⌘-click paths), 185 (viewer presence
in pane), 187–189 (gallery/screenshot in pane)

| # | Missing API | Blocks | Status |
|---|-------------|--------|--------|
| G-1 | `UiStyle.fontFamily` (or `UiFontFamily` token) honored by the native text renderer | Monospace guarantee — a terminal grid is meaningless without it | In P0-8 proposal |
| G-2 | Per-cell text styling: `UiTextSpan`-style rich text node, or native `terminal` kind renderer; plus a damage API (no full-tree rebuild per frame) | True grid rendering; 60fps damage-only redraw | In P0-8 proposal (`UiTerminal` node) |
| G-3 | `UiStyle` text decoration (underline/strikethrough) and italic | SGR underline/italic rendering | In P0-8 proposal |
| G-4 | Custom painting / canvas node (`UiCanvas`/`UiCustomPaint`), or native `terminal` renderer | Cursor shapes (underline/bar), ligature-free glyph grid | In P0-8 proposal |
| G-5 | Raw key-event stream (`KeyDown`/`KeyUp` with key, code, modifiers, text) as `GpuiEvent`s — `UiAction` rejects bare letters/digits | ALL terminal keyboard input (typing `ls` is impossible today) | Proposed, not yet patched |
| G-6 | IME events (preedit, commit) on the key-event stream | CJK input, dead keys, compose sequences | Proposed, not yet patched |
| G-7 | Size observation: layout callback `onLayout(id, w, h)` or `host.measure(nodeId)` | PTY resize → SIGWINCH (cols×rows from window size) | Proposed, not yet patched |
| G-8 | Pointer events (`onPointerDown/Move/Up`, scroll deltas) with node-local coordinates | Mouse reporting (SGR 1006), mouse text selection | Proposed, not yet patched |
| G-9 | Bracketed paste event (or clipboard API) | `\x1b[200~...\x1b[201~` paste wrapping | Proposed, not yet patched |
| G-10 | Scrollback virtualization (or native `terminal` renderer) | 10k-line scrollback (fallback renders every row as nodes) | In P0-8 proposal |
| G-11 | Clipboard read API (copy works via `onCopy`, paste doesn't) | Bracketed paste (see G-9) | Proposed, not yet patched |

**What the native renderer must implement for full P0-8** (spec = `UiTerminal`
JSON contract in the proposal patch):
1. Monospace glyph grid from `cells` (respect `fontFamily`, `fontSize`, `lineHeight`)
2. Per-cell fg/bg from theme-resolved colors (256 + truecolor)
3. `TerminalDamage` ranges → dirty-cell redraws at vsync
4. Cursor: block/underline/bar, blinking/steady
5. Selection overlay
6. Scrollback buffer with wheel + scrollbar
7. Raw key events (G-5), IME (G-6), mouse SGR 1006 (G-8), bracketed paste (G-9), resize → `onResize` (G-7)

---

## Sidebar — pending worker (a) report

**Blocks:** [DESKTOP] rows 150 (vibrancy sidebar), 151 (project/session tree),
152 (detached drag), 153/154 (context menus), 156 (archived sessions),
157 (workspace dots/swipe), 160 (right-side panel), 219 (startup skeleton)

Candidate gaps (to be confirmed by worker (a)):
- Drag-and-drop API for reordering sessions/projects and drop-to-split
- Context-menu API (right-click menus on tree rows)
- Vibrancy/translucency material for the sidebar background
- Trackpad swipe gesture events for workspace switching

*No proposal patches yet. Worker (a) will file them here.*

---

## Command palette / keyboard (Cmd-K, Ctrl-Tab) — pending worker (b) report

**Blocks:** [DESKTOP] rows 164 (⌘K palette), 165 (⌃Tab MRU switcher),
166 (⌘1–9 / ⌃1–9 switching)

Candidate gaps (to be confirmed by worker (b)):
- Global keybinding registration (⌘K, ⌃Tab, ⌘1–9) independent of focus
- Held-key hint overlay while number keys are held

*No proposal patches yet. Worker (b) will file them here.*

---

## Approvals + notifications — pending worker (c) report

**Blocks:** [DESKTOP] rows 167 (recent activity), 168 (toasts), 186 (approval
overlay — DONE via in-Dart overlay, no gap), 195 (Notification Center)

Candidate gaps (to be confirmed by worker (c)):
- Native notification banner API (macOS Notification Center)
- Toast overlay positioning API

*No proposal patches yet. Worker (c) will file them here.*

---

## Settings — pending worker (d) report

**Blocks:** [DESKTOP] rows 200–216 (all Settings sections)

Candidate gaps (to be confirmed by worker (d)):
- Form control set (toggles, segmented pickers, color wells) if missing from gpuidart widgets

*No proposal patches yet. Worker (d) will file them here.*

---

## Panes (split, zoom, focus) — pending worker (e) report

**Blocks:** [DESKTOP] rows 171 (split), 172 (zoom/equalize/spatial focus),
173 (detach), 174 (pane header menu), 175/176 (launcher pane, persisted layouts)

Candidate gaps (to be confirmed by worker (e)):
- Split-container layout API (recursive tree, up to 8 panes)
- Spatial focus navigation hooks

*No proposal patches yet. Worker (e) will file them here.*

---

## Apps (Git, Files, Markdown, Usage on gpuidart) — pending worker (f) report

**Blocks:** [DESKTOP] none directly; [APPS] rows 134–143 (Git/Files/Markdown/
Usage apps are Rust TUI today — gpuidart renderers are future work)

*No proposal patches yet. Worker (f) will file them here.*

---

## Web — pending worker (g) report

**Blocks:** [WEB] rows 147 (App Kit web renderer), 149 (help/docs/download links)

*Worker (g) will file gaps here (likely none — web renderer is TypeScript/DOM,
not gpuidart).*
