/// Tests for tool_icons.dart
/// 
/// Port of RuntimeCatalogTests.swift icon-related behaviors.
import 'package:test/test.dart';
import '../lib/tool_icons.dart';
import '../lib/plugin_settings_list.dart';
import '../lib/runtime_catalog.dart';

void main() {
  group('SupercliToolIcon', () {
    test('terminal icon has expected properties', () {
      expect(SupercliToolIcon.terminal.id, 'terminal');
      expect(SupercliToolIcon.terminal.kind, SupercliRuntimeKind.terminal);
      expect(SupercliToolIcon.terminal.usesRuntimeAsset, false);
      expect(SupercliToolIcon.terminal.isTemplate, true);
    });

    test('forRuntime uses authored SVG when available', () {
      final runtime = SupercliRuntimeMetadata(
        stableID: 'test.runtime',
        slug: 'test',
        legacySlug: 'test',
        label: 'Test',
        platforms: {SupercliRuntimePlatform.macos},
        supportsQuickLaunch: true,
        kind: SupercliRuntimeKind.agent,
        iconKey: 'test',
        iconSVG: '<svg>custom</svg>',
        iconIsTemplate: false,
        windowPaddingX: 8,
        lifecycleSource: 'output',
        lifecycleAuthority: 'none',
        lifecycleFallback: 'none',
        completionReliable: false,
        attentionReliable: false,
        anchorStartEventToOutput: true,
        attentionClearsOnOutput: true,
        distrustStopsWhileOutputGrows: false,
      );
      final icon = SupercliToolIcon.forRuntime(runtime);
      expect(icon.id, 'test.runtime');
      expect(icon.svgSource, '<svg>custom</svg>');
      expect(icon.usesRuntimeAsset, true);
      expect(icon.isTemplate, false);
    });

    test('forRuntime uses generic fallback when no authored SVG', () {
      final runtime = SupercliRuntimeMetadata(
        stableID: 'test.runtime',
        slug: 'test',
        legacySlug: 'test',
        label: 'Test',
        platforms: {SupercliRuntimePlatform.macos},
        supportsQuickLaunch: true,
        kind: SupercliRuntimeKind.editor,
        iconKey: 'editor',
        iconIsTemplate: true,
        windowPaddingX: 8,
        lifecycleSource: 'output',
        lifecycleAuthority: 'none',
        lifecycleFallback: 'none',
        completionReliable: false,
        attentionReliable: false,
        anchorStartEventToOutput: true,
        attentionClearsOnOutput: true,
        distrustStopsWhileOutputGrows: false,
      );
      final icon = SupercliToolIcon.forRuntime(runtime);
      expect(icon.usesRuntimeAsset, false);
      expect(icon.isTemplate, true); // Generic fallback is always template
      expect(icon.fallbackSystemName, 'doc.plaintext');
    });

    test('resolving prefers provider ID over command', () {
      final icon = SupercliToolIcon.resolving(
        providerID: 'com.anthropic.claude-code',
        command: 'codex',
      );
      expect(icon.id, 'com.anthropic.claude-code');
    });

    test('resolving falls back to terminal for unknown', () {
      final icon = SupercliToolIcon.resolving(
        providerID: null,
        command: 'unknown-agent-xyz',
      );
      expect(icon, SupercliToolIcon.terminal);
    });
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
