# gpuidart Gaps for the P0-8 Terminal Pane (for Amein)

**Date:** 2026-09-26
**Branch:** `track-b-terminal-pane`
**Status:** Real implementation where gpuidart allows; gaps logged below.

## PROPOSAL FOR AMEIN (2026-09-26 update)

The `UiTerminal` framework work has been **reverted from the submodule** and exported
as a proposal patch. Rationale: the submodule commit existed only in the build VM;
fresh clones would fail.

- **Proposal patch:** `docs/internal/proposals/gpuidart-p08-uiterminal.patch`
  (git format-patch against upstream `135d300`; applies to `clients/gpuidart`)
- **Vendored types:** `clients/supercli-app/lib/terminal/terminal_types.dart`
  (copied from the proposal so supercli-app builds against upstream gpuidart)
- **Current rendering:** RLE fallback (`UiRow`/`UiText`) via `TerminalPane.buildFallback`
- **When you ship P0-8:** delete `terminal_types.dart`, import from `package:gpuidart`,
  and restore `TerminalState.buildNode` → `UiTerminal` (see patch for the API)

No push to `ameineskinder/gpuidart` was made; no access was requested.

## What was built

- `clients/gpuidart/lib/src/terminal.dart` — `UiTerminal` wire types:
  `TerminalColor` (ANSI 0-255 palette + 24-bit truecolor), `TerminalTheme`
  (16 ANSI + chrome colors, xterm-correct 256-color resolution),
  `TerminalCell`, `TerminalCursor`, `CursorStyle`, `TerminalSelection`,
  `TerminalDamage`.
- `clients/gpuidart/lib/src/nodes.dart` — `UiTerminal extends UiNode` per the
  P0-8 API sketch (immutable snapshot node; callbacks travel via the host
  event stream, not on the node).
- `clients/supercli-app/lib/terminal/terminal_state.dart` — mutable
  `TerminalState`: grid, scrollback (capped), cursor, selection;
  `updateCells` (applies host damage, returns damage ranges),
  `setCursor`, `scrollTo`, `select`/`copySelection`, `buildNode`.
- `clients/supercli-app/lib/terminal/terminal_pane.dart` — `TerminalPane`
  widget: builds the `UiTerminal` node + a fallback renderer using
  UiRow/UiText with run-length-encoded color runs (real ANSI 256 +
  truecolor today). `TerminalKeymap` maps special keys to PTY escape
  sequences; `handleKeyAction` forwards bytes via `onInput`.
- `clients/supercli-app/test/terminal_pane_test.dart` — 34 behavior tests,
  all passing.
- Proof screenshot: `docs/internal/proofs/proof-screenshots/terminal-pane-p08.png`
  (rendered from a real JSON snapshot: ANSI colors, truecolor, selection,
  block cursor, 256-color cube).

## Gaps — what gpuidart cannot do today (P0-8 requirements)

### G-1. No `fontFamily` in UiStyle (BLOCKS monospace guarantee)
`UiStyle` has `fontSize` and `fontWeight` but no font family field.
`UiTerminal` carries `fontFamily` in its JSON, but the native renderer has
nothing to apply it to — it falls back to its default font, which may not
be monospace. A terminal grid is meaningless without a monospace font.
**Needed:** `UiStyle(fontFamily: String?)` or a `UiFontFamily` token, honored
by the native text renderer.

### G-2. No per-cell text styling (BLOCKS true grid rendering)
`UiText` carries ONE foreground/background for its whole string. There is no
way to style individual characters. The fallback works around this with
run-length encoding (consecutive same-attribute cells → one UiText), but:
- Every attribute change splits a node; a `htop`-style screen becomes
  hundreds of nodes per frame.
- There is no damage API: the app rebuilds the entire tree and republishes
  on every update. 60fps damage-only redraw is impossible through the
  snapshot protocol as it stands.
**Needed:** either a native `terminal` kind renderer (the `UiTerminal` node
is designed for this), or a `UiTextSpan`-style rich text node.

### G-3. No text decoration or italic in UiStyle
`UiStyle` has no underline/strikethrough/italic fields. SGR underline and
italic are tracked in `TerminalCell` but cannot render in the fallback.
**Needed:** `UiStyle(textDecoration: ..., fontStyle: ...)` or equivalent.

### G-4. No custom painting / canvas (BLOCKS cursor shapes)
Block cursor is approximated by inverting the cell; underline/bar cursors
are approximated with bold (there is no primitive to draw a 2px bar or a
1px underline at the cell edge). Ligature-free glyph-grid rendering needs
a canvas or a native terminal surface.
**Needed:** a `UiCanvas`/`UiCustomPaint` node, or the native `terminal`
renderer for `UiTerminal`.

### G-5. No raw keyboard input (BLOCKS terminal input)
`UiAction` explicitly rejects bare letters and digits: "native text input
and IME own them." A terminal needs EVERY key (letters, digits, punctuation
with exact modifiers, dead keys, compose sequences). `TerminalKeymap` covers
special keys (arrows, F-keys, enter/tab/backspace/escape, ctrl+letter), but
typing `ls` into the terminal is impossible through `UiAction`.
**Needed:** a raw key-event stream (`KeyDown`/`KeyUp` with `key`, `code`,
modifiers, `text`) delivered as `GpuiEvent`s, independent of focus widgets.

### G-6. No IME / compose support
No IME API is exposed. CJK input, dead keys, and compose sequences cannot
work.
**Needed:** IME events (preedit string, commit string) on the key-event
stream from G-5.

### G-7. No size observation (BLOCKS PTY resize)
There is no API to learn a node's pixel size, so the app cannot compute
cols×rows from the window or fire `onResize` → SIGWINCH. The P0-8 `onResize`
callback exists on `TerminalPane` but nothing can call it.
**Needed:** a layout callback (`onLayout(id, width, height)`) or a
`host.measure(nodeId)` query.

### G-8. No mouse events on arbitrary nodes (BLOCKS SGR 1006)
No click/drag/scroll/motion events are exposed for nodes. Mouse reporting
(SGR 1006: click, drag, scroll, motion) and text selection by mouse are
impossible. Selection is currently keyboard/API-driven only.
**Needed:** pointer events (`onPointerDown/Move/Up`, scroll deltas) with
node-local coordinates.

### G-9. No bracketed paste
No paste event with bracket markers exists. Pasting into the terminal cannot
be wrapped in `\x1b[200~...\x1b[201~`.
**Needed:** a paste event on the key-event stream, or a clipboard API.

### G-10. No scrollback virtualization
The fallback renders every row as nodes. A 10,000-line scrollback would be
10,000 `UiRow`s. The `UiTerminal` node is designed so the native renderer
can virtualize, but the fallback cannot.
**Needed:** native `terminal` renderer, or a virtualized list node.

### G-11. No clipboard read API (copy works, paste doesn't)
`onCopy` fires with selected text and the app can write it to the OS
clipboard through the host, but there is no clipboard-read event for
bracketed paste (see G-9).

## What the native renderer must implement for full P0-8

When the Rust side implements the `terminal` node kind, it needs:
1. Monospace glyph grid from `cells` (respect `fontFamily`, `fontSize`,
   `lineHeight`).
2. Per-cell fg/bg from the theme-resolved colors (256 + truecolor).
3. `TerminalDamage` ranges → dirty-cell redraws at vsync (60fps).
4. Cursor: block/underline/bar, blinking/steady, at `cursor`.
5. Selection overlay from `selection`.
6. Scrollback buffer of `scrollbackLines` with wheel + scrollbar.
7. Raw key events → Dart (G-5), IME (G-6), mouse SGR 1006 (G-8),
   bracketed paste (G-9), resize → `onResize` (G-7).

The `UiTerminal` JSON contract in `terminal.dart`/`nodes.dart` is the spec.
