# Swift port sidecars

This directory holds one YAML sidecar per worker area. They are the **only**
place workers record port status. `docs/parity/swift-port-map.md` is
**generated** — never hand-edit it.

## Workflow

1. Edit **only** your area's file: `docs/parity/swift-port/<area>.yml`.
   Two workers never touch the same file, so there are no merge conflicts.
2. Re-run the generator: `python3 scripts/generate-swift-port-map.py`
3. Commit **both** your `.yml` and the regenerated
   `docs/parity/swift-port-map.md`.

The script composes the map from (a) the Swift file walk (paths, LOC),
(b) filename-heuristic destinations, (c) sidecar overrides — **the sidecar
wins** for every field it sets. Re-running with no sidecar changes produces
byte-identical output.

## Sidecar format

```yaml
area: terminal
files:
  native/SupercliNative/Sources/SupercliNative/TerminalFindBar.swift:
    destination: "Dart: clients/supercli-app"   # or Rust crate, "dropped: ...", "gpuidart gap ..."
    status: todo                                # todo | partial | ported | wired | verified
    checklist: "200, 201"                       # parity checklist rows (string or list)
    behaviours:                                 # behaviour-level proof (required for "ported")
      - swift: "Find bar text input with live highlight"
        target: "lib/screens/terminalfindbar.dart::TerminalFindBar.build"
        test: "test/terminal_test.dart::find highlights matches"
    tests:                                      # test files proving the port
      - file: clients/supercli-app/test/terminal_test.dart
        count: 3
    notes: "Why this destination; what remains."
```

- `behaviours`: list every Swift behaviour (each func, gesture, keybinding,
  state transition). `target`/`test` may be `null` while the audit is pending —
  but the row **cannot** be `ported` until every behaviour has both.
- `tests`: list of `{file, count}` (or plain file-path strings, counted as 1).
  The map's "Tests ported" column shows the summed count.

## The strict "ported" standard

A row is `ported` **only when every Swift behaviour maps to a Dart/Rust
function PLUS a test**. Zero tests means not ported. Use `partial` when some
behaviours are ported. Use `todo` when the behaviour audit hasn't been done.

Overclaimed rows will be reverted on review — cite the function per
behaviour, not just the file.

## Status meanings

- `todo` — not started
- `partial` — some behaviours ported, audit incomplete
- `ported` — every behaviour ported with tests (strict bar above)
- `wired` — ported code wired to real backend/UI and mounted
- `verified` — wired plus real-window screenshot (UI) or conformance proof

## Areas

| Area | Owner's scope |
|---|---|
| `shared` | `shared/SupercliShared` → Rust |
| `macos-services` | launchd, keychain, notifications, Finder, menu-bar, updater → Rust |
| `terminal` | TerminalPaneView, panes, terminal UI → Dart |
| `sidebar` | sidebar, session, workspace views → Dart |
| `settings` | all settings views/tabs → Dart |
| `remote` | remote host, pairing, Link → Rust + Dart |
| `appkit` | `app-kit/swift` renderer → Dart `appkit_widgets` |
| `ios` | `ios/SupercliIOS` non-UI → Rust (UI waits for gpuidart mobile) |
