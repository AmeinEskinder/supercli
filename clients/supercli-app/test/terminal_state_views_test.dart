/// Behaviour tests for the TerminalArea state-views port.
///
/// Covers the portable logic from `TerminalArea.swift` that does not depend
/// on SwiftUI:
///
/// - `terminal_state_views.dart` — restart/resume recommendation bars,
///   scroll-to-bottom visibility model, empty/starting/dead session states.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:test/test.dart';

import 'package:supercli_app/screens/terminal_state_views.dart';

/// Collect all UiText texts in a node tree (depth-first).
List<String> textsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiText) out.add(n.text);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

/// Collect all UiButton labels in a node tree (depth-first).
List<String> buttonsOf(UiNode node) {
  final out = <String>[];
  void walk(UiNode n) {
    if (n is UiButton) out.add(n.label);
    if (n is UiColumn) {
      for (final c in n.children) {
        walk(c);
      }
    }
    if (n is UiRow) {
      for (final c in n.children) {
        walk(c);
      }
    }
  }

  walk(node);
  return out;
}

void main() {
  group('SessionRestartRecommendation', () {
    test('headline uses the action label when present', () {
      const r = SessionRestartRecommendation(
        actionLabel: 'Restart',
        message: 'm',
      );
      expect(r.headline, 'Restart recommended');
    });

    test('headline falls back to "Context queued" without an action', () {
      const r = SessionRestartRecommendation(message: 'm');
      expect(r.headline, 'Context queued');
    });
  });

  group('RestartRecommendedBar', () {
    test('shows the restart button only when an action exists', () {
      const withAction = RestartRecommendedBar(
        recommendation: SessionRestartRecommendation(
          actionLabel: 'Restart',
          message: 'm',
        ),
      );
      const withoutAction = RestartRecommendedBar(
        recommendation: SessionRestartRecommendation(message: 'm'),
      );
      expect(buttonsOf(withAction.build()), contains('↻ Restart'));
      expect(buttonsOf(withoutAction.build()), isNot(contains('↻ Restart')));
      // Dismiss is always present.
      expect(buttonsOf(withAction.build()), contains('✕'));
      expect(buttonsOf(withoutAction.build()), contains('✕'));
    });

    test('headline text matches the recommendation', () {
      const bar = RestartRecommendedBar(
        recommendation: SessionRestartRecommendation(
          actionLabel: 'Restart',
          message: 'ctx',
        ),
      );
      final texts = textsOf(bar.build());
      expect(texts, contains('Restart recommended'));
      expect(texts, contains('ctx'));
    });
  });

  group('ResumeFailedBar', () {
    test('shows start-fresh and dismiss buttons', () {
      const bar = ResumeFailedBar();
      final labels = buttonsOf(bar.build());
      expect(labels, contains('⊕ Start fresh'));
      expect(labels, contains('✕'));
    });

    test('shows the resume-failed copy', () {
      const bar = ResumeFailedBar();
      final texts = textsOf(bar.build());
      expect(texts, contains("Couldn't resume the conversation"));
    });
  });

  group('TerminalScrollButtonModel', () {
    test('hidden: opacity 0, offset 8, not hit-testable', () {
      final m = TerminalScrollButtonModel(visible: false);
      expect(m.restingOpacity, 0.0);
      expect(m.slideOffset, 8.0);
      expect(m.hitTestable, isFalse);
    });

    test('visible: opacity 0.88, offset 0, hit-testable', () {
      final m = TerminalScrollButtonModel(visible: true);
      expect(m.restingOpacity, 0.88);
      expect(m.slideOffset, 0.0);
      expect(m.hitTestable, isTrue);
    });
  });

  group('EmptyStateView', () {
    test('shows title and hint', () {
      const v = EmptyStateView();
      final texts = textsOf(v.build());
      expect(texts, contains('No session selected'));
      expect(
        texts,
        contains('Pick a session in the sidebar, or hit + on a project'),
      );
    });

    test('version label shown only when non-empty', () {
      expect(textsOf(const EmptyStateView().build()), hasLength(2));
      expect(
        textsOf(const EmptyStateView(versionLabel: 'v1.2.3').build()),
        contains('v1.2.3'),
      );
    });
  });

  group('StartingSessionView', () {
    test('shows the session label and a spinner', () {
      const v = StartingSessionView(sessionLabel: 'my session');
      final texts = textsOf(v.build());
      expect(texts, contains('my session'));
      expect(texts.any((t) => t.contains('⠋')), isTrue);
    });
  });

  group('DeadSessionView', () {
    test('shows label, exited text, and resume button by default', () {
      const v = DeadSessionView(sessionLabel: 's');
      expect(v.showsResumeButton, isTrue);
      expect(v.resumeLabel, 'Resume');
      expect(v.resumeEnabled, isTrue);
      expect(buttonsOf(v.build()), contains('Resume'));
    });

    test('hides the resume button when canResume is false', () {
      const v = DeadSessionView(sessionLabel: 's', canResume: false);
      expect(v.showsResumeButton, isFalse);
      expect(buttonsOf(v.build()), isNot(contains('Resume')));
    });

    test('in-flight restart swaps the label and disables the button', () {
      const v = DeadSessionView(sessionLabel: 's', isRestarting: true);
      expect(v.resumeLabel, 'Resuming');
      expect(v.resumeEnabled, isFalse);
      expect(buttonsOf(v.build()), contains('Resuming'));
    });
  });
}
