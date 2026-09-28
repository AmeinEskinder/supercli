/// Tests for the SettingsView.swift port: Appearance and Remote panels.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/screens/appearancesettingspanel.dart';
import 'package:supercli_app/screens/remotesettingspanel.dart';
import 'package:supercli_app/screens/settingspanels.dart';
import 'package:test/test.dart';

void main() {
  group('AppearanceSettingsPanel (SettingsView.swift)', () {
    test('renders all sections', () {
      final panel = AppearanceSettingsPanel(settings: AppSettings());
      final node = panel.build() as UiColumn;
      // title, description, mode, tint, session titles, transparency,
      // terminal font, editor, terminal
      expect(node.children.length, 9);
      expect((node.children[0] as UiText).text, 'Appearance');
    });

    test('theme mode picker shows all modes with active marked', () {
      final panel = AppearanceSettingsPanel(
        settings: AppSettings(theme: ThemeMode.dark),
      );
      final node = panel.build() as UiColumn;
      final mode = node.children[2] as UiColumn;
      final picker = mode.children[2] as UiRow;
      expect(picker.children.length, ThemeMode.values.length);
      final darkButton = picker.children.firstWhere(
        (c) => (c as UiButton).id == 'theme-dark',
      ) as UiButton;
      expect(darkButton.label, startsWith('● '));
    });

    test('tint swatches cover all AppTints with active marked', () {
      final panel = AppearanceSettingsPanel(
        settings: AppSettings(accentColor: 1),
      );
      final node = panel.build() as UiColumn;
      final tint = node.children[3] as UiColumn;
      final swatches = tint.children[2] as UiRow;
      expect(swatches.children.length, AppTint.values.length);
      final blue = swatches.children[1] as UiButton;
      expect(blue.label, startsWith('● '));
      expect(AppTint.blue.title, 'Blue');
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
    });

    test('transparency rows show percentages', () {
      const row = TransparencySliderRow(title: 'Background', value: 0.85);
      final node = row.build() as UiRow;
      expect((node.children[1] as UiText).text, '85%');
    });

    test('session title mode titles', () {
      expect(SessionTitleMode.firstPrompt.title, 'First prompt');
      expect(SessionTitleMode.liveFromAgent.title, 'Live from agent');
      expect(SessionTitleMode.manual.title, 'Manual');
    });

    test('command-T action titles', () {
      expect(CommandTAction.newSession.title, 'New session');
      expect(CommandTAction.commandPalette.title, 'Command palette');
      expect(CommandTAction.quickOpen.title, 'Quick open');
    });
  });

  group('RemoteSettingsPanel (SettingsView.swift)', () {
    test('renders title, description, devices, share actions, iOS link', () {
      const panel = RemoteSettingsPanel();
      final node = panel.build() as UiColumn;
      expect((node.children[0] as UiText).text, 'Remote Control');
      expect(
        (node.children[1] as UiText).text,
        contains('control this Mac'),
      );
      // devices, share mac, share workspace, ios
      expect(node.children.length, 5);
    });

    test('description adapts to scope', () {
      const scoped = RemoteSettingsPanel(scopeName: 'Personal');
      expect(scoped.description, contains('Personal'));
      expect(scoped.description, contains('mints the credentials'));
    });

    test('empty devices shows empty text', () {
      const panel = RemoteSettingsPanel();
      final node = panel.build() as UiColumn;
      final devices = node.children[2] as UiColumn;
      expect((devices.children[1] as UiText).text, 'No controllers paired yet.');
    });

    test('paired devices render with revoke buttons', () {
      const panel = RemoteSettingsPanel(
        pairedDevices: [
          PairedDevice(id: 'd1', name: 'iPhone', pairedAt: '2026-09-28'),
        ],
      );
      final node = panel.build() as UiColumn;
      final devices = node.children[2] as UiColumn;
      expect(devices.children.length, 2); // title + 1 device row
      final row = devices.children[1] as UiRow;
      expect((row.children[0] as UiText).text, 'iPhone');
      expect((row.children[2] as UiButton).id, 'remote-device-revoke-d1');
    });

    test('iOS install URL is the stable superc.li link', () {
      expect(iosAppInstallUrl, 'https://superc.li/ios');
    });
  });

  group('ShareSheet (ShareThisMacSheet)', () {
    test('Mac vs workspace language', () {
      const mac = ShareSheet(pairingCode: '123', sshHostName: 'mac.local');
      expect(mac.title, 'Share This Mac');
      expect(mac.subtitle, contains('this Mac'));

      const ws = ShareSheet(
        pairingCode: '123',
        sshHostName: 'mac.local',
        usesWorkspaceLanguage: true,
      );
      expect(ws.title, 'Share This Workspace');
      expect(ws.subtitle, contains('this workspace'));
    });

    test('CLI command uses SSH transport, no pairing code', () {
      const sheet = ShareSheet(pairingCode: '999', sshHostName: 'mac.local');
      expect(sheet.cliCommand, 'supercli --host ssh://mac.local');
      expect(sheet.cliCommand, isNot(contains('999')));
    });

    test('pairing completed shows paired state', () {
      const sheet = ShareSheet(
        pairingCode: '123',
        sshHostName: 'mac.local',
        pairingCompleted: true,
      );
      final node = sheet.build() as UiColumn;
      final paired = node.children[3] as UiColumn;
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
  });
}
