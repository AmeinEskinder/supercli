/// Terminal pane: the embedded terminal surface for a session.
///
/// Home of the `TerminalPaneView.swift` port
/// (`clients/legacy/native/SupercliNative/.../Views/TerminalPaneView.swift`).
/// The view itself is below; the portable surface logic (title-chip rename
/// editing, focus resolution, archive-verb decision, pane menus, split
/// layout, drop-zone preview rects) lives in `lib/terminal/` and is
/// re-exported here so this screen stays the port's single entry point.
///
/// Real implementation of the P0-8 terminal pane (not a scaffold). The pane
/// is backed by [TerminalState] (grid + scrollback + cursor + selection) and
/// renders through [TerminalPane] via the RLE fallback (P0-8 UiTerminal proposal pending)
/// node for the native renderer plus a run-length-encoded UiRow/UiText
/// fallback that displays today with real ANSI 256 + truecolor.
///
/// See `lib/terminal/` for the implementation and
/// `docs/gpuidart-gaps-terminal.md` for what gpuidart cannot yet do.
library;

import 'package:gpuidart/gpuidart.dart';

import '../host_client.dart';
import '../terminal/session_output_stream.dart';
import '../terminal/terminal_drop_maps.dart';
import '../terminal/terminal_pane.dart';
import '../terminal/terminal_state.dart';

export '../terminal/terminal_drop_maps.dart' show TerminalDropMaps;
export '../terminal/terminal_pane.dart' show TerminalPane, TerminalKeymap;
export '../terminal/terminal_state.dart' show TerminalState, TerminalCellUpdate;
export '../terminal/session_output_stream.dart' show SessionOutputStream;
// Surface logic ported from TerminalPaneView.swift (re-exported so this
// screen is the port's single home; implementation in lib/terminal/).
export '../terminal/terminal_pane_rename.dart'
    show TerminalPaneRename, RenameEnd, RenameJustEnd, RenameCommit;
export '../terminal/terminal_pane_focus.dart'
    show TerminalPaneFocus, PaneSessionStatus;
export '../terminal/terminal_pane_archive.dart'
    show TerminalPaneArchive, PaneArchiveDecision;
export '../terminal/terminal_pane_actions.dart'
    show
        CloseActivePaneDecision,
        CloseActivePaneNone,
        CloseActivePaneDetach,
        CloseActivePaneRemove,
        CloseActivePaneConfirmArchive,
        ClosePaneSessionEntry,
        resolveCloseActivePane,
        ActivatePaneDecision,
        ActivatePaneIgnored,
        ActivatePaneSolo,
        ActivatePaneGrouped,
        resolveActivatePane,
        SyncActivePaneDecision,
        SyncActivePaneResetSolo,
        SyncActivePaneReaffirm,
        SyncActivePaneActivateDefault,
        resolveSyncActivePane,
        resolveFocusRequest,
        resolveClaimPendingReveal,
        resolveZoomedPane,
        liveDividerRatio,
        shouldDetachPane,
        shouldLaunchIntoPane,
        PaneBannerKind,
        paneBanners;
export '../terminal/terminal_pane_menu.dart'
    show
        TranscriptCopyRange,
        PaneMenuAction,
        PaneMenuItem,
        PaneSessionMenuInput,
        buildPaneSessionMenu,
        buildPaneLauncherMenu,
        copySessionIdText;
export '../terminal/terminal_pane_split.dart'
    show TerminalPaneSplitLayout, SplitPaneExtents;
export '../terminal/terminal_pane_drop_zones.dart'
    show
        DropZoneEdge,
        PaneDropTarget,
        PaneTarget,
        GroupEdgeTarget,
        DropZoneRect,
        dropZonePreviewRect,
        fitToDesktopHelpText,
        fitToDesktopLabel;

/// One terminal pane within a session.
final class TerminalPaneView {
  TerminalPaneView({
    required this.paneId,
    required this.title,
    List<String> lines = const [],
    this.findBarVisible = false,
    TerminalState? state,
    this.onInput,
    this.onResize,
    this.onCopy,
    this.sessionDirectory,
  }) : state = state ?? _stateFromLines(lines),
       stream = null,
       onStreamError = null,
       sessionId = null;

  /// A terminal pane bound to a LIVE Host session.
  ///
  /// Creates one [TerminalState] shared by the pane and its
  /// [SessionOutputStream]; [start] begins polling `GET /mobile/output`
  /// and feeding the ANSI parser. Input (key bindings) forwards to
  /// `POST /mobile/write` with a per-batch idempotency id; resize forwards
  /// to `POST /mobile/resize`. All traffic goes through the authenticated
  /// [HostClient] — the view never reads Host journal files directly.
  ///
  /// [appSessionsDir] is the Host's local `app-sessions` directory
  /// (`~/.supercli/app-sessions`). When provided, the pane reads the
  /// session's drop-target / path-drag map files through the Rust FFI
  /// ([acceptsDropAt]/[dragPathAt]). Leave it null for remote/paired Hosts,
  /// where the session directory is not on this machine and the maps stay
  /// disabled.
  factory TerminalPaneView.hosted({
    required HostClient client,
    required String sessionId,
    required String paneId,
    required String title,
    bool findBarVisible = false,
    void Function(String text)? onCopy,
    void Function(Object error)? onStreamError,
    int cols = 80,
    int rows = 24,
    String? appSessionsDir,
  }) {
    final state = TerminalState(cols: cols, rows: rows);
    final stream = SessionOutputStream(
      client: client,
      sessionId: sessionId,
      state: state,
    );
    var widCounter = 0;
    return TerminalPaneView._(
      paneId: paneId,
      title: title,
      state: state,
      stream: stream,
      sessionId: sessionId,
      sessionDirectory: appSessionsDir == null
          ? null
          : TerminalDropMaps.sessionDir(appSessionsDir, sessionId),
      findBarVisible: findBarVisible,
      onCopy: onCopy,
      onStreamError: onStreamError,
      onInput: (bytes) {
        // Fire-and-forget: a failed write surfaces via onStreamError on
        // the next poll cycle; the idempotency id keeps retries safe.
        stream.sendInput(bytes, writeId: 'wid-${widCounter++}').ignore();
      },
      onResize: (cols, rows) {
        stream.resize(cols, rows).ignore();
      },
    );
  }

  const TerminalPaneView._({
    required this.paneId,
    required this.title,
    required this.state,
    required this.stream,
    required this.findBarVisible,
    required this.onInput,
    required this.onResize,
    required this.onCopy,
    required this.onStreamError,
    this.sessionId,
    this.sessionDirectory,
  });

  final String paneId;
  final String title;
  final TerminalState state;

  /// The Host session id this pane streams, or null for static/demo panes.
  final String? sessionId;

  /// Local session directory (`~/.supercli/app-sessions/<id>`), or null
  /// when the maps are unavailable (static/demo panes, remote Hosts).
  /// The drop-target and path-drag maps are read from here.
  final String? sessionDirectory;

  /// Whether the terminal drop/drag maps are available for this pane.
  bool get hasDropMaps => sessionDirectory != null;

  /// Whether the session's drop-target map accepts a drop at (row, column).
  /// Fails closed (false) when the maps are unavailable or the map file is
  /// missing/stale/unreadable. Reads through the Rust FFI.
  bool acceptsDropAt(int row, int column) {
    final dir = sessionDirectory;
    if (dir == null) return false;
    return TerminalDropMaps.acceptsDropAt(
      sessionDir: dir,
      row: row,
      column: column,
    );
  }

  /// Host-local path for the session's path-drag map at (row, column), or
  /// null when unmapped. Fails closed when the maps are unavailable or the
  /// map file is missing/stale/unreadable. Reads through the Rust FFI.
  String? dragPathAt(int row, int column) {
    final dir = sessionDirectory;
    if (dir == null) return null;
    return TerminalDropMaps.dragPathAt(
      sessionDir: dir,
      row: row,
      column: column,
    );
  }

  /// Non-null for [TerminalPaneView.hosted]: the live Host stream feeding
  /// [state]. Null for static/demo panes.
  final SessionOutputStream? stream;
  final bool findBarVisible;
  final void Function(List<int> bytes)? onInput;
  final void Function(int cols, int rows)? onResize;
  final void Function(String text)? onCopy;

  /// Stream errors (auth failures, session gone, …). The app surfaces these
  /// in the status line / toasts.
  final void Function(Object error)? onStreamError;

  /// True for panes bound to a live Host session.
  bool get isLive => stream != null;

  /// Start the Host output stream. No-op for static panes.
  Future<void> start() =>
      stream?.start(onError: onStreamError) ?? Future.value();

  /// Stop the Host output stream. No-op for static panes.
  void stop() => stream?.stop();

  static TerminalState _stateFromLines(List<String> lines) {
    final cols = lines.fold<int>(80, (m, l) => l.length > m ? l.length : m);
    final state = TerminalState(cols: cols, rows: 24);
    if (lines.isNotEmpty) {
      state.writeString(lines.join('\n'));
    }
    return state;
  }

  /// The underlying P0-8 pane (state + rendering + input).
  TerminalPane get pane => TerminalPane(
    paneId: paneId,
    title: title,
    state: state,
    onInput: onInput,
    onResize: onResize,
    onCopy: onCopy,
    findBarVisible: findBarVisible,
  );

  UiNode build() => pane.build();

  /// Key bindings for the pane's special keys, scoped to the terminal node.
  /// The scope must name a node that exists in the mounted tree
  /// ('terminal-pane-$paneId' from [TerminalPane.build]); gpuidart rejects
  /// action contexts that match no node.
  List<UiAction> actions() =>
      const TerminalKeymap().actionsFor('terminal-pane-$paneId');
}

/// Terminal pane window chrome (title bar, traffic lights are native).
/// See terminalarea.dart for the area that hosts panes.
