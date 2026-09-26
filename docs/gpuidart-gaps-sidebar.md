# gpuidart Gaps for the Sidebar (for Amein)

**Date:** 2026-09-26
**Branch:** `track-b-parity-sidebar`
**Status:** Real implementation where gpuidart allows; gaps logged below.

The sidebar renders through the UiRow/UiText fallback pattern (same as the
terminal RLE fallback): session rows, group headers, project trees, context
menus and the drag overlay are all real widgets with behavioral tests.
What is missing is framework-level interactivity — the items below need
native gpuidart APIs. Nothing was worked around silently.

## G-1: Drag-and-drop (blocks #152 — live drag)

Detached drag of sessions/projects ("Dia feel": reorder, move to group,
drop-to-split) needs OS pointer drag events: drag-start on a row, hover
target under the pointer, drop position (before/after/into), and a floating
drag image. `SidebarSessionDrag` models the full state machine
(`SidebarDropTarget.reorder/group/project/split`, `canDrop` validation,
drop-to-split), but there is no `onDragStart`/`onDragOver`/`onDrop` event
in `native_event.dart` and no drag-image node. Proposed: `dragstart`,
`dragover` (with target node id + drop position), `drop` native events.

## G-2: Popup context menus (blocks #153/#154 — popup behavior)

`SessionContextMenu` (11 items) and `ProjectContextMenu` (9 items) render
as real button columns and item activation works via the existing `click`
event (action decoded from the button id). What is missing is *popup*
behavior: anchoring the menu at the pointer, dismiss-on-outside-click, and
keyboard navigation. Proposed: a `UiMenu`/`UiPopover` node or a
`showPopup(nodeId, at)` host call.

## G-3: Per-row click without a button (row selection)

Session rows are `UiRow`s of `UiText`; selecting a row currently needs a
`UiButton` per row (or a `UiTable` with `table_selection` events, the old
scaffold approach). A `click` event on any node id (not just buttons)
would let rows be selectable directly. Workaround in use: rows are static;
selection is host-driven state.

## G-4: UiAction requires a key binding

`UiAction.toJson()` throws on empty `keys`, so pointer-only actions (menu
items, dots, collapse toggles) cannot be expressed as actions. Menu items
use `click` events instead (works), but a first-class "command without
shortcut" would be cleaner. Proposed: allow empty `keys` for
pointer/menu-triggered actions.

## Non-gaps (already work)

- `click` native events on `UiButton` by node id — used for menu items,
  workspace dots, archived restore.
- `UiAction` with node-scoped contexts — used for `sidebar.filter`
  (cmd+f), `session.new` (cmd+n), `session.select-next` (ctrl+tab),
  `sidebar.drag.cancel` (escape).
- `UiColor.hex` + `UiStyle` foreground/background/fontWeight — attention
  dot, selected-row highlight, unread badges, folder color swatches.
