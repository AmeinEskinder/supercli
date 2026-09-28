/// Terminal area state views: the non-terminal surfaces of the content pane.
///
/// Port of `TerminalArea.swift` (SupercliNative/Views, 1,461 lines). The
/// Swift file's `ContentArea` composition (settings/content swap, workspace
/// fade, delayed connecting banner) is Host/store orchestration that lives
/// in `bin/main.dart` + `lib/screens/terminalarea.dart`; this file ports the
/// discrete state views:
///
/// - `RestartRecommendedBar` — session-restart recommendation with
///   restart/dismiss actions.
/// - `ResumeFailedBar` — resume-failed banner with start-fresh/dismiss.
/// - `TerminalScrollButtonModel` + `TerminalScrollToBottomButton` —
///   scroll-to-bottom affordance visibility logic.
/// - `EmptyStateView` — no-session-selected placeholder.
/// - `StartingSessionView` — session-starting spinner state.
/// - `DeadSessionView` — exited session with resume affordance.
///
/// The NSViewRepresentable hosts (`WarmPaneHostView`, `TerminalHostView`)
/// are AppKit/Metal specifics with no Dart equivalent; the live terminal
/// surface is `lib/screens/terminalpaneview.dart::TerminalPaneView.hosted`.
library;

import 'package:gpuidart/gpuidart.dart';

/// A session-restart recommendation from the Host.
final class SessionRestartRecommendation {
  const SessionRestartRecommendation({this.actionLabel, required this.message});

  /// Label for the recommended action (e.g. "Restart"), or null when the
  /// recommendation is just "Context queued" with no action.
  final String? actionLabel;
  final String message;

  /// Headline text: "<label> recommended" or "Context queued".
  String get headline =>
      actionLabel != null ? '$actionLabel recommended' : 'Context queued';
}

/// Restart-recommended bar (TerminalArea.swift: RestartRecommendedBar).
///
/// Shows the recommendation headline + message, a restart button when an
/// action exists, and a dismiss button. Rendered as a UiRow; hover states
/// are framework-driven (see docs/gpuidart-gaps-sidebar.md).
final class RestartRecommendedBar {
  const RestartRecommendedBar({
    required this.recommendation,
    this.onRestart,
    this.onDismiss,
  });

  final SessionRestartRecommendation recommendation;
  final void Function()? onRestart;
  final void Function()? onDismiss;

  UiNode build() {
    return UiRow('restart-recommended-bar', [
      UiText(
        'restart-headline',
        recommendation.headline,
        style: const UiStyle(fontSize: 12, fontWeight: UiFontWeight.semibold),
      ),
      UiText(
        'restart-message',
        recommendation.message,
        style: const UiStyle(fontSize: 11),
      ),
      if (recommendation.actionLabel != null)
        UiButton(
          'restart-action',
          '↻ ${recommendation.actionLabel}',
        ),
      UiButton('restart-dismiss', '✕'),
    ]);
  }
}

/// Resume-failed banner (TerminalArea.swift: ResumeFailedBar).
///
/// Shown when a restart-with-resume relaunch died because the provider's
/// conversation storage no longer exists on disk. Same anatomy as
/// [RestartRecommendedBar]; the action relaunches fresh.
final class ResumeFailedBar {
  const ResumeFailedBar({this.onStartFresh, this.onDismiss});

  final void Function()? onStartFresh;
  final void Function()? onDismiss;

  UiNode build() {
    return UiRow('resume-failed-bar', [
      const UiText(
        'resume-failed-headline',
        "Couldn't resume the conversation",
        style: UiStyle(fontSize: 12, fontWeight: UiFontWeight.semibold),
      ),
      const UiText(
        'resume-failed-message',
        'Its history no longer exists on disk, so the agent can only start over.',
        style: UiStyle(fontSize: 11),
      ),
      const UiButton('resume-failed-fresh', '⊕ Start fresh'),
      const UiButton('resume-failed-dismiss', '✕'),
    ]);
  }
}

/// Scroll-to-bottom button model (TerminalArea.swift:
/// TerminalScrollButtonModel).
///
/// `visible` drives the button's opacity (0.88 resting → 1 on hover, 0 when
/// hidden), slide offset (0 vs 8pt), and hit-testing.
final class TerminalScrollButtonModel {
  TerminalScrollButtonModel({this.visible = false, this.onTap});

  bool visible;
  void Function()? onTap;

  /// Resting opacity: 0 when hidden, 0.88 when visible (1.0 on hover,
  /// which the framework handles).
  double get restingOpacity => visible ? 0.88 : 0.0;

  /// Slide offset: 0 when visible, 8pt down when hidden.
  double get slideOffset => visible ? 0.0 : 8.0;

  /// Hit-testing is only enabled while visible.
  bool get hitTestable => visible;
}

/// Scroll-to-bottom button (TerminalArea.swift:
/// TerminalScrollToBottomButton).
///
/// 36pt circle with a chevron-down glyph. Visibility comes from
/// [TerminalScrollButtonModel].
final class TerminalScrollToBottomButton {
  const TerminalScrollToBottomButton({required this.model});

  final TerminalScrollButtonModel model;

  UiNode build() {
    return UiButton(
      'scroll-to-bottom',
      '⌄',
    );
  }
}

/// Empty state (TerminalArea.swift: EmptyStateView).
///
/// "No session selected" + hint + version label. The Swift version shows a
/// pixel mascot animation; the Dart port shows the text (mascot assets are
/// not bundled with the Dart app).
final class EmptyStateView {
  const EmptyStateView({this.versionLabel = ''});

  final String versionLabel;

  UiNode build() {
    return UiColumn('empty-state', [
      const UiText(
        'empty-state-title',
        'No session selected',
        style: UiStyle(fontSize: 14),
      ),
      const UiText(
        'empty-state-hint',
        'Pick a session in the sidebar, or hit + on a project',
        style: UiStyle(fontSize: 11),
      ),
      if (versionLabel.isNotEmpty)
        UiText(
          'empty-state-version',
          versionLabel,
          style: const UiStyle(fontSize: 10),
        ),
    ]);
  }
}

/// Session-starting state (TerminalArea.swift: StartingSessionView).
///
/// Tool icon + session label + braille spinner while the session starts.
final class StartingSessionView {
  const StartingSessionView({required this.sessionLabel});

  final String sessionLabel;

  UiNode build() {
    return UiColumn('starting-session', [
      UiText(
        'starting-session-label',
        sessionLabel,
        style: const UiStyle(fontSize: 14, fontWeight: UiFontWeight.medium),
      ),
      const UiText('starting-session-spinner', '⠋'),
    ]);
  }
}

/// Exited-session state (TerminalArea.swift: DeadSessionView).
///
/// Shows the session label, "Session exited", and — when [canResume] — a
/// Resume button that swaps to a disabled "Resuming" in-flight state while
/// [isRestarting]. Unknown non-empty commands have no trustworthy resume
/// recipe, so [canResume] is false for them: the exited output stays
/// visible without offering a button that would silently start unrelated
/// fresh work.
final class DeadSessionView {
  const DeadSessionView({
    required this.sessionLabel,
    this.canResume = true,
    this.isRestarting = false,
    this.onResume,
  });

  final String sessionLabel;
  final bool canResume;
  final bool isRestarting;
  final void Function()? onResume;

  /// Whether the resume button is shown at all.
  bool get showsResumeButton => canResume;

  /// Resume button label: "Resuming" while a restart is in flight.
  String get resumeLabel => isRestarting ? 'Resuming' : 'Resume';

  /// The button is disabled while a restart is in flight.
  bool get resumeEnabled => !isRestarting;

  UiNode build() {
    return UiColumn('dead-session', [
      UiText(
        'dead-session-label',
        sessionLabel,
        style: const UiStyle(fontSize: 14, fontWeight: UiFontWeight.medium),
      ),
      const UiText(
        'dead-session-exited',
        'Session exited',
        style: UiStyle(fontSize: 11),
      ),
      if (showsResumeButton)
        UiButton('dead-session-resume', resumeLabel),
    ]);
  }
}
