# gpuidart gaps — [APPS] panes (Git, Files, Markdown, Usage)

Logged by the [APPS] parity worker (`track-b-parity-apps`). Framework gaps
are for Amein; host-backend gaps block live data until the Host ships routes.

## Framework gaps (package:gpuidart upstream)

### GAP-A4 — No native context-menu / clipboard / file-dialog APIs
- **What the panes need:** the Files pane's context menu (open / send to
  agent / copy path), the Git pane's "Copy lines", and the Markdown pane's
  Open/Save dialogs are exposed as Dart callbacks (`onOpen`, `onCopyPath`,
  `onCopyLines`, `onOpenNote`, `onSaveNote`). There is no upstream
  `UiContextMenu`, clipboard, or file-dialog node/action in gpuidart.
- **Current workaround:** the app shell wires the callbacks to platform
  implementations. The panes render `UiButton`s for the same actions so
  they stay reachable without a context menu.
- **Needed from Amein:** upstream clipboard read/write, a context-menu
  primitive, and file open/save dialogs (or a statement that the shell
  owns these permanently).

### GAP-A3 — No syntax-highlighting primitive for diffs
- **What the pane needs:** parity row 135 wants syntax-highlighted patches.
  The RLE fallback colors whole lines (add/del/hunk); token-level
  highlighting needs either per-token `UiText` runs (expensive but
  possible today) or a framework-provided highlighted-code node.
- **Current workaround:** line-level coloring only; per-token highlight
  explicitly out of scope until a cheap primitive exists.
- **Needed from Amein:** guidance — is per-token `UiText` the blessed
  approach, or will gpuidart ship a code/highlight node?

## Host backend gaps (supercli-serve)

### GAP-A1 — Git backend route now exists (Host side)
- RESOLVED (Host side, 2026-09-26, `track-b-parity-hostroutes`):
  `crates/supercli-core/src/host_git.rs` exposes `GET /mobile/git/status`,
  `GET /mobile/git/diff`, `GET /mobile/git/history`,
  `POST /mobile/git/{stage,unstage,commit,fetch,pull,push}` (all scoped
  through `ResourceScope`; 10 Rust tests against a temp repo).
  `HostClient` gained `gitStatus/gitDiff/gitHistory/gitStage/gitUnstage/
  gitCommit/gitFetch/gitPull/gitPush` (12 Dart mock-Host tests) and
  `lib/screens/git_pane_controller.dart` binds the view's callbacks.
- REMAINING: the Git pane still renders from `StubGitDataSource` by
  default; the app shell must construct it via `GitPaneController.view()`
  and feed live data (worker h's app-shell integration).

### GAP-A2 — File-browse / usage backend routes now exist (Host side)
- RESOLVED (Host side, 2026-09-26, `track-b-parity-hostroutes`):
  `GET /mobile/files/list`, `POST /mobile/files/write` (atomic,
  scope-checked), `GET /mobile/usage/stats` (session counts + provider
  transcript presence) in `host_git.rs`; `HostClient.filesList/filesRead/
  filesWrite/usageStats` with Dart tests.
- REMAINING: Files/Usage panes still render stub data; shell integration
  pending (worker h). Notes (`GET /mobile/notes`) not yet implemented.

## Deliberately not gaps
- `UiColumn`, `UiRow`, `UiText`, `UiButton`, `UiStyle`, `UiColor`,
  `UiFontWeight`, `UiAction`/`UiActionContext.node` all exist upstream and
  are the only framework APIs used. The `clients/gpuidart` submodule was
  not modified.
