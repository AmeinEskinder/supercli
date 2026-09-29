/// Active-pane focus resolution for the terminal pane container.
///
/// Port of the portable focus logic from `TerminalPaneView.swift`
/// (native/SupercliNative, `TerminalPaneContainer`):
///
/// - `effectiveActivePaneID`: the ONE focused pane id. Local view state
///   wins when it names a presented pane; then the store's
///   `activeTerminalPane` when its group matches and the pane is still
///   presented; otherwise the default (representative) pane.
/// - `paneIsFocused`: a multi-pane split uses its own active pane. A solo
///   / project-sidebar pane is focused only when it is the last solo pane
///   clicked (the store's `focusedSoloSessionID`) and focus is not held by
///   a split group; defaulting to the main area otherwise. This is what
///   makes "focus" meaningful ACROSS the main area and the project
///   sidebar, where each pane is its own solo container.
/// - `paneShowsActiveBorder`: the white active-focus border only reads when
///   the window holds more than one pane (a split, or the project sidebar
///   contributing panes beside the main one) and follows the focused pane.
/// - `showsPaneSplitControls`: split affordances show only on the focused
///   pane (the one wearing the white border) and the pane under the
///   pointer — never on every card. `effectiveActivePaneID` alone is
///   useless here: a solo container always reports itself active, which is
///   exactly what made the buttons show everywhere; the border logic
///   resolves the ONE globally focused pane.
/// - `paneIsWorking`: busy/starting/restarting/resuming — drives the chip's
///   spinner slot in `TerminalPaneTitleChip`.
///
/// The session status below mirrors Swift `SessionStatus`; map from the
/// Dart wire model (`SessionSummary`): activity "starting" → starting,
/// "working" → busy, "blocked" → attention, "idle"/"done" → idle, and
/// status "exited" → exited.
library;

/// Session status as `TerminalPaneView.swift` sees it.
enum PaneSessionStatus { starting, busy, attention, idle, exited }

/// Portable focus-resolution rules for terminal panes.
abstract final class TerminalPaneFocus {
  const TerminalPaneFocus._();

  /// Resolve the focused pane id.
  ///
  /// Mirrors Swift `effectiveActivePaneID`:
  /// 1. local [activePaneID] when it names a presented pane,
  /// 2. the store's [storeActivePaneID] when [storeActiveGroupID] matches
  ///    [groupID] and the pane is still presented,
  /// 3. [defaultActivePaneID].
  static String effectiveActivePaneID({
    required String? activePaneID,
    required List<String> presentedPaneIDs,
    required String? storeActivePaneID,
    required String? storeActiveGroupID,
    required String? groupID,
    required String defaultActivePaneID,
  }) {
    if (activePaneID != null && presentedPaneIDs.contains(activePaneID)) {
      return activePaneID;
    }
    if (storeActivePaneID != null &&
        storeActiveGroupID == groupID &&
        presentedPaneIDs.contains(storeActivePaneID)) {
      return storeActivePaneID;
    }
    return defaultActivePaneID;
  }

  /// Whether [paneID] is the focused pane.
  ///
  /// Mirrors Swift `paneIsFocused`: with more than one presented pane the
  /// container's own active pane decides; a solo pane is focused only when
  /// no split group holds focus and it is the last solo session clicked
  /// ([focusedSoloSessionID]), defaulting to the main area
  /// (![isAuxiliaryRegion]).
  static bool paneIsFocused({
    required int presentedPaneCount,
    required String paneID,
    required String effectiveActivePaneID,
    required bool storeHoldsActiveTerminalPane,
    required String? focusedSoloSessionID,
    required String? contentSessionID,
    required bool isAuxiliaryRegion,
  }) {
    if (presentedPaneCount > 1) {
      return effectiveActivePaneID == paneID;
    }
    if (storeHoldsActiveTerminalPane) return false;
    if (focusedSoloSessionID != null) {
      return contentSessionID == focusedSoloSessionID;
    }
    return !isAuxiliaryRegion;
  }

  /// Whether the pane wears the white active-focus border.
  ///
  /// Mirrors Swift `paneShowsActiveBorder`: only meaningful when the window
  /// holds more than one pane ([multiPane]: a split, or the project
  /// sidebar contributing panes beside the main one); then it follows the
  /// focused pane. Never while zoomed.
  static bool paneShowsActiveBorder({
    required bool zoomedPresent,
    required bool panePresented,
    required bool multiPane,
    required bool focused,
  }) {
    if (zoomedPresent) return false;
    if (!panePresented) return false;
    return multiPane && focused;
  }

  /// Whether the pane's split buttons are shown.
  ///
  /// Mirrors Swift `showsPaneSplitControls`: only on the focused pane and
  /// the pane under the pointer.
  static bool showsPaneSplitControls({
    required String? hoveredPaneID,
    required String paneID,
    required bool focused,
  }) =>
      hoveredPaneID == paneID || focused;

  /// Whether the title chip shows the working spinner in its logo slot.
  ///
  /// Mirrors Swift `paneIsWorking`: status starting/busy, or the session
  /// is restarting/resuming.
  static bool paneIsWorking({
    required PaneSessionStatus status,
    required bool restarting,
    required bool resumingAgent,
  }) =>
      status == PaneSessionStatus.starting ||
      status == PaneSessionStatus.busy ||
      restarting ||
      resumingAgent;
}
