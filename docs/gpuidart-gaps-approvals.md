# gpuidart gaps — approvals panel + notifications

Logged 2026-09-26 by parity worker (c). These are framework APIs the
Swift originals use that have no counterpart in upstream gpuidart
(at `135d300`). The Dart port renders through UiColumn/UiRow/UiText/
UiButton primitives (RLE fallback pattern) until Amein ships them.
Do NOT work around by modifying `clients/gpuidart/` — it is a submodule.

## P0-1: UiApprovalCard widget (from mcpapprovalpanel.dart)

The Swift `MCPApprovalPanel` renders a glass-card overlay:
- severity/risk badge styling on the attention dot
- timeout countdown ("expires in 0:42")
- structured diff view for write/file approvals
- glassmorphism background effect

Port status: `McpApprovalPanel` uses UiRow(UiText dot + UiText title) +
UiText body + UiRow of UiButtons. Functional, unstyled.

## P0-2: UiToast / UiBanner widget (from toastcenter.dart)

The Swift `ToastCenter` renders transient bottom-edge toasts with:
- slide-in/slide-out animation
- TTL progress-bar indicator on each toast
- stacked offset layout (newest on top, older pushed down)

Port status: `ToastCenter` uses UiColumn of UiRow(dot + body + dismiss
button). No animation, no TTL indicator, flat list layout.

## P0-3: Bare-letter key bindings rejected (from mcpapprovalpanel.dart)

`UiAction` docs: "Bare letters and digits with no modifier are rejected:
native text input and IME own them." The Swift approval panel binds bare
`E` to edit-before-answering (`McpApprovalKeyMonitor`). The port uses
`ctrl+e` instead. If the framework ever allows scoped bare-letter
bindings when no text input is focused, the binding should revert to `e`.

## P0-4: Native notification delivery (from notifications.dart)

`AppNotification`/`NotificationQueue` are Dart-side models only. The
Swift originals deliver through:
- macOS Notification Center banners (needs-input / finished / App alerts)
- in-app banner overlay for foreground notifications
- notification sounds / haptics

There is no gpuidart API for posting a native notification or playing a
notification sound. Checklist item [DESKTOP] #195 (macOS Notification
Center banners) stays `missing` until the framework exposes it.

## P0-5: Click-to-focus target resolution

`ToastCenter.handleAction('toast.focus', id)` fires the Dart callback,
but there is no framework API to move native focus to an arbitrary node
id (e.g. `mcp-approval-overlay`). The host must implement focus routing
outside gpuidart.
