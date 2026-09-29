/// Tool icons for the Supercli app.
/// 
/// Port of `ToolIcons.swift` from SupercliShared.
/// A resolved, client-renderable runtime icon.
library;

import 'native_client.dart';
import 'plugin_settings_list.dart';

/// Runtime kind for icon selection. The authoritative kind lives in the Rust
/// catalog; this mirrors it for UI switch statements.
enum SupercliRuntimeKind {
  agent,
  app,
  editor,
  terminal;

  static SupercliRuntimeKind fromString(String? value) {
    switch (value) {
      case 'agent':
        return SupercliRuntimeKind.agent;
      case 'app':
        return SupercliRuntimeKind.app;
      case 'editor':
        return SupercliRuntimeKind.editor;
      default:
        return SupercliRuntimeKind.terminal;
    }
  }
}

/// A resolved, client-renderable runtime icon. Provider artwork is generated
/// from `runtimes/<slug>/assets/icon.svg`; this type owns only generic agent
/// and terminal fallbacks so adding a runtime never requires a Dart case.
final class SupercliToolIcon {
  const SupercliToolIcon({
    required this.id,
    required this.key,
    required this.label,
    required this.kind,
    required this.svgSource,
    required this.isTemplate,
    required this.fallbackSystemName,
    required this.usesRuntimeAsset,
  });

  final String id;
  final String key;
  final String label;
  final SupercliRuntimeKind kind;
  final String svgSource;
  final bool isTemplate;
  final String fallbackSystemName;
  final bool usesRuntimeAsset;

  /// All icons: one per runtime (from the Rust catalog via FFI) plus the
  /// terminal fallback.
  static List<SupercliToolIcon> get allCases => [
    ...SupercliNative.runtimeCatalog().map(forRuntime),
    terminal,
  ];

  /// Runtime art first, then an installed Supercli App (by Host-stamped App
  /// id, else by the command's leading binary), else the terminal mark.
  static SupercliToolIcon resolving({
    String? appID,
    String? providerID,
    required String command,
  }) {
    Map<String, dynamic>? runtime;
    if (providerID != null) {
      runtime = SupercliNative.runtimeById(providerID);
    }
    runtime ??= () {
      final slug = SupercliNative.runtimeDetectTool(command);
      return slug == null ? null : SupercliNative.runtimeById(slug);
    }();
    if (runtime != null) {
      return forRuntime(runtime);
    }
    return SupercliAppIconCatalog.icon(appID: appID) ??
        SupercliAppIconCatalog.icon(command: command) ??
        terminal;
  }

  /// An installed Supercli App's mark from the Host's catalog: the
  /// registry-authored SVG when it ships one, else the generic App mark.
  static SupercliToolIcon forApp({
    required String id,
    required String name,
    String? iconSVG,
  }) {
    final authored = iconSVG?.trim();
    final hasAuthored = authored?.isNotEmpty == true;
    return SupercliToolIcon(
      id: 'app:$id',
      key: id,
      label: name,
      kind: SupercliRuntimeKind.app,
      svgSource: hasAuthored ? authored! : _genericSvg(SupercliRuntimeKind.app),
      isTemplate: true,
      fallbackSystemName: _fallbackSystemName(SupercliRuntimeKind.app),
      usesRuntimeAsset: hasAuthored,
    );
  }

  static SupercliToolIcon forRuntime(Map<String, dynamic> runtime) {
    final authoredSVG = (runtime['icon'] as String?)?.trim();
    final hasAuthoredSVG = authoredSVG?.isNotEmpty == true;
    final kind = SupercliRuntimeKind.fromString(runtime['kind'] as String?);
    return SupercliToolIcon(
      id: runtime['id'] as String? ?? '',
      key: runtime['slug'] as String? ?? '',
      label: runtime['label'] as String? ?? '',
      kind: kind,
      svgSource: hasAuthoredSVG ? authoredSVG! : _genericSvg(kind),
      // The generic fallback is always monochrome regardless of a
      // malformed descriptor's rendering hint.
      isTemplate: true,
      fallbackSystemName: _fallbackSystemName(kind),
      usesRuntimeAsset: hasAuthoredSVG,
    );
  }

  static const terminal = SupercliToolIcon(
    id: 'terminal',
    key: 'terminal',
    label: 'Terminal',
    kind: SupercliRuntimeKind.terminal,
    svgSource: '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M116,132.48l-72,64a6,6,0,0,1-8-9L103,128,36,68.49a6,6,0,0,1,8-9l72,64a6,6,0,0,1,0,9ZM216,186H120a6,6,0,0,0,0,12h96a6,6,0,0,0,0-12Z"></path></svg>',
    isTemplate: true,
    fallbackSystemName: 'terminal',
    usesRuntimeAsset: false,
  );

  /// Kind-owned generic marks so a future markdown-editor or Supercli App
  /// CLI does not inherit the agent sparkle when it ships without art.
  static String _genericSvg(SupercliRuntimeKind kind) {
    switch (kind) {
      case SupercliRuntimeKind.agent:
        return '<svg width="16" height="16" viewBox="0 0 24 24" fill="#FFFFFF" xmlns="http://www.w3.org/2000/svg"><path d="M12 2l1.64 5.36L19 9l-5.36 1.64L12 16l-1.64-5.36L5 9l5.36-1.64L12 2Z"/><path d="M19 15l.82 2.18L22 18l-2.18.82L19 21l-.82-2.18L16 18l2.18-.82L19 15Z"/></svg>';
      case SupercliRuntimeKind.app:
        return '<svg width="16" height="16" viewBox="0 0 256 256" fill="#FFFFFF" xmlns="http://www.w3.org/2000/svg"><path d="M208,40H48A16,16,0,0,0,32,56V200a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V56A16,16,0,0,0,208,40Zm0,16V88H48V56ZM48,200V104H208v96Z"/></svg>';
      case SupercliRuntimeKind.editor:
        return '<svg width="16" height="16" viewBox="0 0 256 256" fill="#FFFFFF" xmlns="http://www.w3.org/2000/svg"><path d="M208,24H72A16,16,0,0,0,56,40V216a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V40A16,16,0,0,0,208,24Zm0,192H72V40H208ZM96,80h80a8,8,0,0,1,0,16H96a8,8,0,0,1,0-16Zm0,40h80a8,8,0,0,1,0,16H96a8,8,0,0,1,0-16Zm0,40h48a8,8,0,0,1,0,16H96a8,8,0,0,1,0-16Z"/></svg>';
      case SupercliRuntimeKind.terminal:
        return '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M116,132.48l-72,64a6,6,0,0,1-8-9L103,128,36,68.49a6,6,0,0,1,8-9l72,64a6,6,0,0,1,0,9ZM216,186H120a6,6,0,0,0,0,12h96a6,6,0,0,0,0-12Z"></path></svg>';
    }
  }

  static String _fallbackSystemName(SupercliRuntimeKind kind) {
    switch (kind) {
      case SupercliRuntimeKind.agent:
        return 'sparkles';
      case SupercliRuntimeKind.app:
        return 'square.stack';
      case SupercliRuntimeKind.editor:
        return 'doc.plaintext';
      case SupercliRuntimeKind.terminal:
        return 'terminal';
    }
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is SupercliToolIcon &&
          runtimeType == other.runtimeType &&
          id == other.id;

  @override
  int get hashCode => id.hashCode;
}

/// The Host's App catalog as icons. Fed from every bootstrap snapshot
/// (`availableApps`) by each client's store.
abstract final class SupercliAppIconCatalog {
  static final Map<String, SupercliToolIcon> _byAppID = {};
  static final Map<String, SupercliToolIcon> _byBinary = {};

  /// Updates the catalog from the Host's app list.
  static void update(List<RemoteAppSummary> apps) {
    _byAppID.clear();
    _byBinary.clear();
    for (final app in apps) {
      final icon = SupercliToolIcon.forApp(
        id: app.id,
        name: app.name,
        iconSVG: null, // RemoteAppSummary doesn't have iconSvg in Dart port
      );
      _byAppID[app.id.toLowerCase()] = icon;
      // Extract binary name the same way icon() does: first token, unquoted, basename
      final token = app.command.trim().split(RegExp(r'\s+')).firstOrNull ?? '';
      final unquoted = token.replaceAll(RegExp('^[\'"]|[\'"]\$'), '');
      final binary = unquoted.split('/').last.toLowerCase();
      if (binary.isNotEmpty) {
        _byBinary[binary] = icon;
      }
    }
  }

  static SupercliToolIcon? icon({String? appID, String? command}) {
    if (appID != null && appID.isNotEmpty) {
      return _byAppID[appID.toLowerCase()];
    }
    if (command != null) {
      final token = command.trim().split(RegExp(r'\s+')).firstOrNull;
      if (token == null) return null;
      final unquoted = token.replaceAll(RegExp('^[\'"]|[\'"]\$'), '');
      final binary = unquoted.split('/').last.toLowerCase();
      if (binary.isEmpty) return null;
      return _byBinary[binary];
    }
    return null;
  }

  /// Whether the command's leading word is an installed Plugin (Supercli
  /// App) on the current Host.
  static bool isPluginCommand(String command) => icon(command: command) != null;
}
