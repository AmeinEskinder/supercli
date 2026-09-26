/// Remote folder picker for launching sessions on remote Hosts.
///
/// Port of `RemoteFolderPicker.swift`. Covers checklist row 199:
/// "Remote folder picker for launching on remote Hosts".
///
/// Stateful browser over the remote Host's filesystem: breadcrumb
/// navigation, parent-directory traversal, directory-only listing, and
/// an explicit selection that the "Choose" action commits. Directory
/// contents come from the Host (`HostClient`); this is the navigation
/// + selection model with a testable pure core.
library;

import 'package:gpuidart/gpuidart.dart';

/// One entry in a remote directory listing.
final class RemoteFolderEntry {
  const RemoteFolderEntry({
    required this.name,
    required this.path,
    this.isDirectory = true,
  });

  final String name;
  final String path;
  final bool isDirectory;
}

/// Navigation + selection state for the remote folder picker.
///
/// Pure logic: the app shell feeds directory listings in via
/// [setEntries] (from `HostClient` file browse) and reads
/// [selectedPath] when the user commits.
final class RemoteFolderBrowser {
  RemoteFolderBrowser({String initialPath = '/'}) : _currentPath = initialPath;

  String _currentPath;
  String get currentPath => _currentPath;

  List<RemoteFolderEntry> _entries = const [];
  List<RemoteFolderEntry> get entries => _entries;

  String? _selectedPath;
  String? get selectedPath => _selectedPath;

  /// Feed a fresh directory listing (directories only).
  void setEntries(List<RemoteFolderEntry> entries) {
    _entries = entries.where((e) => e.isDirectory).toList()
      ..sort((a, b) => a.name.compareTo(b.name));
    // Drop the selection if it is no longer visible.
    if (_selectedPath != null &&
        !_entries.any((e) => e.path == _selectedPath)) {
      _selectedPath = null;
    }
  }

  /// Navigate into a subdirectory.
  void enter(String path) {
    _currentPath = path;
    _entries = const [];
    _selectedPath = null;
  }

  /// Navigate to the parent directory. Returns false at the root.
  bool goUp() {
    if (_currentPath == '/' || _currentPath.isEmpty) return false;
    final trimmed = _currentPath.endsWith('/')
        ? _currentPath.substring(0, _currentPath.length - 1)
        : _currentPath;
    final idx = trimmed.lastIndexOf('/');
    _currentPath = idx <= 0 ? '/' : trimmed.substring(0, idx);
    _entries = const [];
    _selectedPath = null;
    return true;
  }

  /// Breadcrumb segments for the current path, e.g. `/a/b` →
  /// `[('/', '/'), ('/a', 'a'), ('/a/b', 'b')]`.
  List<(String, String)> get breadcrumbs {
    if (_currentPath == '/' || _currentPath.isEmpty) {
      return [('/', '/')];
    }
    final parts = _currentPath.split('/').where((p) => p.isNotEmpty).toList();
    final crumbs = <(String, String)>[('/', '/')];
    var acc = '';
    for (final part in parts) {
      acc += '/$part';
      crumbs.add((acc, part));
    }
    return crumbs;
  }

  void select(String path) {
    if (_entries.any((e) => e.path == path)) {
      _selectedPath = path;
    }
  }

  void clearSelection() => _selectedPath = null;
}

/// Remote folder picker sheet.
final class RemoteFolderPicker {
  const RemoteFolderPicker({
    this.hostName = '',
    required this.browser,
  });

  final String hostName;
  final RemoteFolderBrowser browser;

  UiNode build() {
    final crumbs = browser.breadcrumbs;
    return UiColumn('remote-folder-picker', [
      UiText('rfp-title', 'Choose Folder on $hostName'),
      UiRow('rfp-crumbs', [
        for (final crumb in crumbs) UiButton('rfp-crumb-${crumb.$1}', crumb.$2),
        if (crumbs.length > 1) const UiButton('rfp-up', '↑'),
      ]),
      UiText('rfp-path', browser.currentPath),
      UiTable('rfp-entries', dataset: 'rfp-entries'),
      UiRow('rfp-actions', [
        const UiButton('rfp-choose', 'Choose'),
        const UiButton('rfp-cancel', 'Cancel'),
      ]),
    ]);
  }

  TableDataset dataset() => TableDataset(
        'rfp-entries',
        columns: const ['Name'],
        rows: browser.entries
            .map((e) => [e.name + (browser.selectedPath == e.path ? ' ✓' : '')])
            .toList(),
      );
}
