/// Remote Control settings panel.
///
/// Port of `RemoteSettingsPanel` (SettingsView.swift, 2932-3330) with
/// `ShareThisMacSheet` (3330-3518), `ShareWorkspaceSheet` (3518-3679),
/// `LinkEnrollmentSection` (3680-3822), `TestFlightIconView` (3823-3856),
/// and `PairingQRCodeView` (3857-3908).
///
/// Pairing follows the scope (one pairing = one workspace). The iOS app
/// install link is always https://superc.li/ios (the site 302s to the
/// current TestFlight/App Store page, so shipped builds never hold a stale
/// store URL).
///
/// Renders through the RLE fallback pattern since gpuidart has no native
/// settings widgets.
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';

/// Install link for the iPhone app (Swift: `RemoteSettingsPanel.iosAppURL`).
const String iosAppInstallUrl = 'https://superc.li/ios';

/// Read-only projection of a workspace's own paired-device list
/// (`<home>/mobile/devices.json`). Display only: revocation stays in that
/// workspace's own instance, which owns the file and its cached credential
/// state. (Swift: `RemoteSettingsPanel.ScopedPairedDevice`.)
final class ScopedPairedDevice {
  const ScopedPairedDevice({
    required this.id,
    required this.name,
    this.platform,
  });

  final String id;
  final String name;
  final String? platform;
}

/// A paired controller driving this workspace, with the detail fields the
/// Controls list shows (Swift: `RemotePairedDeviceSummary`).
final class PairedDevice {
  const PairedDevice({
    required this.id,
    required this.name,
    this.platform = '',
    this.appVersion,
    this.lastSeenText,
    this.relayAllowed = true,
  });

  final String id;
  final String name;
  final String platform;
  final String? appVersion;
  final String? lastSeenText;
  final bool relayAllowed;

  /// "iOS 1.2 • last seen Aug 13, 09:41" — shared by the Controls This Mac
  /// list and the Supercli Link enrollment list so both describe a device
  /// the same way. (Swift: `deviceDetail`.)
  String get detail {
    final lastSeen =
        lastSeenText == null ? 'never seen' : 'last seen $lastSeenText';
    final version = appVersion == null ? '' : ' $appVersion';
    return '$platform$version • $lastSeen';
  }

  /// Link reach badge (Swift: `device.relayAllowed != false ? "Link" : "Direct only"`).
  String get reachBadge => relayAllowed ? 'Link' : 'Direct only';
}

/// TestFlight beta banner (Swift: `RemoteSettingsPanel.testflightSection`).
///
/// Sits directly below the inbound list because installing the phone app is
/// step zero of pairing it. (Swift: `TestFlightIconView` renders the real
/// TestFlight app icon; the Dart port shows a labelled placeholder.)
final class TestFlightBanner {
  const TestFlightBanner({this.usesWorkspaceLanguage = false});

  final bool usesWorkspaceLanguage;

  String get bodyText => usesWorkspaceLanguage
      ? 'Join the TestFlight beta to control this workspace from your phone. Open the invite link on your iPhone to install it with TestFlight.'
      : 'Join the TestFlight beta to control this Mac from your phone. Open the invite link on your iPhone to install it with TestFlight.';

  UiNode build() {
    return UiColumn('testflight-banner', [
      const UiText('testflight-icon', 'TestFlight'),
      const UiText(
          'testflight-title', 'Supercli for iPhone is in beta'),
      UiText('testflight-body', bodyText),
      const UiButton('testflight-join', 'Join the Beta'),
    ]);
  }
}

/// Pairing QR code view (Swift: `PairingQRCodeView`).
///
/// Renders the one-time pairing payload as a QR code (native CoreImage in
/// Swift; the Dart port carries the payload and renders a placeholder —
/// QR rasterisation is a native-rendering gap, not faked). The terminal
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

/// Share-this-Mac sheet state machine (Swift: `ShareThisMacSheet`).
///
/// Mints a one-time pairing code on open and shows the QR until a
/// Controller consumes it. Minting on open (rather than on every
/// Remote-panel visit) keeps a live code off the screen until the user
/// actually intends to pair; re-minting is free — it just replaces the
/// single active one-time token. Dismissing keeps the code valid until its
/// TTL so copy-then-paste-on-another-Mac flows survive closing the sheet.
enum ShareThisMacPhase {
  idle,
  pairing,
  paired,
  failed,
}

/// "Share This Mac" / "Share This Workspace" sheet
/// (Swift: `ShareThisMacSheet`, 3330-3518).
final class ShareThisMacSheet {
  const ShareThisMacSheet({
    this.usesWorkspaceLanguage = false,
    this.deviceName = '',
    this.phase = ShareThisMacPhase.idle,
    this.code = '',
    this.expiresInText = '',
    this.error,
    this.sshHostName = '',
  });

  final bool usesWorkspaceLanguage;
  final String deviceName;
  final ShareThisMacPhase phase;
  final String code;
  final String expiresInText;
  final String? error;
  final String sshHostName;

  String get title => usesWorkspaceLanguage
      ? 'Share This Workspace'
      : 'Share This Mac';

  String get subtitle {
    final target = deviceName.isNotEmpty
        ? deviceName
        : (usesWorkspaceLanguage ? 'this workspace' : 'this Mac');
    return 'Let another Supercli device control $target.';
  }

  /// The terminal counterpart of the QR code (Swift: `cliCommand`).
  String get cliCommand => 'supercli --host ssh://$sshHostName';

  UiNode build() {
    return UiColumn('share-this-mac', [
      UiText('share-title', title),
      UiText('share-subtitle', subtitle),
      if (phase == ShareThisMacPhase.failed && error != null)
        UiText('share-error', error!),
      if (phase == ShareThisMacPhase.paired)
        UiColumn('share-paired', [
          const UiText('share-paired-title', 'Controller paired'),
          const UiText('share-paired-subtitle',
              'The displayed one-time code has been consumed.'),
          const UiButton('share-pair-another', 'Pair Another Controller'),
        ])
      else
        UiColumn('share-pairing', [
          PairingQRCodeView(
            payload: code,
            expiresInText: expiresInText,
          ).build(),
          UiText('share-cli-command', cliCommand),
          const UiText(
            'share-cli-note',
            'The supercli CLI controls this Mac over SSH — no pairing code needed.',
          ),
          const UiButton('share-refresh', 'New Code'),
        ]),
      const UiButton('share-done', 'Done'),
    ]);
  }
}

/// Share-workspace sheet phase (Swift: `ShareWorkspaceSheet` states).
enum ShareWorkspacePhase {
  idle,
  pairing,
  paired,
  failed,
}

/// Share-workspace sheet (Swift: `ShareWorkspaceSheet`, 3518-3679).
///
/// Workspace-scoped pairing: mints on open, watches the paired-device list
/// for completion, and surfaces mint errors with a retry path.
final class ShareWorkspaceSheet {
  const ShareWorkspaceSheet({
    required this.workspaceName,
    this.phase = ShareWorkspacePhase.idle,
    this.code = '',
    this.expiresInText = '',
    this.error,
  });

  final String workspaceName;
  final ShareWorkspacePhase phase;
  final String code;
  final String expiresInText;
  final String? error;

  String get title => 'Share $workspaceName';

  UiNode build() {
    return UiColumn('share-workspace', [
      UiText('share-workspace-title', title),
      UiText('share-workspace-subtitle',
          'Pairing is scoped: one pairing = one workspace.'),
      if (phase == ShareWorkspacePhase.failed && error != null)
        UiColumn('share-workspace-error', [
          UiText('share-workspace-error-text', error!),
          const UiButton('share-workspace-retry', 'Try Again'),
        ])
      else if (phase == ShareWorkspacePhase.paired)
        UiColumn('share-workspace-paired', [
          const UiText('share-workspace-paired-title', 'Controller paired'),
          const UiButton('share-workspace-done', 'Done'),
        ])
      else
        UiColumn('share-workspace-pairing', [
          PairingQRCodeView(
            payload: code,
            expiresInText: expiresInText,
          ).build(),
          const UiButton('share-workspace-refresh', 'New Code'),
          const UiButton('share-workspace-cancel', 'Cancel'),
        ]),
    ]);
  }
}

/// Link enrollment section (Swift: `LinkEnrollmentSection`, 3680-3822).
///
/// Supercli Link: the enrollment list (which replaced the old global relay
/// toggle — the uplink runs whenever ≥1 inbound device is on Link). Rows
/// are Link-enrolled devices/Hosts; each row's remove action drops that
/// entry back to direct-only. Everything not listed connects direct-only,
/// on the user's own network.
final class LinkEnrollmentSection {
  const LinkEnrollmentSection({
    this.usesWorkspaceLanguage = false,
    this.enrolledDevices = const [],
    this.directOnlyDevices = const [],
    this.enrolledHosts = const [],
    this.relayStatus = '',
  });

  final bool usesWorkspaceLanguage;
  final List<PairedDevice> enrolledDevices;
  final List<PairedDevice> directOnlyDevices;
  final List<String> enrolledHosts;
  final String relayStatus;

  String get reachDescription {
    final target = usesWorkspaceLanguage ? 'this workspace' : 'this Mac';
    return 'These devices reach $target — and these Hosts stay reachable — '
        'from any network, through the superc.li relay. Session traffic is '
        'end-to-end encrypted; notification titles pass through Supercli and '
        'Apple Push. Everything not listed here connects direct-only, on your '
        'own network.';
  }

  UiNode build() {
    return UiColumn('link-enrollment', [
      const UiText('link-enrollment-title', 'Supercli Link'),
      UiText('link-enrollment-desc', reachDescription),
      if (enrolledDevices.isEmpty && enrolledHosts.isEmpty)
        const UiText('link-enrollment-empty',
            'Nothing is on Link — every connection stays direct, on your own network.')
      else
        UiColumn('link-enrollment-rows', [
          for (final d in enrolledDevices)
            UiRow('link-enrolled-${d.id}', [
              UiText('link-enrolled-name-${d.id}', d.name),
              UiText('link-enrolled-detail-${d.id}', d.detail),
              UiButton('link-enrolled-remove-${d.id}', 'Remove from Link'),
            ]),
          for (final h in enrolledHosts)
            UiRow('link-enrolled-host-$h', [
              UiText('link-enrolled-host-name-$h', h),
              UiButton('link-enrolled-host-remove-$h', 'Remove from Link'),
            ]),
          if (enrolledDevices.isNotEmpty && relayStatus.isNotEmpty)
            UiText('link-relay-status', 'Relay: $relayStatus'),
        ]),
      if (directOnlyDevices.isNotEmpty)
        UiColumn('link-add-candidates', [
          const UiText('link-add-title', 'Add to Link'),
          for (final d in directOnlyDevices)
            UiButton('link-add-${d.id}', d.name),
        ]),
    ]);
  }
}

/// Scope kinds the Remote panel renders for (Swift: `RemoteSettingsPanel` body branches).
enum RemoteScopeKind {
  /// A selected local workspace scope: scoped pairing section.
  localWorkspace,
  /// A remote Host scope (SSH or paired).
  remoteHost,
  /// This Mac (default): inbound list + enrollment + security.
  thisMac,
}

/// Remote Control settings panel (Swift: `RemoteSettingsPanel`).
final class RemoteSettingsPanel {
  const RemoteSettingsPanel({
    this.scopeKind = RemoteScopeKind.thisMac,
    this.scopeName,
    this.hasMultipleLocalWorkspaces = false,
    this.scopedWorkspaceHome = '',
    this.scopedWorkspaceName = '',
    this.scopedDevices = const [],
    this.advertisedHostName = '',
    this.remoteHostSupportsPairingInvitation = false,
    this.devices = const [],
    this.managementError,
    this.localEndpointServing = false,
    this.linkEnrollment = const LinkEnrollmentSection(),
  });

  final RemoteScopeKind scopeKind;
  final String? scopeName;
  final bool hasMultipleLocalWorkspaces;
  final String scopedWorkspaceHome;
  final String scopedWorkspaceName;
  final List<ScopedPairedDevice> scopedDevices;
  final String advertisedHostName;
  final bool remoteHostSupportsPairingInvitation;
  final List<PairedDevice> devices;
  final String? managementError;
  final bool localEndpointServing;
  final LinkEnrollmentSection linkEnrollment;

  /// Pane description (Swift: `RemoteSettingsPanel.remoteDescription`).
  String get description {
    if (scopeKind == RemoteScopeKind.localWorkspace) {
      return 'Let another Supercli device control $scopedWorkspaceName. '
          'Devices you pair here reach $scopedWorkspaceName only — each '
          'workspace pairs its own devices.';
    }
    if (scopeName != null) {
      return 'Let another Supercli device control $scopeName. '
          '$scopeName mints the credentials; this Mac only forwards '
          'the one-time sealed exchange.';
    }
    if (hasMultipleLocalWorkspaces) {
      return 'Let another Supercli device control this workspace.';
    }
    return 'Let another Supercli device control this Mac.';
  }

  String get _shareButtonLabel =>
      hasMultipleLocalWorkspaces ? 'Share This Workspace…' : 'Share This Mac…';

  UiNode _scopedWorkspaceSection() {
    return UiColumn('remote-scoped-workspace', [
      SettingsSectionHeader(
        title: 'Controls $scopedWorkspaceName',
        description: 'Each workspace pairs its own devices — a device '
            'paired here reaches only $scopedWorkspaceName. Revoke devices '
            'from $scopedWorkspaceName\'s own Remote Control settings.',
      ).build(),
      if (scopedDevices.isEmpty)
        UiText('remote-scoped-empty',
            'No devices are paired with $scopedWorkspaceName.')
      else
        UiColumn('remote-scoped-devices', [
          for (final d in scopedDevices)
            UiColumn('remote-scoped-device-${d.id}', [
              UiText('remote-scoped-device-name-${d.id}', d.name),
              if (d.platform != null && d.platform!.isNotEmpty)
                UiText('remote-scoped-device-platform-${d.id}', d.platform!),
            ]),
        ]),
      UiButton(
          'remote-pair-device', 'Pair a Device with $scopedWorkspaceName…'),
      UiText('remote-scoped-link-footnote',
          'Supercli Link enrollment for $scopedWorkspaceName\'s devices lives in '
          '$scopedWorkspaceName\'s own Remote Control settings; the license '
          'covers every workspace on this Mac and is managed from '
          '$advertisedHostName\'s scope.'),
    ]);
  }

  UiNode _remoteHostSection() {
    final name = scopeName ?? 'this Host';
    return UiColumn('remote-host', [
      SettingsSectionHeader(
        title: 'Controls $name',
        description: 'Each workspace pairs its own devices — a device '
            'paired here reaches only $name, and its entry is revocable on '
            '$name itself.',
      ).build(),
      if (remoteHostSupportsPairingInvitation)
        UiButton('remote-host-pair', 'Pair a Device with $name…')
      else
        UiText('remote-host-no-invitation',
            '$name cannot mint pairing invitations over this connection. '
            'Pair devices from its own running Supercli instead — the '
            'terminal UI\'s Settings ▸ Remote, or `supercli pair`.'),
    ]);
  }

  UiNode _controlsThisMacSection() {
    final title = hasMultipleLocalWorkspaces
        ? 'Controls This Workspace'
        : 'Controls This Mac';
    return UiColumn('remote-controls', [
      SettingsSectionHeader(
        title: title,
        description: 'Devices pair directly over your network and receive '
            'their own revocable credential. Revoking one immediately invalidates it.',
      ).build(),
      if (managementError != null)
        UiText('remote-controls-error', managementError!),
      SettingsValueRow(
        label: 'Local access',
        value: localEndpointServing ? 'Serving on this network' : 'Unavailable',
      ).build(),
      if (devices.isEmpty)
        const UiText('remote-controls-empty', 'No paired devices.')
      else
        UiColumn('remote-controls-devices', [
          for (final d in devices)
            UiRow('remote-device-${d.id}', [
              UiColumn('remote-device-info-${d.id}', [
                UiText('remote-device-name-${d.id}', d.name),
                UiText('remote-device-detail-${d.id}', d.detail),
              ]),
              UiText('remote-device-reach-${d.id}', d.reachBadge),
              UiButton('remote-device-revoke-${d.id}', 'Revoke'),
            ]),
        ]),
      UiButton('remote-share', _shareButtonLabel),
      if (hasMultipleLocalWorkspaces)
        const UiText('remote-share-footnote',
            'Each workspace on this Mac is shared separately.'),
    ]);
  }

  UiNode _securitySection() {
    return UiColumn('remote-security', [
      const SettingsSectionHeader(
        title: 'Security',
        description: 'The hook and MCP servers stay localhost-only. Remote '
            'Controllers use a separate LAN server.',
      ).build(),
      const SettingsValueRow(
        label: 'Authentication',
        value: 'Per-device Bearer <redacted>',
      ).build(),
      const SettingsValueRow(
        label: 'Pairing code',
        value: 'One-time, 5 minutes',
      ).build(),
      const SettingsValueRow(
        label: 'Stored token',
        value: 'SHA-256 hash',
      ).build(),
    ]);
  }

  UiNode build() {
    final sections = <UiNode>[
      SettingsPaneHeader(
        title: 'Remote Control',
        description: description,
      ).build(),
    ];
    switch (scopeKind) {
      case RemoteScopeKind.localWorkspace:
        sections.add(_scopedWorkspaceSection());
        sections.add(const TestFlightBanner().build());
      case RemoteScopeKind.remoteHost:
        sections.add(_remoteHostSection());
        sections.add(const TestFlightBanner().build());
      case RemoteScopeKind.thisMac:
        sections.add(_controlsThisMacSection());
        sections.add(
            TestFlightBanner(usesWorkspaceLanguage: hasMultipleLocalWorkspaces)
                .build());
        sections.add(linkEnrollment.build());
        // Link license sections live in the licensing UI; the enrollment
        // note above points at the managing scope.
        sections.add(_securitySection());
    }
    return UiColumn('remote-settings', sections);
  }
}
