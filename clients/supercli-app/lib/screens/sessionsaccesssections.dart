/// Sessions access sections: permission scopes for sessions.
///
/// Port of `SessionsAccessSections.swift` and `BrowserAccessSections.swift`.
/// These render the per-session capability grants (file access, network,
/// browser takeover) in Settings.
library;

import 'package:gpuidart/gpuidart.dart';

/// One access scope row.
final class AccessScope {
  const AccessScope({
    required this.id,
    required this.title,
    required this.detail,
    this.granted = false,
  });

  final String id;
  final String title;
  final String detail;
  final bool granted;
}

/// Sessions access sections.
final class SessionsAccessSections {
  const SessionsAccessSections({this.scopes = const []});

  final List<AccessScope> scopes;

  UiNode build() {
    return UiColumn('sessions-access', [
      const UiText('sessions-access-title', 'Session Access'),
      for (final s in scopes)
        UiRow('access-${s.id}', [
          UiText('access-title-${s.id}', s.title),
          UiText('access-detail-${s.id}', s.detail),
          UiButton('access-toggle-${s.id}', s.granted ? 'Revoke' : 'Grant'),
        ]),
    ]);
  }
}

/// Browser access sections: browser-takeover permissions.
///
/// Port of `BrowserAccessSections.swift`.
final class BrowserAccessSections {
  const BrowserAccessSections({this.scopes = const []});

  final List<AccessScope> scopes;

  UiNode build() {
    return UiColumn('browser-access', [
      const UiText('browser-access-title', 'Browser Takeover'),
      const UiText('browser-access-desc',
          'Allow the agent to take over a browser tab for web tasks.'),
      for (final s in scopes)
        UiRow('browser-access-${s.id}', [
          UiText('browser-access-title-${s.id}', s.title),
          UiButton('browser-access-toggle-${s.id}',
              s.granted ? 'Revoke' : 'Grant'),
        ]),
    ]);
  }
}
