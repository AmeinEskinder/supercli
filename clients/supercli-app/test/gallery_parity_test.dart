/// Tests for gallery, launcher pane, avatars, in-pane approvals.
/// Rows 175, 185–189 — [DESKTOP] parity.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/models.dart';
import 'package:supercli_app/screens/mcpapprovalpanel.dart';
import 'package:supercli_app/screens/sessiongallerypanel.dart';
import 'package:supercli_app/screens/sessionlauncherview.dart';
import 'package:supercli_app/screens/sessionscreenshotcapture.dart';
import 'package:supercli_app/screens/vieweravatarsview.dart';
import 'package:test/test.dart';

void main() {
  group('TransientLauncherPane (row 175)', () {
    test('shows agents and templates for the group', () {
      final pane = TransientLauncherPane(
        groupId: 'g1',
        agents: ['claude'],
        templates: ['blank', 'web'],
      );
      final node = pane.build() as UiColumn;
      final agents = node.children[2] as UiRow;
      expect((agents.children[0] as UiButton).label, 'claude');
      final templates = node.children[4] as UiRow;
      expect(templates.children.length, 2);
    });

    test('cancel and launch buttons present', () {
      final node =
          TransientLauncherPane(groupId: 'g1').build() as UiColumn;
      final buttons = node.children.last as UiRow;
      expect((buttons.children[0] as UiButton).label, 'Cancel');
      expect((buttons.children[1] as UiButton).label, 'Launch');
    });
  });

  group('FitToDesktopControl (row 185)', () {
    test('toggle reflects enabled state', () {
      final on = FitToDesktopControl(enabled: true).build() as UiRow;
      expect((on.children[0] as UiButton).label, contains('☑'));
      final off = FitToDesktopControl(enabled: false).build() as UiRow;
      expect((off.children[0] as UiButton).label, contains('☐'));
    });
  });

  group('InPaneApprovalOverlay (row 186)', () {
    PendingApproval approval() => const PendingApproval(
          id: 'ap1',
          tool: 'write',
          summary: 'Write file',
          detail: 'Writes /tmp/x',
        );

    test('write kind shows target path', () {
      final node = InPaneApprovalOverlay(
        approval: approval(),
        kind: ApprovalKind.write,
        target: '/tmp/x',
      ).build() as UiColumn;
      expect((node.children[0] as UiText).text, 'Write approval');
      expect((node.children[1] as UiText).text, '/tmp/x');
    });

    test('browser kind label', () {
      final node = InPaneApprovalOverlay(
        approval: approval(),
        kind: ApprovalKind.browser,
        target: 'https://example.com',
      ).build() as UiColumn;
      expect((node.children[0] as UiText).text, 'Browser approval');
    });

    test('app-open kind label', () {
      final node = InPaneApprovalOverlay(
        approval: approval(),
        kind: ApprovalKind.appOpen,
        target: 'Git',
      ).build() as UiColumn;
      expect((node.children[0] as UiText).text, 'App open approval');
    });
  });

  group('SessionGalleryPanel tabs (row 187)', () {
    test('three tabs with active marker', () {
      final node = SessionGalleryPanel(tab: GalleryTab.downloads).build()
          as UiColumn;
      final tabs = node.children[1] as UiRow;
      expect(tabs.children.length, 3);
      expect((tabs.children[1] as UiButton).label, contains('●'));
      expect((tabs.children[1] as UiButton).label, contains('Downloads'));
    });
  });

  group('SessionGalleryMarkup (row 188)', () {
    test('includes arrow and crop tools', () {
      final node = SessionGalleryMarkup().build() as UiColumn;
      final tools = node.children[0] as UiRow;
      final labels =
          tools.children.map((c) => (c as UiButton).label).toList();
      expect(labels.any((l) => l.contains('arrow')), isTrue);
      expect(labels.any((l) => l.contains('crop')), isTrue);
    });

    test('active tool marked and Add to prompt present', () {
      final markup = SessionGalleryMarkup(
        activeTool: 'crop',
        annotations: [
          {'tool': 'arrow', 'x1': 0, 'y1': 0, 'x2': 10, 'y2': 10},
        ],
      );
      final node = markup.build() as UiColumn;
      expect((node.children[1] as UiText).text, '1 annotations');
      expect((node.children[2] as UiButton).label, 'Add to prompt');
    });
  });

  group('SessionScreenshotCapture (row 189)', () {
    test('attach toggle reflects state', () {
      final attached =
          SessionScreenshotCapture(attachedToPrompt: true).build() as UiRow;
      expect((attached.children[3] as UiButton).label, contains('☑'));
    });

    test('shift+mod+s action exposed', () {
      final actions = SessionScreenshotCapture().actions();
      final take = actions.firstWhere((a) => a.name == 'screenshot.take');
      expect(take.keys, startsWith('shift+'));
      expect(take.keys, endsWith('+s'));
    });
  });
}
