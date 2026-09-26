/// Clickable path: file path rendered as tappable segments, plus OSC
/// hyperlink and cwd tracking.
///
/// Port of `ClickablePath.swift`.
/// Rows 179, 180, 181 — [DESKTOP] parity.
/// GAP: no native-window screenshot proof yet (screenshot proof pending).
/// GAP (row 181): no native drag-out primitive in gpuidart; the drop target
/// accepts file drops via the Host file-drop event.
library;

import 'package:gpuidart/gpuidart.dart';

/// Parsed OSC 8 hyperlink: `OSC 8 ; params ; URI ST text OSC 8 ;; ST`.
final class OscHyperlink {
  const OscHyperlink({required this.uri, required this.text});

  final String uri;
  final String text;
}

/// Parses OSC 8 hyperlinks and OSC 7 cwd notifications out of terminal output.
///
/// OSC 8: `\x1b]8;;<uri>\x1b\\<text>\x1b]8;;\x1b\\`
/// OSC 7: `\x1b]7;file://<host><path>\x1b\\` (cwd tracking)
final class OscSequenceParser {
  static final _osc8 = RegExp(
      '\x1b\\]8;;([^\x1b\\\\]*)\x1b\\\\([^\x1b]*)\x1b\\]8;;\x1b\\\\');
  static final _osc7 =
      RegExp('\x1b\\]7;file://[^/\x1b]*(/[^\x1b\\\\]*)\x1b\\\\');

  /// All hyperlinks in [output], in order.
  static List<OscHyperlink> hyperlinks(String output) => _osc8
      .allMatches(output)
      .map((m) => OscHyperlink(uri: m.group(1) ?? '', text: m.group(2) ?? ''))
      .toList();

  /// The last cwd reported via OSC 7 in [output], or null.
  static String? cwd(String output) {
    String? last;
    for (final m in _osc7.allMatches(output)) {
      last = m.group(1);
    }
    return last;
  }
}

/// A bare file path with optional line/column suffix: `path:line:col`.
final class FileLocation {
  const FileLocation({required this.path, this.line, this.column});

  final String path;
  final int? line;
  final int? column;

  static final _pattern = RegExp(r'^(.+?):(\d+)(?::(\d+))?$');

  /// Parses `path`, `path:line`, or `path:line:col`. Returns null when the
  /// text is not a bare file path.
  static FileLocation? parse(String text) {
    final trimmed = text.trim();
    if (trimmed.isEmpty) return null;
    final m = _pattern.firstMatch(trimmed);
    if (m == null) {
      // Plain path without line info — must look like a path.
      if (!trimmed.contains('/') && !trimmed.contains('.')) return null;
      return FileLocation(path: trimmed);
    }
    return FileLocation(
      path: m.group(1)!,
      line: int.tryParse(m.group(2)!),
      column: m.group(3) == null ? null : int.tryParse(m.group(3)!),
    );
  }
}

final class ClickablePath {
  const ClickablePath({required this.path});

  final String path;

  UiNode build() {
    final segments = path.split('/').where((s) => s.isNotEmpty).toList();
    return UiRow('clickable-path', [
      for (var i = 0; i < segments.length; i++) ...[
        if (i > 0) const UiText('path-sep', '/'),
        UiButton('path-seg-$i', segments[i]),
      ],
    ]);
  }
}

/// Row 180: ⌘-click a bare file path to open it in the App or editor.
///
/// The parsed [FileLocation] carries line/column so the editor can jump
/// straight to the position. The open target (App vs external editor) comes
/// from the user's default-editor preference.
final class CmdClickPathOpener {
  const CmdClickPathOpener({required this.location, this.openInApp = true});

  final FileLocation location;
  final bool openInApp;

  UiNode build() {
    final pos = location.line == null
        ? ''
        : ':${location.line}${location.column == null ? '' : ':${location.column}'}';
    return UiRow('cmdclick-path', [
      UiText('cmdclick-label', '${location.path}$pos'),
      UiButton('cmdclick-open', openInApp ? 'Open in App' : 'Open in Editor'),
    ]);
  }
}

/// Row 181: File drop target for hosted Apps and terminals.
///
/// Native drag-out of files has no gpuidart primitive (GAP); this component
/// is the drop side, fed by Host file-drop events carrying file URLs.
final class FileDropTarget {
  const FileDropTarget({required this.targetId, this.hovering = false});

  final String targetId;
  final bool hovering;

  UiNode build() {
    return UiRow('file-drop-$targetId', [
      UiText('file-drop-hint',
          hovering ? 'Drop files to attach' : 'Drop files here'),
    ]);
  }
}
