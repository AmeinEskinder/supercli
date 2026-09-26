/// Host picker: workspace-adding and remote-pairing sheets.
///
/// Port of `HostPickerView.swift`. Includes:
/// - AddWorkspaceSheet: fresh local workspace, nearby/code-paired Host, SSH Host
/// - RemoteHostPairingSheet: QR/pairing code for adding iPhone/iPad
///
/// GAP (P0-11): No QR code widget in gpuidart. The pairing code is shown as
/// text; the QR must be rendered by gpuidart. Logged in requirements.
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
final class HostPickerView {
  HostPickerView({
    this.hosts = const [],
    this.pairingCode,
    this.pairingError,
    this.pairingCompleted = false,
    this.selectedHostName = '',
  });

  final List<HostEntry> hosts;
  final String? pairingCode;
  final String? pairingError;
  final bool pairingCompleted;
  final String selectedHostName;

  UiNode build() {
    return UiColumn('host-picker', [
      const UiText('host-picker-title', 'Add Workspace'),
      UiRow('host-picker-actions', [
        const UiButton('add-local', 'New Local Workspace'),
        const UiButton('add-ssh', 'Add SSH Host'),
      ]),
      const UiText('nearby-hosts-title', 'Nearby Hosts'),
      UiTable('nearby-hosts', dataset: 'nearby-hosts'),
      if (pairingCode != null) _pairingSheet(),
    ]);
  }

  UiNode _pairingSheet() {
    return UiColumn('pairing-sheet', [
      UiText('pairing-title', 'Add an iPhone or iPad to $selectedHostName'),
      const UiText('pairing-desc',
          'This Mac forwards a one-time sealed exchange to the remote workspace.'),
      if (pairingError != null) UiText('pairing-error', pairingError!),
      if (pairingCompleted)
        const UiText('pairing-done', '✓ Device added')
      else ...[
        // GAP P0-11: QR code widget missing. Showing code as text.
        UiText('pairing-code', 'Pairing code: $pairingCode'),
        const UiText('pairing-qr-gap',
            '(QR code renders here when gpuidart ships UiQrCode)'),
      ],
      const UiButton('pairing-done-btn', 'Done'),
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
