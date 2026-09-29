/// Tests for the SettingsView.swift port: Appearance and Remote panels.
/// Ports the Swift `AppearanceSettingsPanel` (2495-2730),
/// `TerminalFontSection` (2731-2871), `TransparencySliderRow` (2872-2903),
/// `AppTintSwatch` (2904-2931), `RemoteSettingsPanel` (2932-3329),
/// `ShareThisMacSheet` (3330-3517), `ShareWorkspaceSheet` (3518-3679),
/// `LinkEnrollmentSection` (3680-3822) and `PairingQRCodeView` (3857-3908).
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/appearancesettingspanel.dart';
import 'package:supercli_app/screens/remotesettingspanel.dart';
import 'package:supercli_app/screens/settingspanels.dart';
import 'package:test/test.dart';

void main() {
  group('AppearanceSettingsPanel (SettingsView.swift)', () {
    test('renders header + 7 sections', () {
      final panel = AppearanceSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      // header, mode, tint, session titles, transparency, terminal font,
      // open resources (+editor), terminal (+session gallery toggle)
      expect(node.children.length, 8);
      final header = node.children[0] as UiColumn;
      expect((header.children[0] as UiText).text, 'Appearance');
    });

    test('theme mode picker shows all modes with active marked', () {
      final panel = AppearanceSettingsPanel(
        settings: AppSettings(theme: ThemeMode.dark),
      );
      final node = panel.build() as UiColumn;
      final mode = node.children[1] as UiColumn;
      final picker = mode.children[1] as UiRow;
      expect(picker.children.length, ThemeMode.values.length);
      final darkButton = picker.children.firstWhere(
        (c) => (c as UiButton).id == 'theme-dark',
      ) as UiButton;
      expect(darkButton.label, startsWith('● '));
    });

    test('tint swatches cover all AppTints with active marked', () {
      // AppTint order: none(0), peel(1), amber(2), green(3), teal(4),
      // blue(5), indigo(6), violet(7).
      final panel = AppearanceSettingsPanel(
        settings: AppSettings(accentColor: 5),
      );
      final node = panel.build() as UiColumn;
      final tint = node.children[2] as UiColumn;
      final swatches = tint.children[1] as UiRow;
      expect(swatches.children.length, AppTint.values.length);
      final blue = swatches.children[5] as UiButton;
      expect(blue.label, startsWith('● '));
      expect(AppTint.none.title, 'Default');
      expect(AppTint.peel.title, 'Peel');
      expect(AppTint.blue.title, 'Blue');
      expect(AppTint.violet.title, 'Violet');
    });

    test('non-default instance shows the inherit section', () {
      final panel = AppearanceSettingsPanel(
        settings: AppSettings(),
        isDefaultInstance: false,
        defaultWorkspaceLabel: 'Personal',
      );
      final node = panel.build() as UiColumn;
      expect(node.children.length, 9);
      final inherit = node.children[1] as UiColumn;
      expect(inherit.id, 'appearance-inherit');
      final button = inherit.children[1] as UiButton;
      expect(button.id, 'appearance-use-inherited');
      expect(button.label, contains('Personal'));
    });

    test('terminal font section shows family, size, line height', () {
      final section = TerminalFontSection(
        family: 'JetBrains Mono',
        size: 14.0,
        lineHeight: 1.4,
      );
      final node = section.build() as UiColumn;
      expect(node.children.length, 4);
      final sizeRow = node.children[2] as UiRow;
      expect((sizeRow.children[1] as UiText).text, '14.0 pt');
      final familyRow = node.children[1] as UiRow;
      expect((familyRow.children[1] as UiText).text, 'JetBrains Mono');
    });

    test('transparency rows show percentages', () {
      const row = TransparencySliderRow(title: 'Background', value: 0.85);
      final node = row.build() as UiRow;
      expect((node.children[1] as UiText).text, '85%');
    });

    test('session title mode titles', () {
      expect(SessionTitleMode.firstPrompt.title, 'First prompt');
      expect(SessionTitleMode.agent.title, 'Live from agent');
      expect(SessionTitleMode.off.title, 'Off');
    });

    test('command-T action titles', () {
      expect(CommandTAction.newTerminal.title, 'New terminal');
      expect(CommandTAction.presetPicker.title, 'Preset screen');
    });
  });

  group('RemoteSettingsPanel (SettingsView.swift)', () {
    test('this-Mac scope renders controls, banner, enrollment, security', () {
      const panel = RemoteSettingsPanel();
      final node = panel.build() as UiColumn;
      final header = node.children[0] as UiColumn;
      expect((header.children[0] as UiText).text, 'Remote Control');
      expect(panel.description, contains('control this Mac'));
      expect(node.children.length, 5);
      expect((node.children[1] as UiColumn).id, 'remote-controls');
      expect((node.children[2] as UiColumn).id, 'testflight-banner');
      expect((node.children[4] as UiColumn).id, 'remote-security');
    });

    test('description adapts to scope', () {
      const scoped = RemoteSettingsPanel(scopeName: 'Personal');
      expect(scoped.description, contains('Personal'));
      expect(scoped.description, contains('mints the credentials'));

      const ws = RemoteSettingsPanel(
        scopeKind: RemoteScopeKind.localWorkspace,
        scopedWorkspaceName: 'API',
      );
      expect(ws.description, contains('API'));
      expect(ws.description, contains('pairs its own devices'));
    });

    test('empty devices shows empty text', () {
      const panel = RemoteSettingsPanel();
      final node = panel.build() as UiColumn;
      final controls = node.children[1] as UiColumn;
      // section header, local-access value row, empty text, share button
      expect((controls.children[2] as UiText).text, 'No paired devices.');
    });

    test('paired devices render with revoke buttons', () {
      const panel = RemoteSettingsPanel(
        devices: [
          PairedDevice(id: 'd1', name: 'iPhone'),
        ],
      );
      final node = panel.build() as UiColumn;
      final controls = node.children[1] as UiColumn;
      final devices = controls.children[2] as UiColumn;
      expect(devices.id, 'remote-controls-devices');
      final row = devices.children[0] as UiRow;
      expect(row.id, 'remote-device-d1');
      final info = row.children[0] as UiColumn;
      expect((info.children[0] as UiText).text, 'iPhone');
      expect((row.children[2] as UiButton).id, 'remote-device-revoke-d1');
    });

    test('remote-Host scope shows scoped controls section', () {
      const panel = RemoteSettingsPanel(
        scopeKind: RemoteScopeKind.remoteHost,
        scopeName: 'Office Mac',
        remoteHostSupportsPairingInvitation: true,
      );
      final node = panel.build() as UiColumn;
      final host = node.children[1] as UiColumn;
      expect(host.id, 'remote-host');
      final pairButton = host.children[1] as UiButton;
      expect(pairButton.id, 'remote-host-pair');
      expect(pairButton.label, contains('Office Mac'));
    });

    test('scoped workspace section lists scoped devices read-only', () {
      const panel = RemoteSettingsPanel(
        scopeKind: RemoteScopeKind.localWorkspace,
        scopedWorkspaceName: 'API',
        scopedDevices: [
          ScopedPairedDevice(id: 's1', name: 'iPad'),
        ],
      );
      final node = panel.build() as UiColumn;
      final section = node.children[1] as UiColumn;
      expect(section.id, 'remote-scoped-workspace');
      final devices = section.children[1] as UiColumn;
      expect((devices.children[0] as UiColumn).id, 'remote-scoped-device-s1');
      expect((node.children[2] as UiColumn).id, 'testflight-banner');
    });

    test('iOS install URL is the stable superc.li link', () {
      expect(iosAppInstallUrl, 'https://superc.li/ios');
    });

    test('TestFlight banner invites to the beta', () {
      const banner = TestFlightBanner();
      final node = banner.build() as UiColumn;
      expect((node.children[1] as UiText).text, contains('beta'));
      final join = node.children[3] as UiButton;
      expect(join.id, 'testflight-join');
      expect(join.label, 'Join the Beta');
    });
  });

  group('ShareThisMacSheet (SettingsView.swift)', () {
    test('Mac vs workspace language', () {
      const mac = ShareThisMacSheet(code: '123', sshHostName: 'mac.local');
      expect(mac.title, 'Share This Mac');
      expect(mac.subtitle, contains('this Mac'));

      const ws = ShareThisMacSheet(
        code: '123',
        sshHostName: 'mac.local',
        usesWorkspaceLanguage: true,
      );
      expect(ws.title, 'Share This Workspace');
      expect(ws.subtitle, contains('this workspace'));
    });

    test('CLI command uses SSH transport, no pairing code', () {
      const sheet = ShareThisMacSheet(code: '999', sshHostName: 'mac.local');
      expect(sheet.cliCommand, 'supercli --host ssh://mac.local');
      expect(sheet.cliCommand, isNot(contains('999')));
    });

    test('pairing completed shows paired state', () {
      const sheet = ShareThisMacSheet(
        code: '123',
        sshHostName: 'mac.local',
        phase: ShareThisMacPhase.paired,
      );
      final node = sheet.build() as UiColumn;
      final paired = node.children[2] as UiColumn;
      expect(paired.id, 'share-paired');
      expect((paired.children[0] as UiText).text, 'Controller paired');
    });

    test('pairing QR shows code and expiry', () {
      const qr = PairingQRCodeView(
        payload: 'ABC-123',
        expiresInText: 'Expires in 4:59',
      );
      final node = qr.build() as UiColumn;
      expect((node.children[0] as UiText).text, 'ABC-123');
      expect((node.children[1] as UiText).text, 'Expires in 4:59');
    });

    test('workspace sheet mints per workspace', () {
      const sheet = ShareWorkspaceSheet(
        workspaceName: 'API',
        phase: ShareWorkspacePhase.paired,
      );
      expect(sheet.title, 'Share API');
      final node = sheet.build() as UiColumn;
      final paired = node.children[2] as UiColumn;
      expect(paired.id, 'share-workspace-paired');
      expect((paired.children[0] as UiText).text, 'Controller paired');
    });
  });
}
