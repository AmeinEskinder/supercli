/// License settings: key entry, activation, seats.
///
/// Port of `LicenseSettingsPanel.swift`. Covers checklist item 209:
/// "Unpeel Link license section (activate, seats, release seat, get Link)".
library;

import 'package:gpuidart/gpuidart.dart';

/// One licensed seat/device.
final class LicenseSeat {
  const LicenseSeat({
    required this.id,
    required this.deviceName,
    this.lastSeen = '',
  });

  final String id;
  final String deviceName;
  final String lastSeen;
}

/// License settings panel.
final class LicenseSettingsPanel {
  const LicenseSettingsPanel({
    this.licenseKey = '',
    this.status = 'No license',
    this.activated = false,
    this.seats = const [],
  });

  final String licenseKey;
  final String status;
  final bool activated;
  final List<LicenseSeat> seats;

  UiNode build() {
    return UiColumn('license-settings', [
      const UiText('license-title', 'License'),
      UiText('license-status', 'Status: $status'),
      if (!activated) ...[
        const UiInput('license-key', placeholder: 'License key…'),
        const UiButton('license-activate', 'Activate'),
        const UiButton('license-get-link', 'Get Unpeel Link'),
      ] else
        UiRow('license-active-row', [
          UiText('license-key-masked', 'Key: ••••-${_last4(licenseKey)}'),
          const UiButton('license-deactivate', 'Deactivate'),
        ]),
      const UiText('license-seats-title', 'Seats'),
      UiTable('license-seats', dataset: 'license-seats'),
      const UiButton('license-release-seat', 'Release seat'),
    ]);
  }

  String _last4(String key) =>
      key.length >= 4 ? key.substring(key.length - 4) : key;

  TableDataset dataset() => TableDataset(
        'license-seats',
        columns: const ['Device', 'Last seen'],
        rows: seats.map((s) => [s.deviceName, s.lastSeen]).toList(),
      );
}
