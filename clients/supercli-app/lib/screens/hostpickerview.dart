/// Host picker: workspace-adding and remote-pairing sheets.
///
/// Port of `HostPickerView.swift`. Includes:
/// - AddWorkspaceSheet: fresh local workspace, nearby/code-paired Host, SSH Host
/// - RemoteHostPairingSheet: QR/pairing code for adding iPhone/iPad
///
/// GAP (P0-11): No QR code widget in gpuidart. The pairing code is shown as
/// text; the QR must be rendered by gpuidart. Logged in requirements.
///
/// Shell contract (mirrors Swift's store-driven lifecycle):
/// - When the sheet appears with `pairingCode == null && !pairingCompleted`,
///   the shell must call `beginRemoteHostPairing` (Swift `onAppear`).
/// - When the sheet disappears, the shell must call `cancelRemoteHostPairing`
///   (Swift `onDisappear`).
/// - The shell must re-render at least once per second while the sheet is
///   visible, passing an updated `nowUnixMs`; when `nowUnixMs` passes
///   `pairingExpiresAtUnixMs` the shell must call `beginRemoteHostPairing`
///   again (Swift's 1s `Timer` auto-refresh on expiry).
/// - Button IDs dispatched by the shell:
///   - `pairing-generate` / `pairing-refresh` → `beginRemoteHostPairing`
///   - `pairing-copy-code` → copy `pairingCode` to the platform clipboard
///     (no clipboard API in gpuidart; shell wires it, same as gitpaneview)
///   - `pairing-add-another` → `beginRemoteHostPairing` (fresh invitation)
///   - `pairing-done` → dismiss the sheet
library;

import 'package:gpuidart/gpuidart.dart';

/// A discovered or configured host.
final class HostEntry {
  const HostEntry({
    required this.id,
    required this.name,
    this.address = '',
    this.paired = false,
  });

  final String id;
  final String name;
  final String address;
  final bool paired;
}

/// The host picker / pairing sheet.
///
/// All pairing state is injected (the view is pure); the app shell owns the
/// pairing lifecycle per the contract above.
final class HostPickerView {
  HostPickerView({
    this.hosts = const [],
    this.pairingCode,
    this.pairingExpiresAtUnixMs,
    this.nowUnixMs,
    this.pairingError,
    this.pairingCompleted = false,
    this.selectedHostName = '',
  });

  final List<HostEntry> hosts;

  /// The current one-time pairing code, if an invitation exists.
  final String? pairingCode;

  /// Unix milliseconds when the current invitation expires (Swift
  /// `expiresAtUnixMs`). Null while the invitation is being created.
  final int? pairingExpiresAtUnixMs;

  /// Unix milliseconds "now" (injected by the shell's 1s re-render tick).
  /// Used only for the countdown text.
  final int? nowUnixMs;

  final String? pairingError;
  final bool pairingCompleted;
  final String selectedHostName;

  /// Swift `expiresInText`: "Expires in M:SS", clamped at zero. Empty when
  /// there is no expiry.
  static String expiresInText(int? expiresAtUnixMs, int? nowUnixMs) {
    if (expiresAtUnixMs == null || nowUnixMs == null) return '';
    final remainingSeconds =
        ((expiresAtUnixMs - nowUnixMs) / 1000).floor().clamp(0, 1 << 62);
    final minutes = remainingSeconds ~/ 60;
    final seconds = remainingSeconds % 60;
    return 'Expires in $minutes:${seconds.toString().padLeft(2, '0')}';
  }

  UiNode build() {
    return UiColumn('host-picker', [
      const UiText('host-picker-title', 'Add Workspace'),
      UiRow('host-picker-actions', [
        const UiButton('add-local', 'New Local Workspace'),
        const UiButton('add-ssh', 'Add SSH Host'),
      ]),
      const UiText('nearby-hosts-title', 'Nearby Hosts'),
      UiTable('nearby-hosts', dataset: 'nearby-hosts'),
      if (pairingCode != null || pairingCompleted) _pairingSheet(),
    ]);
  }

  UiNode _pairingSheet() {
    return UiColumn('pairing-sheet', [
      UiText('pairing-title', 'Add an iPhone or iPad to $selectedHostName'),
      const UiText('pairing-desc',
          'This Mac forwards a one-time sealed exchange to the remote workspace.'),
      if (pairingError != null) UiText('pairing-error', pairingError!),
      if (pairingCompleted) _completedState() else _pendingState(),
      UiText('pairing-footer',
          'After pairing, the phone connects to $selectedHostName itself — Direct when reachable, otherwise through Supercli Link if enabled.'),
      const UiButton('pairing-done', 'Done'),
    ]);
  }

  /// Swift: completed branch — green checkmark, "Device added", Add Another.
  UiNode _completedState() {
    return UiColumn('pairing-completed', [
      const UiText('pairing-done-title', '✓ Device added'),
      UiText('pairing-done-desc',
          'The phone now has its own revocable Direct and Link credentials for $selectedHostName.'),
      const UiButton('pairing-add-another', 'Add Another iPhone or iPad'),
    ]);
  }

  /// Swift: pending branch — QR/code, countdown, generate/refresh, copy.
  UiNode _pendingState() {
    final countdown = expiresInText(pairingExpiresAtUnixMs, nowUnixMs);
    return UiColumn('pairing-pending', [
      // GAP P0-11: QR code widget missing. Showing code as text.
      if (pairingCode != null) ...[
        UiText('pairing-code', 'Pairing code: $pairingCode'),
        const UiText('pairing-qr-gap',
            '(QR code renders here when gpuidart ships UiQrCode)'),
        const UiText('pairing-scan-hint',
            'Scan this code in Supercli on the phone.'),
      ] else if (pairingError == null) ...[
        const UiText('pairing-creating', 'Creating invitation…'),
      ],
      if (countdown.isNotEmpty) UiText('pairing-countdown', countdown),
      UiRow('pairing-actions', [
        UiButton(
          pairingCode == null ? 'pairing-generate' : 'pairing-refresh',
          pairingCode == null ? 'Generate QR Code' : 'Refresh QR Code',
        ),
        if (pairingCode != null)
          const UiButton('pairing-copy-code', 'Copy Pairing Code'),
      ]),
    ]);
  }

  TableDataset nearbyDataset() {
    return TableDataset(
      'nearby-hosts',
      columns: const ['Host', 'Address', 'Status'],
      rows: hosts
          .map((h) => [h.name, h.address, h.paired ? 'Paired' : ''])
          .toList(),
    );
  }
}
