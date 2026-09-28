/// Remote Control settings panel.
///
/// Port of `RemoteSettingsPanel` (SettingsView.swift, 2932-3330) with
/// `ShareThisMacSheet` (3330-3518), `ShareWorkspaceSheet` (3518-3857), and
/// `PairingQRCodeView` (3857-3903).
///
/// Pairing follows the scope (one pairing = one workspace): this tab is
/// per-workspace. The iOS app install link is always https://superc.li/ios
/// (the site 302s to the current TestFlight/App Store page, so shipped
/// builds never hold a stale store URL).
///
/// Renders through the RLE fallback pattern since gpuidart has no native
/// settings widgets.
library;

import 'package:gpuidart/gpuidart.dart';

/// Install link for the iPhone app (Swift: `RemoteSettingsPanel.iosAppURL`).
const String iosAppInstallUrl = 'https://superc.li/ios';

/// A paired controller device (Swift: `ScopedPairedDevice`).
final class PairedDevice {
  const PairedDevice({
    required this.id,
    required this.name,
    required this.pairedAt,
  });

  final String id;
  final String name;
  final String pairedAt;
}

/// Pairing QR code view (Swift: `PairingQRCodeView`).
///
/// Renders the one-time pairing payload as a QR code. The terminal
/// counterpart is the `supercli --host ssh://<hostname>` CLI command —
/// SSH is a transport for the same Host contract, no pairing code involved.
final class PairingQRCodeView {
  const PairingQRCodeView({
    required this.payload,
    this.expiresInText = '',
  });

  final String payload;
  final String expiresInText;

  UiNode build() {
    return UiColumn('pairing-qr', [
      UiText('pairing-qr-payload', payload),
      if (expiresInText.isNotEmpty)
        UiText('pairing-qr-expiry', expiresInText),
    ]);
  }
}

/// Share sheet (Swift: `ShareThisMacSheet` / `ShareWorkspaceSheet`).
///
/// "Share This Mac" or "Share This Workspace" depending on
/// [usesWorkspaceLanguage]. Shows the pairing QR, the SSH CLI command,
/// and the paired-device state.
final class ShareSheet {
  const ShareSheet({
    required this.pairingCode,
    required this.sshHostName,
    this.usesWorkspaceLanguage = false,
    this.pairingCompleted = false,
    this.error,
    this.expiresInText = '',
  });

  final String pairingCode;
  final String sshHostName;
  final bool usesWorkspaceLanguage;
  final bool pairingCompleted;
  final String? error;
  final String expiresInText;

  String get title =>
      usesWorkspaceLanguage ? 'Share This Workspace' : 'Share This Mac';

  String get subtitle => usesWorkspaceLanguage
      ? 'Let another Supercli device control this workspace.'
      : 'Let another Supercli device control this Mac.';

  /// The terminal counterpart of the QR code (Swift: `cliCommand`).
  String get cliCommand => 'supercli --host ssh://$sshHostName';

  UiNode build() {
    return UiColumn('share-sheet', [
      UiText('share-title', title),
      UiText('share-subtitle', subtitle),
      if (error != null) UiText('share-error', error!),
      if (pairingCompleted)
        UiColumn('share-paired', [
          const UiText('share-paired-title', 'Controller paired'),
          const UiText('share-paired-subtitle',
              'The displayed one-time code has been consumed.'),
          const UiButton('share-pair-another', 'Pair Another Controller'),
        ])
      else
        UiColumn('share-pairing', [
          PairingQRCodeView(
            payload: pairingCode,
            expiresInText: expiresInText,
          ).build(),
          UiText('share-cli-command', cliCommand),
          const UiText(
            'share-cli-note',
            'The supercli CLI controls this Mac over SSH — no pairing code needed.',
          ),
        ]),
      const UiButton('share-done', 'Done'),
    ]);
  }
}

/// Remote Control settings panel (Swift: `RemoteSettingsPanel`).
final class RemoteSettingsPanel {
  const RemoteSettingsPanel({
    this.scopeName,
    this.pairedDevices = const [],
    this.pairingCode = '',
    this.sshHostName = '',
  });

  /// The selected scope display name, if any.
  final String? scopeName;
  final List<PairedDevice> pairedDevices;
  final String pairingCode;
  final String sshHostName;

  /// Description text (Swift: `remoteDescription`).
  String get description {
    if (scopeName != null) {
      return 'Let another Supercli device control $scopeName. '
          '$scopeName mints the credentials; this Mac only forwards '
          'the one-time sealed exchange.';
    }
    return 'Let another Supercli device control this Mac.';
  }

  UiNode build() {
    return UiColumn('remote-settings', [
      const UiText('remote-title', 'Remote Control'),
      UiText('remote-description', description),
      // Paired devices
      UiColumn('remote-devices', [
        const UiText('remote-devices-title', 'Paired devices'),
        if (pairedDevices.isEmpty)
          const UiText(
            'remote-devices-empty',
            'No controllers paired yet.',
          )
        else
          for (final device in pairedDevices)
            UiRow('remote-device-${device.id}', [
              UiText('remote-device-name-${device.id}', device.name),
              UiText('remote-device-paired-${device.id}', device.pairedAt),
              UiButton('remote-device-revoke-${device.id}', 'Revoke'),
            ]),
      ]),
      // Share actions
      const UiButton('remote-share-mac', 'Share This Mac'),
      const UiButton('remote-share-workspace', 'Share This Workspace'),
      // iOS app install
      UiColumn('remote-ios', [
        const UiText('remote-ios-title', 'iPhone app'),
        const UiText('remote-ios-url', iosAppInstallUrl),
      ]),
    ]);
  }
}
