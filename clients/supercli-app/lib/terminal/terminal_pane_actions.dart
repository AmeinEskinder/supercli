/// Write-path action decisions for the terminal pane container.
///
/// Ports the portable mutation logic from `TerminalPaneView.swift`
/// (native/SupercliNative, `TerminalPaneContainer`, ~2,224 lines) that the
/// earlier surface-logic port left behind. The read-path focus rules live in
/// `terminal_pane_focus.dart`; this file covers the write path — the
/// decisions a container makes before touching view state or the store:
///
/// - `resolveCloseActivePane`: Ghostty-style ⌘W on the focused leaf.
///   Resolves the focused pane via `effectiveActivePaneID`, computes the
///   close action, and applies the stale-slot guard: a defensive pane whose
///   session no longer resolves must never turn ⌘W into an unknown-id Host
///   mutation, so it falls back to detach.
/// - `resolveActivatePane`: click/hover activation. Membership-guarded;
///   solo containers clear the group-scoped store state and remember the
///   focused solo session so the white border can follow focus across the
///   main area and the project sidebar.
/// - `resolveSyncActivePane`: remount restore (e.g. after Settings).
///   Re-affirms a still-valid store-active pane, else falls back to the
///   representative.
/// - `resolveFocusRequest`: the sidebar's exact-terminal focus request.
///   Five guards (group match, representative match, pane membership,
///   request still pending) before activation; the transport-specific focus
///   (remote runtime vs local surface cache) stays with the caller.
/// - `resolveClaimPendingReveal`: one-shot insertion-marker claim — the
///   inserted pane is focused in place with no entrance motion.
/// - `resolveZoomedPane`: which pane the zoom targets, when the zoomed
///   pane belongs to this group and is still present. (Only the zoomed
///   leaf renders — that rendering rule is AppKit-side and stays dropped.)
/// - `liveDividerRatio`: the divider's live ratio during a drag — the
///   drag's ratio while its path key matches, else the model's ratio.
/// - `shouldDetachPane` / `shouldLaunchIntoPane`: the synthetic-pane
///   guards on detach and preset-launch.
/// - `paneBanners`: which in-pane banners show. Chrome that reads this
///   instance's own state stays local-only; a restart recommendation and a
///   resume failure are independent and may both show.
///
/// All decisions are pure: the caller applies them to view state / the
/// store. Mirrors the Swift method bodies 1:1; see the per-method notes.
library;

import 'terminal_pane_close.dart';
import 'terminal_pane_model.dart';

/// What ⌘W (close-active-pane) does after resolving the focused leaf.
///
/// Mirrors Swift `closeActivePane()`:
/// ```swift
/// guard let pane = presentedPanes.first(where: {
///     $0.paneID == effectiveActivePaneID
/// }) else { return }
/// let sessionID = pane.content.sessionID
/// let action = terminalPaneCloseAction(
///     for: pane.content,
///     canArchiveSession: sessionID.map(store.sessionCanArchive) ?? false
/// )
/// switch action {
/// case .detachPane: detachPane(pane)
/// case let .removeSession(sessionID):
///     // A stale defensive slot must never turn ⌘W into an unknown-id
///     // Host mutation. Real mounted Session panes always resolve here.
///     guard entry(for: sessionID) != nil else { detachPane(pane); return }
///     store.confirmRemoveSession(sessionID)
/// case let .confirmArchive(sessionID):
///     guard let entry = entry(for: sessionID) else { detachPane(pane); return }
///     paneConfirmation = PaneConfirmation(
///         action: .archive, sessionID: sessionID,
///         label: entry.label, isLive: entry.isLive)
/// }
/// ```
sealed class CloseActivePaneDecision {
  const CloseActivePaneDecision();
}

/// No pane resolved for the focused id — ⌘W is a no-op.
final class CloseActivePaneNone extends CloseActivePaneDecision {
  const CloseActivePaneNone();
}

/// Detach the pane (launcher, or stale slot whose session is gone).
final class CloseActivePaneDetach extends CloseActivePaneDecision {
  const CloseActivePaneDetach(this.paneId);
  final String paneId;
}

/// Ask the store to confirm-remove the session.
final class CloseActivePaneRemove extends CloseActivePaneDecision {
  const CloseActivePaneRemove(this.sessionId);
  final String sessionId;
}

/// Raise the in-app archive confirmation card for the session.
final class CloseActivePaneConfirmArchive extends CloseActivePaneDecision {
  const CloseActivePaneConfirmArchive({
    required this.sessionId,
    required this.label,
    required this.isLive,
  });
  final String sessionId;
  final String label;
  final bool isLive;
}

/// Minimal session entry view needed by the close decision.
class ClosePaneSessionEntry {
  const ClosePaneSessionEntry({
    required this.id,
    required this.label,
    required this.isLive,
  });
  final String id;
  final String label;
  final bool isLive;
}

/// Decide what ⌘W does.
///
/// - [presentedPanes]: the container's current panes.
/// - [effectiveActivePaneID]: the resolved focused pane id (see
///   `TerminalPaneFocus.effectiveActivePaneID`).
/// - [canArchiveSession]: whether the focused session can archive
///   (`store.sessionCanArchive`).
/// - [entryFor]: session entry lookup (`entry(for:)`); null when the
///   session no longer resolves (stale defensive slot → detach).
CloseActivePaneDecision resolveCloseActivePane({
  required List<PresentedPane> presentedPanes,
  required String effectiveActivePaneID,
  required bool Function(String sessionId) canArchiveSession,
  required ClosePaneSessionEntry? Function(String sessionId) entryFor,
}) {
  PresentedPane? focused;
  for (final pane in presentedPanes) {
    if (pane.paneId == effectiveActivePaneID) {
      focused = pane;
      break;
    }
  }
  if (focused == null) return const CloseActivePaneNone();
  final pane = focused;

  final sessionId = switch (pane.content) {
    SessionContent(:final sessionId) => sessionId,
    LauncherContent() => null,
  };
  final action = terminalPaneCloseAction(
    sessionId: sessionId,
    canArchiveSession:
        sessionId == null ? false : canArchiveSession(sessionId),
  );
  if (action == TerminalPaneCloseAction.detachPane) {
    return CloseActivePaneDetach(pane.paneId);
  }
  // `detachPane` is the only null-session outcome, so `sessionId` is the
  // action's session id here.
  final id = sessionId!;
  final entry = entryFor(id);
  // Stale defensive slot: never turn ⌘W into an unknown-id Host mutation —
  // fall back to detaching the pane.
  if (entry == null) return CloseActivePaneDetach(pane.paneId);
  if (action == TerminalPaneCloseAction.removeSession(id)) {
    return CloseActivePaneRemove(id);
  }
  return CloseActivePaneConfirmArchive(
    sessionId: id,
    label: entry.label,
    isLive: entry.isLive,
  );
}

/// What pane activation does.
///
/// Mirrors Swift `activatePane(_:)`:
/// ```swift
/// guard presentedPanes.contains(where: { $0.paneID == paneID }) else { return }
/// if activePaneID != paneID { activePaneID = paneID }
/// guard let group else {
///     store.clearActiveTerminalPane()
///     // Solo/panel pane: remember it as the focused pane so the white
///     // border can follow focus across the main area and project sidebar.
///     store.setFocusedSoloSession(
///         presentedPanes.first(where: { $0.paneID == paneID })?.content.sessionID)
///     return
/// }
/// let sessionID = presentedPanes.first(where: { $0.paneID == paneID })?
///     .content.sessionID
/// store.setActiveTerminalPane(groupID: group.id, paneID: paneID, sessionID: sessionID)
/// ```
sealed class ActivatePaneDecision {
  const ActivatePaneDecision();
}

/// The pane is not presented — ignore the activation.
final class ActivatePaneIgnored extends ActivatePaneDecision {
  const ActivatePaneIgnored();
}

/// Solo container: set the local active pane, clear the group-scoped store
/// state, and remember the focused solo session (null for a launcher).
final class ActivatePaneSolo extends ActivatePaneDecision {
  const ActivatePaneSolo({required this.paneId, required this.sessionId});
  final String paneId;
  final String? sessionId;
}

/// Grouped container: set the local active pane and the store's active
/// terminal pane.
final class ActivatePaneGrouped extends ActivatePaneDecision {
  const ActivatePaneGrouped({
    required this.paneId,
    required this.groupId,
    required this.sessionId,
  });
  final String paneId;
  final String groupId;
  final String? sessionId;
}

/// Decide what activating [paneId] does.
///
/// - [presentedPanes]: the container's current panes.
/// - [groupId]: the group's id, or null for a solo container.
ActivatePaneDecision resolveActivatePane({
  required String paneId,
  required List<PresentedPane> presentedPanes,
  required String? groupId,
}) {
  PresentedPane? pane;
  for (final p in presentedPanes) {
    if (p.paneId == paneId) {
      pane = p;
      break;
    }
  }
  if (pane == null) return const ActivatePaneIgnored();
  final sessionId = switch (pane.content) {
    SessionContent(:final sessionId) => sessionId,
    LauncherContent() => null,
  };
  if (groupId == null) {
    return ActivatePaneSolo(paneId: paneId, sessionId: sessionId);
  }
  return ActivatePaneGrouped(
    paneId: paneId,
    groupId: groupId,
    sessionId: sessionId,
  );
}

/// What a container remount does to restore the transient active pane.
///
/// Mirrors Swift `syncActivePane()`:
/// ```swift
/// guard let group else {
///     activePaneID = defaultActivePaneID
///     store.clearActiveTerminalPane()
///     return
/// }
/// if let active = store.activeTerminalPane,
///    active.groupID == group.id,
///    group.panes.contains(where: { $0.id == active.paneID }) {
///     activePaneID = active.paneID
///     let sessionID = group.panes.first(where: { $0.id == active.paneID })?
///         .content.sessionID
///     store.setActiveTerminalPane(
///         groupID: group.id, paneID: active.paneID, sessionID: sessionID)
///     return
/// }
/// activatePane(defaultActivePaneID)
/// ```
sealed class SyncActivePaneDecision {
  const SyncActivePaneDecision();
}

/// Solo container: reset to the default pane and clear store state.
final class SyncActivePaneResetSolo extends SyncActivePaneDecision {
  const SyncActivePaneResetSolo(this.defaultPaneId);
  final String defaultPaneId;
}

/// The store's active pane is still valid: keep it and re-affirm the
/// store state (session id re-resolved from the group).
final class SyncActivePaneReaffirm extends SyncActivePaneDecision {
  const SyncActivePaneReaffirm({
    required this.paneId,
    required this.groupId,
    required this.sessionId,
  });
  final String paneId;
  final String groupId;
  final String? sessionId;
}

/// Fall back to activating the default pane (route through
/// [resolveActivatePane]).
final class SyncActivePaneActivateDefault extends SyncActivePaneDecision {
  const SyncActivePaneActivateDefault(this.defaultPaneId);
  final String defaultPaneId;
}

/// Decide what a remount does.
///
/// - [groupId]: the group's id, or null for a solo container.
/// - [groupPaneIds]: ids of the group's panes.
/// - [sessionIdFor]: session id for a group pane id, or null.
/// - [storeActivePaneId]/[storeActiveGroupId]: the store's active pane.
/// - [defaultActivePaneID]: the fallback pane.
SyncActivePaneDecision resolveSyncActivePane({
  required String? groupId,
  required List<String> groupPaneIds,
  required String? Function(String paneId) sessionIdFor,
  required String? storeActivePaneId,
  required String? storeActiveGroupId,
  required String defaultActivePaneID,
}) {
  if (groupId == null) {
    return SyncActivePaneResetSolo(defaultActivePaneID);
  }
  if (storeActivePaneId != null &&
      storeActiveGroupId == groupId &&
      groupPaneIds.contains(storeActivePaneId)) {
    return SyncActivePaneReaffirm(
      paneId: storeActivePaneId,
      groupId: groupId,
      sessionId: sessionIdFor(storeActivePaneId),
    );
  }
  return SyncActivePaneActivateDefault(defaultActivePaneID);
}

/// Whether a sidebar focus request activates its pane.
///
/// Mirrors Swift `focusRequestedPane(_:)`:
/// ```swift
/// guard let group,
///       request.groupID == group.id,
///       group.representativeSessionID == store.selectedSessionID,
///       group.panes.contains(where: { $0.id == request.paneID }),
///       store.terminalPaneFocusRequest == request
/// else { return }
/// activatePane(request.paneID)
/// guard store.consumeTerminalPaneFocus(request) else { return }
/// // ... transport-specific focus (remote runtime vs local cache)
/// ```
///
/// Activation is established before the surface exists: local and remote
/// mounts both focus themselves when their `isActive` input becomes true,
/// so a slow cold mount cannot lose the user's intent.
///
/// Returns the pane id to activate, or null when the request is stale.
/// Consuming the request and performing the transport-specific focus stay
/// with the caller.
String? resolveFocusRequest({
  required String? groupId,
  required String requestGroupId,
  required String requestPaneId,
  required String groupRepresentativeSessionId,
  required String? selectedSessionId,
  required List<String> groupPaneIds,
  required bool requestStillPending,
}) {
  if (groupId == null) return null;
  if (requestGroupId != groupId) return null;
  if (groupRepresentativeSessionId != selectedSessionId) return null;
  if (!groupPaneIds.contains(requestPaneId)) return null;
  if (!requestStillPending) return null;
  return requestPaneId;
}

/// Which pane a pending one-shot reveal marker claims.
///
/// Mirrors Swift `claimPendingReveal()`:
/// ```swift
/// guard let group,
///       let pending = store.pendingPaneReveal(in: group.id),
///       presentedPanes.contains(where: { $0.paneID == pending })
/// else { return }
/// _ = store.consumePaneReveal(groupID: group.id, paneID: pending)
/// activatePane(pending)
/// ```
///
/// Returns the pane id to activate (the caller consumes the marker), or
/// null when there is nothing to claim.
String? resolveClaimPendingReveal({
  required String? groupId,
  required String? pendingPaneId,
  required List<String> presentedPaneIds,
}) {
  if (groupId == null) return null;
  if (pendingPaneId == null) return null;
  if (!presentedPaneIds.contains(pendingPaneId)) return null;
  return pendingPaneId;
}

/// Which pane the zoom targets.
///
/// Mirrors Swift `zoomedPane`:
/// ```swift
/// guard let group,
///       let zoomed = store.zoomedTerminalPane,
///       zoomed.groupID == group.id
/// else { return nil }
/// return presentedPanes.first(where: { $0.paneID == zoomed.paneID })
/// ```
///
/// Returns the pane id, or null when no zoom applies to this container.
String? resolveZoomedPane({
  required String? groupId,
  required String? zoomedGroupId,
  required String? zoomedPaneId,
  required List<String> presentedPaneIds,
}) {
  if (groupId == null) return null;
  if (zoomedGroupId != groupId) return null;
  if (zoomedPaneId == null) return null;
  if (!presentedPaneIds.contains(zoomedPaneId)) return null;
  return zoomedPaneId;
}

/// The divider's live ratio during a drag.
///
/// Mirrors Swift `liveRatio(of:at:)`:
/// ```swift
/// if let drag = dividerDrag, drag.pathKey == Self.pathKey(path) {
///     return drag.ratio
/// }
/// return CGFloat(split.ratio)
/// ```
///
/// The live ratio stays local during the drag (never commits to the model
/// mid-gesture, so structural identity stays stable); the model commits
/// once on release.
double liveDividerRatio({
  required String? dragPathKey,
  required double? dragRatio,
  required String pathKey,
  required double splitRatio,
}) {
  if (dragPathKey != null && dragPathKey == pathKey && dragRatio != null) {
    return dragRatio;
  }
  return splitRatio;
}

/// Whether detaching applies — mirrors Swift `detachPane(_:)`'s guard:
/// a synthetic (solo, no-group) pane has nothing to detach.
bool shouldDetachPane({required bool isSynthetic}) => !isSynthetic;

/// Whether launching a preset into the pane applies — mirrors Swift
/// `launch(_:into:)`'s guard: a synthetic pane cannot host a launch.
bool shouldLaunchIntoPane({required bool isSynthetic}) => !isSynthetic;

/// Which in-pane banners show for a session.
///
/// Mirrors Swift `paneBanners(for:background:)`'s selection (rendering of
/// the bars themselves is UI):
/// ```swift
/// if isOwnLocalScope {
///     if let recommendation = store.restartRecommendations[entry.id] {
///         RestartRecommendedBar(...)
///     }
///     if store.resumeFailures.contains(entry.id) {
///         ResumeFailedBar(...)
///     }
/// }
/// ```
///
/// Chrome that reads this instance's own state (restart recommendations,
/// resume failures) stays local-only even though another local workspace
/// now shares the attach transport. The two banners are independent and
/// may both show.
enum PaneBannerKind {
  /// The session has a pending restart recommendation.
  restartRecommended,

  /// A previous resume of the session failed.
  resumeFailed,
}

Set<PaneBannerKind> paneBanners({
  required bool isOwnLocalScope,
  required bool hasRestartRecommendation,
  required bool hasResumeFailure,
}) {
  if (!isOwnLocalScope) return const {};
  return {
    if (hasRestartRecommendation) PaneBannerKind.restartRecommended,
    if (hasResumeFailure) PaneBannerKind.resumeFailed,
  };
}
