# gpuidart API gaps — settings screens (track-b-parity-settings)

Date: 2026-09-26
Worker: (d) settings screens
Branch: `track-b-parity-settings`

Upstream gpuidart (clients/gpuidart, pinned at 135d300) exposes only basic
layout/data nodes. The settings port works around every missing widget with
an RLE-compatible fallback. Do NOT modify the submodule; these are requests
for upstream.

## Missing APIs and the fallbacks used

| # | Missing API | Fallback in this port | Requested shape |
|---|-------------|----------------------|-----------------|
| P0-10 | `UiToggle` / switch | `SettingsToggle`: `UiRow` + `UiButton` whose label is `On`/`Off` | `UiToggle(id, {value, onChanged})` |
| P0-11 | `UiSelect` / dropdown | `SettingsSelect`: one `UiButton` per option, selected marked `● ` | `UiSelect(id, {options, value, onChanged})` |
| P0-12 | `UiSlider` / stepper | Font-size row with `−`/`+` buttons | `UiSlider(id, {min, max, value, onChanged})` |
| P0-13 | Drag-and-drop reorder | Plugin reorder via `Move up`/`Move down` buttons; `PluginListDrag` renders a placeholder note | `onReorder` support in `UiTable` or a `UiReorderableList` |
| P0-14 | Native tab / segmented control | Tab strip = column of `UiButton`s, active marked `● ` | `UiTabBar` / `UiSegmentedControl` |
| P0-15 | `UiColorPicker` | Accent color = `SettingsSelect` with 8 named options | `UiColorPicker(id, {value, onChanged})` |
| P0-16 | Secure input | License key uses plain `UiInput` | `UiInput(obscureText: true)` |
| P0-17 | `UiQR` widget | Pairing code shown as `UiText` (hostpickerview P0-11 gap) | `UiQR(id, data)` |
| P0-18 | Native folder picker | `Choose…` buttons with no backend call wired | `showFolderPicker()` async API |
| P0-19 | Form binding/validation | Preset editor is raw `UiInput`s + Save/Cancel | `Form` + `TextEditingController` equivalents |

## Notes

- `TableDataset` exposes `rowCount`, `row(index)`, `cell(row, column)` — no
  public `rows` getter. Tests use the index API.
- `UiRow`/`UiColumn` take `(id, children)`; `UiButton`/`UiText` take
  `(id, label)`; `UiInput(id, {placeholder})`; `UiTable(id, {dataset})`.
- Host persistence: `AppSettings.toHostJson()` serializes the Host
  allowlist (`settings.workspace.set`) in the camelCase wire format;
  `HostClient.settingsSet` (POST `/mobile/workspace-settings`) and
  `settingsGet` (GET `/mobile/workspace-settings`) provide the transport,
  and `SettingsController` (settings_controller.dart) loads on startup and
  persists edits with a debounce, surfacing failures via `onError` for the
  ToastCenter. Desktop-only keys (accent, terminal font, notifications)
  are local state only. Note: `theme` IS Host-managed via
  `appearanceSettings.theme`.
