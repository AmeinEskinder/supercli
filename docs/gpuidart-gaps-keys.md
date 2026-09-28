# gpuidart API gaps — native key parser rejects punctuation keys

Date: 2026-09-27
Worker: wire-settings fix
Branch: `feat/wire-settings`

Upstream gpuidart (clients/gpuidart) native key parser accepts only
letters, digits, f1–f12, enter, escape, space, tab, arrows, home/end,
pageup/pagedown, delete, and backspace as the final key of a chord
(see `UiAction` grammar in gpuidart `lib/src/actions.dart`). Punctuation
keys — `,`, `=`, `-`, `+`, `;`, `.`, `/`, etc. — are rejected. Worse, a
single invalid chord causes the native host to reject the ENTIRE window:
no window opens at all (verified empirically 2026-09-27: `ctrl+,`,
`ctrl+comma`, `ctrl+;` all fail; `ctrl+s` works).

## Impact on the Supercli keymap

| Chord | Canonical use | Status |
|-------|---------------|--------|
| Cmd-, / Ctrl-, | Settings… (AppDelegate.swift:313, `Keymap.settings`) | NOT registered natively; Settings reachable from command palette |
| Cmd-= / Ctrl-= | Increase font size | NOT registered natively |
| Cmd-- / Ctrl-- | Decrease font size | NOT registered natively |
| Cmd-0 / Ctrl-0 | Reset font size | NOT registered natively (see correction below) |

**Correction (2026-09-29):** digits ARE accepted by the native parser — only
`,`, `=`, `-`, `+` (and other punctuation) are blocked. So Cmd-0/Ctrl-0 for
reset-font-size CAN be registered natively; the table row above is
overly conservative. The blocked set is punctuation only.

## Workaround in the app (no submodule changes)

- The canonical chords stay the single source of truth in
  `clients/supercli-app/lib/keymap.dart` (`Keymap.settings`,
  font-size chords) and are shown in the command palette for
  documentation.
- No `UiAction` is registered natively for any punctuation chord.
- Settings opens from the command palette ("Open settings"), which
  dispatches the action name directly through `SupercliApp.handleAction`
  — no native chord needed.
- We deliberately did NOT remap Settings to Shift+Cmd+P: that chord is
  the common command-palette chord and a silent remap would surprise
  users.

## Request for Amein (upstream gpuidart)

Extend the native key parser to accept punctuation keys in chords
(`,`, `=`, `-`, `+`, and ideally the full printable set), and — more
importantly — make a single invalid chord fail closed on that binding
only, never reject the whole window. Until then, punctuation chords stay
palette-only.
