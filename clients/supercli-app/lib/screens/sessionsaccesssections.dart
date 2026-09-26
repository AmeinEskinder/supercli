/// Sessions access sections: permission scopes for sessions.
///
/// Port of `SessionsAccessSections.swift` and `BrowserAccessSections.swift`.
/// These render the per-session capability grants (file access, network,
/// browser takeover) in Settings. Covers checklist items 204 and 205:
///
/// 204: "Settings ▸ Agent access ▸ Sessions (write policy, worktree
///      permission, auto-gallery, approved pairs/Apps with Revoke)"
/// 205: "Settings ▸ Agent access ▸ Browser (engine status, access mode,
///      approvals, window/cursor/scope/app path, site rules, clear data)"
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingspanels.dart';

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

/// One approved pair/app grant that can be revoked.
final class ApprovedPair {
  const ApprovedPair({
    required this.id,
    required this.name,
    this.detail = '',
  });

  final String id;
  final String name;
  final String detail;
}

/// Agent access settings: which agents can access what.
///
/// Combines the sessions access sections with the approved-pairs list.
final class AgentAccessSettingsPanel {
  AgentAccessSettingsPanel({
    required this.settings,
    this.scopes = const [],
    this.approvedPairs = const [],
  });

  final AppSettings settings;
  final List<AccessScope> scopes;
  final List<ApprovedPair> approvedPairs;

  UiNode build() {
    return UiColumn('agent-access', [
      const UiText('agent-access-title', 'Agent Access'),
      const UiText('agent-access-sessions-title', 'Sessions'),
      SettingsSelect(
        id: 'agent-write-policy',
        label: 'Write policy',
        options: WritePolicy.values.map((w) => w.label).toList(),
        selected: settings.writePolicy.label,
      ).fallback(),
      SettingsToggle(
        id: 'agent-worktree-permission',
        label: 'Worktree permission',
        value: settings.worktreeAccess,
      ).fallback(),
      SettingsToggle(
        id: 'agent-auto-gallery',
        label: 'Auto-add to gallery',
        value: settings.autoGallery,
      ).fallback(),
      const UiText('agent-access-scopes-title', 'Capability grants'),
      for (final s in scopes)
        UiRow('access-${s.id}', [
          UiText('access-title-${s.id}', s.title),
          UiText('access-detail-${s.id}', s.detail),
          UiButton('access-toggle-${s.id}', s.granted ? 'Revoke' : 'Grant'),
        ]),
      const UiText('agent-access-pairs-title', 'Approved pairs & Apps'),
      if (approvedPairs.isEmpty)
        const UiText('agent-access-pairs-empty', 'No approved pairs.')
      else
        for (final p in approvedPairs)
          UiRow('pair-${p.id}', [
            UiText('pair-name-${p.id}', p.name),
            UiText('pair-detail-${p.id}', p.detail),
            UiButton('pair-revoke-${p.id}', 'Revoke'),
          ]),
    ]);
  }
}

/// Sessions access sections (legacy name kept for the re-export).
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

/// One browser site rule.
final class BrowserSiteRule {
  const BrowserSiteRule({
    required this.id,
    required this.pattern,
    this.allowed = true,
  });

  final String id;
  final String pattern;
  final bool allowed;
}

/// Browser access sections: browser-takeover permissions.
///
/// Port of `BrowserAccessSections.swift`.
final class BrowserAccessSections {
  BrowserAccessSections({
    required this.settings,
    this.engineStatus = 'Not installed',
    this.accessMode = 'Ask',
    this.scopes = const [],
    this.siteRules = const [],
    this.appPath = '',
  });

  final AppSettings settings;
  final String engineStatus;
  final String accessMode;
  final List<AccessScope> scopes;
  final List<BrowserSiteRule> siteRules;
  final String appPath;

  UiNode build() {
    return UiColumn('browser-access', [
      const UiText('browser-access-title', 'Browser Takeover'),
      const UiText('browser-access-desc',
          'Allow the agent to take over a browser tab for web tasks.'),
      UiRow('browser-engine-row', [
        UiText('browser-engine-status', 'Engine: $engineStatus'),
        const UiButton('browser-engine-install', 'Install engine'),
      ]),
      SettingsSelect(
        id: 'browser-access-mode',
        label: 'Access mode',
        options: BrowserDefaultAccess.values.map((b) => b.label).toList(),
        selected: settings.browserDefaultAccess.label,
      ).fallback(),
      SettingsToggle(
        id: 'browser-mcp',
        label: 'Browser MCP (experimental)',
        value: settings.browserMcp,
      ).fallback(),
      SettingsToggle(
        id: 'browser-auto-screenshots',
        label: 'Auto-add screenshots to gallery',
        value: settings.autoAddBrowserScreenshots,
      ).fallback(),
      for (final s in scopes)
        UiRow('browser-access-${s.id}', [
          UiText('browser-access-title-${s.id}', s.title),
          UiButton('browser-access-toggle-${s.id}',
              s.granted ? 'Revoke' : 'Grant'),
        ]),
      UiRow('browser-app-path-row', [
        UiText('browser-app-path',
            'App path: ${appPath.isEmpty ? '(default)' : appPath}'),
        const UiButton('browser-app-path-choose', 'Choose…'),
      ]),
      const UiText('browser-site-rules-title', 'Site rules'),
      if (siteRules.isEmpty)
        const UiText('browser-site-rules-empty', 'No site rules.')
      else
        for (final r in siteRules)
          UiRow('site-rule-${r.id}', [
            UiText('site-rule-pattern-${r.id}', r.pattern),
            UiButton(
                'site-rule-toggle-${r.id}', r.allowed ? 'Block' : 'Allow'),
            UiButton('site-rule-delete-${r.id}', 'Delete'),
          ]),
      const UiButton('browser-site-rule-add', 'Add site rule…'),
      const UiButton('browser-clear-data', 'Clear browsing data…'),
    ]);
  }
}
