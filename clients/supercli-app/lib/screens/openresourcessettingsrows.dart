/// Open-resources settings rows.
///
/// Port of `OpenResourcesSettingsRows` (SettingsView.swift, 2326-2494):
/// per-selector opener pickers for a remote Host's app registry — which
/// app opens a file type or resource kind. The resolution chain
/// (explicit override → Host-saved opener → registry default → single
/// handler → editor/system fallback) and the missing/outdated-app
/// install/update actions are ported as pure functions so they are
/// testable; the rows render them.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingspanels.dart' as base;

/// A Host app-registry entry (Swift: `RemoteAppSummary`).
final class RemoteAppSummary {
  const RemoteAppSummary({
    required this.id,
    required this.name,
    this.mediaTypes = const [],
    this.resourceKinds = const [],
    this.defaultFor = const [],
    this.installed = false,
    this.updateAvailable = false,
    this.version,
    this.installedVersion,
  });

  final String id;
  final String name;
  final List<String> mediaTypes;
  final List<String> resourceKinds;
  final List<String> defaultFor;
  final bool installed;
  final bool updateAvailable;
  final String? version;
  final String? installedVersion;

  /// Whether this app handles a `file:<media-type>` / `resource:<kind>`
  /// selector (Swift: `RemoteAppSummary.handles(selector:)`).
  bool handles(String selector) {
    if (selector.startsWith('file:')) {
      final mediaType = selector.substring(5).toLowerCase();
      return mediaTypes.any((m) => m.toLowerCase() == mediaType);
    }
    if (selector.startsWith('resource:')) {
      final kind = selector.substring(9).toLowerCase();
      return resourceKinds.any((k) => k.toLowerCase() == kind);
    }
    return false;
  }
}

/// Display title for a selector (Swift: `selectorTitle`).
String openResourceSelectorTitle(String selector) {
  switch (selector) {
    case 'file:text/markdown':
      return 'Markdown';
    case 'file:text/csv':
      return 'CSV';
    case 'resource:folder':
      return 'Folders';
    case 'resource:git.working-tree':
      return 'Git changes';
    case 'resource:github.repository':
      return 'GitHub repositories';
    default:
      return selector
          .replaceAll('file:', '')
          .replaceAll('resource:', '');
  }
}

/// Sorted selector list derived from the available apps
/// (Swift: `OpenResourcesSettingsRows.selectors`).
List<String> openResourceSelectors(List<RemoteAppSummary> apps) {
  final fileSelectors =
      apps.expand((a) => a.mediaTypes).map((m) => 'file:$m');
  final resourceSelectors =
      apps.expand((a) => a.resourceKinds).map((k) => 'resource:$k');
  final selectors = {...fileSelectors, ...resourceSelectors}.toList();
  selectors.sort((a, b) =>
      openResourceSelectorTitle(a).compareTo(openResourceSelectorTitle(b)));
  return selectors;
}

/// Resolve the selected opener for a selector.
///
/// Resolution chain (Swift: `selection(for:)` getter): explicit override →
/// Host-saved opener → registry default (`defaultFor`) → the single
/// handling app → `editor` for file selectors, empty otherwise.
String resolveOpener({
  required String selector,
  required List<RemoteAppSummary> apps,
  String? savedOpener,
  String? override,
}) {
  if (override != null) return override;
  if (savedOpener != null) return savedOpener;
  for (final app in apps) {
    if (app.handles(selector) && app.defaultFor.contains(selector)) {
      return 'app:${app.id}';
    }
  }
  final matches = apps.where((a) => a.handles(selector)).toList();
  if (matches.length == 1) return 'app:${matches.first.id}';
  return selector.startsWith('file:') ? 'editor' : '';
}

/// The selected app when it is not installed on the Host
/// (Swift: `selectedMissingApp(for:)`).
RemoteAppSummary? selectedMissingApp({
  required String selector,
  required List<RemoteAppSummary> apps,
  required Set<String> installedIDs,
  String? savedOpener,
  String? override,
}) {
  final opener = resolveOpener(
      selector: selector, apps: apps, savedOpener: savedOpener, override: override);
  if (!opener.startsWith('app:')) return null;
  final appID = opener.substring(4);
  if (installedIDs.contains(appID)) return null;
  for (final app in apps) {
    if (app.id == appID) return app;
  }
  return null;
}

/// The selected app when it is installed but the Host's registry publishes
/// a different version (Swift: `selectedOutdatedApp(for:)`).
RemoteAppSummary? selectedOutdatedApp({
  required String selector,
  required List<RemoteAppSummary> apps,
  String? savedOpener,
  String? override,
}) {
  final opener = resolveOpener(
      selector: selector, apps: apps, savedOpener: savedOpener, override: override);
  if (!opener.startsWith('app:')) return null;
  final appID = opener.substring(4);
  for (final app in apps) {
    if (app.id == appID && app.installed && app.updateAvailable) return app;
  }
  return null;
}

/// Label for an opener value in the picker.
String openerLabel(String opener, List<RemoteAppSummary> apps) {
  if (opener == 'editor') return 'Default Editor';
  if (opener == 'system') return 'System Default';
  if (opener.startsWith('app:')) {
    final appID = opener.substring(4);
    for (final app in apps) {
      if (app.id == appID) return app.name;
    }
    return appID;
  }
  return opener;
}

/// Open-resources settings rows (Swift: `OpenResourcesSettingsRows`).
///
/// One labeled row per selector: the opener picker plus an Install /
/// "Update to <version>" button when the selected app is missing or
/// outdated. Running panes keep the old binary until Restart App.
final class OpenResourcesSettingsRows {
  const OpenResourcesSettingsRows({
    this.apps = const [],
    this.installedIDs = const {},
    this.savedOpeners = const {},
    this.overrides = const {},
    this.installingIDs = const {},
    this.errorMessage,
  });

  final List<RemoteAppSummary> apps;
  final Set<String> installedIDs;
  final Map<String, String> savedOpeners;
  final Map<String, String> overrides;
  final Set<String> installingIDs;
  final String? errorMessage;

  UiNode build() {
    final rows = <UiNode>[];
    for (final selector in openResourceSelectors(apps)) {
      final selection = resolveOpener(
        selector: selector,
        apps: apps,
        savedOpener: savedOpeners[selector],
        override: overrides[selector],
      );
      final missing = selectedMissingApp(
        selector: selector,
        apps: apps,
        installedIDs: installedIDs,
        savedOpener: savedOpeners[selector],
        override: overrides[selector],
      );
      final outdated = missing == null
          ? selectedOutdatedApp(
              selector: selector,
              apps: apps,
              savedOpener: savedOpeners[selector],
              override: overrides[selector],
            )
          : null;
      final handling =
          apps.where((a) => a.handles(selector)).toList();
      final options = <String>[];
      for (final app in handling) {
        final suffix = installedIDs.contains(app.id) ? '' : ' (Not installed)';
        options.add('app:${app.id}|${app.name}$suffix');
      }
      if (selector.startsWith('file:')) {
        options.add('editor|Default Editor');
        options.add('system|System Default');
      }
      final rowChildren = <UiNode>[
        base.SettingsSelect(
          id: 'opener-$selector',
          label: openResourceSelectorTitle(selector),
          selected: openerLabel(selection, apps),
          options: options.map((o) => o.split('|')[1]).toList(),
        ).fallback(),
      ];
      final target = missing ?? outdated;
      if (target != null) {
        final installing = installingIDs.contains(target.id);
        final label = missing != null
            ? (installing ? 'Installing…' : 'Install')
            : (installing
                ? 'Updating…'
                : (target.version == null
                    ? 'Update'
                    : 'Update to ${target.version}'));
        rowChildren.add(UiButton('opener-install-${target.id}', label));
      }
      rows.add(UiColumn('opener-row-$selector', rowChildren));
    }
    if (errorMessage != null) {
      rows.add(UiText('opener-error', errorMessage!));
    }
    return UiColumn('open-resources-rows', rows);
  }
}
