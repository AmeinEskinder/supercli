/// Tests for tool_icons.dart
///
/// Port of RuntimeCatalogTests.swift icon-related behaviors.
///
/// Note: `SupercliToolIcon.resolving` calls through to the Rust catalog via
/// FFI. Those tests require the real `supercli-client-ffi` cdylib: CI builds
/// it (`cargo build -p supercli-client-ffi --release`) and sets
/// `SUPERCLI_FFI_LIB`. Pure-icon tests (forRuntime/terminal) need no library.
library;

import 'package:test/test.dart';
import 'package:supercli_app/tool_icons.dart';
import 'package:supercli_app/plugin_settings_list.dart';

/// Minimal runtime descriptor map, shaped like the JSON the Rust catalog
/// returns over FFI.
Map<String, dynamic> testRuntime({
  String id = 'test.runtime',
  String slug = 'test',
  String legacySlug = 'test',
  String label = 'Test',
  bool supportsQuickLaunch = true,
  String kind = 'agent',
  String? icon,
}) {
  final m = <String, dynamic>{
    'id': id,
    'slug': slug,
    'legacy_slug': legacySlug,
    'label': label,
    'supports_quick_launch': supportsQuickLaunch,
    'kind': kind,
  };
  if (icon != null) m['icon'] = icon;
  return m;
}

void main() {
  group('SupercliToolIcon', () {
    test('terminal icon has expected properties', () {
      expect(SupercliToolIcon.terminal.id, 'terminal');
      expect(SupercliToolIcon.terminal.kind, SupercliRuntimeKind.terminal);
      expect(SupercliToolIcon.terminal.usesRuntimeAsset, false);
      expect(SupercliToolIcon.terminal.isTemplate, true);
    });

    test('forRuntime uses authored SVG when available', () {
      final icon = SupercliToolIcon.forRuntime(
        testRuntime(icon: '<svg>custom</svg>'),
      );
      expect(icon.id, 'test.runtime');
      expect(icon.svgSource, '<svg>custom</svg>');
      expect(icon.usesRuntimeAsset, true);
      expect(icon.isTemplate, true); // generic fallback template flag
    });

    test('forRuntime uses generic fallback when no authored SVG', () {
      final icon = SupercliToolIcon.forRuntime(
        testRuntime(kind: 'editor'),
      );
      expect(icon.usesRuntimeAsset, false);
      expect(icon.isTemplate, true); // Generic fallback is always template
      expect(icon.fallbackSystemName, 'doc.plaintext');
      expect(icon.kind, SupercliRuntimeKind.editor);
    });

    test('forRuntime maps unknown kind to terminal', () {
      final icon = SupercliToolIcon.forRuntime(testRuntime(kind: 'bogus'));
      expect(icon.kind, SupercliRuntimeKind.terminal);
    });

    test(
      'resolving prefers provider ID over command (requires FFI lib)',
      () {
        final icon = SupercliToolIcon.resolving(
          providerID: 'com.anthropic.claude-code',
          command: 'codex',
        );
        expect(icon.id, 'com.anthropic.claude-code');
      },
      tags: 'ffi',
    );

    test(
      'resolving falls back to terminal for unknown (requires FFI lib)',
      () {
        final icon = SupercliToolIcon.resolving(
          providerID: null,
          command: 'unknown-agent-xyz',
        );
        expect(icon, SupercliToolIcon.terminal);
      },
      tags: 'ffi',
    );
  });

  group('SupercliAppIconCatalog', () {
    test('update and icon lookup by app ID', () {
      SupercliAppIconCatalog.update([
        RemoteAppSummary(
          id: 'supercli.app.markdown',
          name: 'Markdown',
          command: 'supercli-markdown',
          installed: true,
        ),
      ]);
      final icon = SupercliAppIconCatalog.icon(appID: 'supercli.app.markdown');
      expect(icon, isNotNull);
      expect(icon!.id, 'app:supercli.app.markdown');

      // Cleanup
      SupercliAppIconCatalog.update([]);
    });

    test('icon lookup by command binary', () {
      SupercliAppIconCatalog.update([
        RemoteAppSummary(
          id: 'test.app',
          name: 'Test',
          command: '/opt/bin/myapp --flag',
          installed: true,
        ),
      ]);
      final icon = SupercliAppIconCatalog.icon(command: '/opt/bin/myapp --flag');
      expect(icon, isNotNull);

      SupercliAppIconCatalog.update([]);
    });

    test('isPluginCommand detects installed plugins', () {
      SupercliAppIconCatalog.update([
        RemoteAppSummary(
          id: 'test.app',
          name: 'Test',
          command: 'myapp',
          installed: true,
        ),
      ]);
      expect(SupercliAppIconCatalog.isPluginCommand('myapp --flag'), true);
      expect(SupercliAppIconCatalog.isPluginCommand('unknown'), false);

      SupercliAppIconCatalog.update([]);
    });
  });
}
