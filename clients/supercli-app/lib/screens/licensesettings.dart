/// License settings: key entry, activation, seats.
///
/// Port of `LicenseSettingsPanel.swift`. Covers checklist item 209:
/// "Supercli Link license section (activate, seats, release seat, get Link)".
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

  /// License key prefix. Rebranded from the legacy `CLRTY-` key format;
  /// `CLRTY-` keys are not accepted.
  static const String keyPrefix = 'SCLI-';

  /// True if the key has the expected `SCLI-` prefix format.
  static bool isValidKeyFormat(String key) {
    final normalized = key.trim();
    return normalized.startsWith(keyPrefix) && normalized.contains('.');
  }

  /// True if the key uses the legacy `CLRTY-` key format.
  /// These are rejected with a clear message.
  static bool isLegacyKey(String key) {
    return key.trim().startsWith('CLRTY-');
  }

  UiNode build() {
    return UiColumn('license-settings', [
      const UiText('license-title', 'License'),
      UiText('license-status', 'Status: $status'),
      if (!activated) ...[
        const UiInput('license-key', placeholder: 'SCLI-…'),
        const UiButton('license-activate', 'Activate'),
        const UiButton('license-get-link', 'Get Supercli Link'),
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
