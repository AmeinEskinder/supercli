# gpuidart Requirements for supercli

This document records what supercli's UI needs from
[gpuidart](https://github.com/ameineskinder/gpuidart) (Amein's framework),
what gpuidart already provides, and the gaps Amein will implement himself.

Port order: shared components → desktop → mobile → web.

## Framework overview

| Aspect | gpuidart today |
| --- | --- |
| Language | **Dart** (application code, SDK ≥ 3.13) + **Rust** (native controls via GPUI Kit, pinned toolchain 1.98.1) |
| Rendering model | **Retained-mode, GPU-accelerated.** Dart submits a whole UI description as UTF-8 JSON over FFI; Rust owns the description and retained control state. One snapshot per `publish()`; no node patches. Snapshots ≤ 4,096 nodes, depth ≤ 32, ≤ 16 MiB. |
| Widget set | `UiColumn`, `UiRow` (layouts), `UiText`, `UiButton`, `UiInput` (native text input), `UiTable` (virtualized, dataset-backed, ≤ 100k rows × 64 cols) |
| Platforms | Windows x64 (reference), macOS 15 ARM64, Ubuntu 24.04 x64 (X11). **No iOS/Android, no web.** |
| Async/events | `Stream<GpuiEvent>` broadcast: `click`, `input`, `table_selection`, `ready`, `closed`, `applied`, `rejected`, `dataset_*`, `error`, `diagnostic`. `Future<void> publish(UiNode)` applies a snapshot (not a present fence). Datasets: `registerDataset` / `editDataset` / `replaceDataset` / `releaseDataset` with revisioned transactions. |
| Styling | `UiStyle` per node: padding, gap, width/height (`px`/`full`/`fit`), align/justify, background/foreground/border colors, border radius, font size/weight. Colors are `ThemeToken` (16 fixed roles from gpui-kit) or raw `#RRGGBB[AA]`. No inheritance. |
| Text input | `UiInput`: native input owns text, cursor, selection, undo, IME composition. Dart receives `input` events. **Uncontrolled** — no value setter. Single-line only. |
| Focus/keyboard | **None.** No focus API, no keybindings. A design exists in gpuidart's `docs/feature-stack.md` ("actions and scoped keymaps") but is not implemented. |
| Accessibility | **None** in the Dart API. |
| i18n | **None.** No locale, no string-table hooks. |

Key architectural facts that shape the requirements below:

- Node IDs must be nonempty and unique per description; reusing an ID + kind
  preserves native state (inputs keep text/cursor across rebuilds).
- Table datasets upload once and are referenced by ID; cell/row edits send
  only deltas. Cells are **strings rendered in Rust** — no Dart callbacks in
  layout/paint.
- Native event queue holds 64 commands; acknowledgements have a 30 s deadline.
- One host, one window per process.

## P0 — blocking supercli UI (Amein implements)

### P0-1. Approval card widget
supercli's core interaction is approve/deny/cancel on tool calls. Today this
must be hand-assembled from `UiColumn` + `UiText` + `UiButton` with no
standard layout, no severity styling, and no keyboard default.

```dart
// Sketch
final card = UiApprovalCard('appr-42',
  title: 'Write file',
  detail: 'src/main.rs (12 lines)',
  severity: ApprovalSeverity.danger,   // info | warn | danger
  actions: const ['approve', 'deny'],  // buttons, first = default
  timeoutSecs: 120,
);
// events: {type:'approval_answer', id, answer:'approve'|'deny', revision}
```

### P0-2. List views (sessions, approvals, files)
`UiTable` is built for 100k-row datasets with string cells. supercli needs a
lightweight list for tens of items with per-row tap, selection highlight, and
subtitle text — without the dataset ceremony.

```dart
// Sketch
final list = UiList('sessions', items: [
  UiListItem(id: 's1', title: 'api-server', subtitle: 'idle · 3 panes',
             selected: true),
]);
// events: {type:'list_select', id, item:'s1', revision}
```

### P0-3. Multiline text composer (controlled input)
`UiInput` is single-line and uncontrolled (native owns the text; Dart only
receives events). supercli's composer needs multiline, programmatic
set/clear, and submit-on-shortcut.

```dart
// Sketch
final composer = UiComposer('composer',
  placeholder: 'Message the agent…',
  minLines: 1, maxLines: 8,
  submitKeys: 'ctrl+enter',            // see P0-4
);
await host.setComposerText('composer', '');   // controlled clear
// events: {type:'composer_submit', id, value, revision}
```

### P0-4. Keyboard focus navigation + scoped keymap
No focus API and no keybindings exist. supercli must be fully keyboard
operable (the accessibility pass requirement): tab order, arrow-key list
navigation, and shortcuts. gpuidart's own `docs/feature-stack.md` already
designs "actions and scoped keymaps" — implement that design.

```dart
// Sketch
await host.registerActions([
  UiAction(name: 'approval.approve', keys: 'ctrl+enter', context: 'global'),
  UiAction(name: 'list.down', keys: 'arrowdown', context: 'sessions'),
]);
await host.focus('composer');   // programmatic focus
// events: {type:'action', name, context, revision}
```

### P0-5. Host→UI event push pattern
supercli's UI is a **controller of a Rust Host** (`supercli-host`) over the
existing Host protocol (websocket). gpuidart's event stream is native→Dart
only. What is missing is the *blessed pattern* for external async sources
(Host websocket messages, timers) to trigger UI rebuilds without racing
`publish()`.

```dart
// Sketch — serialize external events through one Dart-side queue:
final queue = StreamQueue(hostEventsFromWebsocket);
await for (final msg in queue.rest) {
  applyToModel(msg);        // update Dart model
  await host.rebuild();     // whole-snapshot publish, awaited, no overlap
}
// Requirement: document that overlapping publish() calls are an error and
// provide a helper (e.g. host.serialRebuild(fn)) so app code can't get it wrong.
```

## P1 — needed before ship

- **Theming (dark/light + custom).** Today: 16 fixed `ThemeToken`s, no
  runtime switching. Need: `host.setTheme(ThemeMode.dark)` and loading a
  token table from the app, so supercli can ship its own theme.
- **i18n hooks.** Today: none. Need: a string-table lookup the app registers
  once (`host.registerStrings({'en': {...}, 'de': {...}})`) plus a
  `UiText.tr('key')` constructor, so the 238 UI strings stay keyed.
- **Accessibility.** Today: none. Need: `semanticsLabel` on nodes, focus
  order override, and screen-reader role hints (`role: 'button'|'list'|'text'`),
  wired to each platform's native accessibility API.
- **Mobile targets.** Today: desktop only. Need: iOS + Android shells
  (even if via a single-window activity/view-controller embedding the same
  GPUI view), plus touch input events and on-screen-keyboard handling for
  `UiInput`/`UiComposer`.

## P2 — polish, after first release

- **Charts.** Tiny sparkline/bar widget for benchmark and usage screens
  (data via the dataset mechanism, rendering in Rust).
- **Rich text.** Markdown subset in `UiText` (bold/code/links) for agent
  transcripts; no full HTML.
- **Animations.** Reduced-motion-aware transitions (fade/slide) on node
  insert/remove; must respect `prefers-reduced-motion`.

## Prioritized build list for Amein

1. P0-4 keymap + focus (unblocks keyboard operability everywhere)
2. P0-3 composer (unblocks the main input surface)
3. P0-1 approval card (unblocks the core approve/deny loop)
4. P0-2 list views (unblocks session/approval/file browsing)
5. P0-5 event-push pattern + `serialRebuild` helper (unblocks Host wiring)
6. P1 theming, i18n, a11y, mobile shells
7. P2 charts, rich text, animations
