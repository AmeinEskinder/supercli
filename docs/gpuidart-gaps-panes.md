# gpuidart Gaps: Split Panes, Zoom, Focus Navigation

Date: 2026-09-26
Worker: track-b-parity-panes

These are framework APIs that upstream gpuidart (135d300) does not provide,
needed for a native-quality split-pane implementation. The supercli-app
implementation in `lib/pane_layout.dart` works around them with UiRow/UiColumn
composition. Do NOT modify `clients/gpuidart/` — these are for Amein to
implement upstream.

## P0-9: No resizable split-pane primitive

**Status:** Worked around (UiRow/UiColumn composition)

The `RootView` already logs this gap. For pane splits specifically:
- No `UiSplit` node with a draggable divider
- No way to express flex ratios (our `PaneSplit.ratio` is model-only; the
  renderer cannot allocate proportional space)
- Dividers render as `UiButton` strips with `│`/`─` labels — they are not
  draggable, and there is no hover cursor API to indicate resizability

**Needed:**
- `UiSplit(direction, ratio, first, second, onRatioChanged)` or equivalent
- Divider drag events in the host event stream
- Flex layout: children with flex factors, not just wrap-on-narrow `UiRow`

## P0-15: No focus ring / focused-node styling

**Status:** Worked around (`●` marker text in pane header)

There is no framework-level "focused" visual state. We render a `●` text
marker in the focused pane's header. Upstream Ghostty draws a focus ring /
accent border around the focused split.

**Needed:**
- `UiStyle` support for focus rings, or a `focused` flag on `UiNode` that
  the native renderer maps to platform focus styling

## P0-16: No zoom-to-fill primitive

**Status:** Worked around (conditional render of zoomed pane only)

Zoom is implemented by rendering only the zoomed pane plus a banner. There
is no animated transition and no framework "maximize pane" concept.

**Needed:**
- Optional: animated zoom transition API

## P0-17: Keyboard focus traversal

**Status:** Implemented in Dart model (`PaneLayout.focusDirection`)

Spatial focus (alt+cmd+arrows) is computed from the split tree geometry in
Dart. This works, but the framework has no concept of "focus order" or
spatial navigation between nodes — every app reimplements it.

**Needed (nice-to-have):**
- Framework-level spatial focus navigation API

## Notes

- All pane management key bindings (cmd+d, shift+cmd+d, shift+cmd+enter,
  alt+cmd+arrows, ctrl+tab) are declared as `UiAction`s scoped to the
  `pane-layout` node. They require the host to route key events to the
  action system — verified present in upstream `UiAction`/`UiActionContext`.
- Pane persistence (`pane-layouts.json` per scope, checklist row 176) is
  not yet implemented — the model has `toJson()` ready; the host-side
  persistence hook is future work.
- Detach Pane (checklist row 173) maps to the existing `TerminalPaneWindow`
  (needs multi-window support, P0-14, already logged).
