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

### GAP-A1 — No git backend route
- The Host exposes no git status/diff/log/push route (`HostClient` has
  approvals/sessions/messages/browser only). The Git pane renders from
  `StubGitDataSource` (clearly marked STUB in
  `lib/widgets/git_widgets.dart`). Live data needs e.g.
  `GET /mobile/git/status`, `GET /mobile/git/diff?path=…`,
  `GET /mobile/git/log`, `POST /mobile/git/{stage,commit,push,…}`.

### GAP-A2 — No file-browse / notes / usage backend routes
- The Files pane (`StubFilesDataSource`), Markdown pane
  (`StubMarkdownDataSource`), and Usage pane (`StubUsageDataSource`) all
  render representative stub data. Live data needs e.g.
  `GET /mobile/files/list?path=…`, `GET /mobile/notes`,
  `GET /mobile/usage`.
- Row 140 (panes follow the neighbouring agent's project/worktree) is
  modeled via `rootPath` on the Files stub; the real follow behavior needs
  the Host to report the adjacent agent's worktree.

## Deliberately not gaps
- `UiColumn`, `UiRow`, `UiText`, `UiButton`, `UiStyle`, `UiColor`,
  `UiFontWeight`, `UiAction`/`UiActionContext.node` all exist upstream and
  are the only framework APIs used. The `clients/gpuidart` submodule was
  not modified.
