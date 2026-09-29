/// Local Features settings panel.
///
/// Port of `FeaturesSettingsPanel` (SettingsView.swift, 4330-4429).
///
/// Turn Supercli's optional features on or off — no restart needed. A
/// workspace instance inherits the default workspace's feature flags until
/// it sets its own; the revert is offered inline. Shipped features render
/// first; experimental ones group under their own section.
///
/// Feature definitions come from `AppFeature` (FeatureFlags.swift):
/// everything shown in Settings ▸ Features, in display order. Shipped
/// feature keys are immutable — the workspaces feature's persisted key is
/// deliberately still `profiles`.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart' as base;

/// Header copy for the Features tab's Experimental section, shared by the
/// local, per-workspace, and remote Host panels
/// (Swift: `SupercliFeatureFlags.experimentalSectionDescription`).
const String experimentalSectionDescription =
    'Early features that are still being shaped. They can change or '
    'disappear between releases. Turn one off here if it gets in the way.';

/// A feature toggle descriptor (Swift: `AppFeature`).
final class FeatureDefinition {
  const FeatureDefinition({
    required this.key,
    required this.title,
    required this.summary,
    this.defaultOn = false,
    this.isExperimental = false,
  });

  final String key;
  final String title;
  final String summary;
  final bool defaultOn;
  final bool isExperimental;
}

/// Everything shown in Settings ▸ Features, in display order (shipped
/// features first; the panel groups the experimental ones under their own
/// section). Remote workspaces, Git worktrees, Sessions use, and Workspaces
/// graduated on 2026-09-08; Browser use stays experimental.
/// (Swift: `SupercliFeatureFlags.all`.)
const List<FeatureDefinition> allFeatures = [
  FeatureDefinition(
    key: 'remoteWorkspaces',
    title: 'Remote workspaces',
    summary: 'Add and control workspaces on other machines — pair another Mac, a '
        'headless `supercli serve` box, or an SSH host — and share this Mac with '
        'other devices. Direct connections are for your own network or VPN; '
        'Supercli Link carries the encrypted path when you are away.',
    defaultOn: true,
  ),
  FeatureDefinition(
    key: 'worktrees',
    title: 'Git worktrees',
    summary: 'Run sessions in an isolated git worktree of a project so multiple '
        'agents can work the same repo in parallel without touching each other\'s '
        'files. Adds worktree controls to the project menu, sidebar, and the '
        'Worktrees settings tab.',
    defaultOn: true,
  ),
  FeatureDefinition(
    key: 'sessionsMcp',
    title: 'Sessions use',
    summary: 'Let an agent session see your other sessions: it can read them all, '
        'and asks before writing to another session unless you already approved '
        'that pair. These are cooperation controls, not a sandbox against commands '
        'running as your macOS user. Adds the Sessions settings tab. Applies when '
        'a session starts, so already-running sessions pick it up after a restart.',
    defaultOn: true,
  ),
  FeatureDefinition(
    // The persisted key is deliberately still `profiles`: shipped
    // experimental-feature keys are immutable.
    key: 'profiles',
    title: 'Workspaces',
    summary: 'Use extra, fully separate workspaces on this Mac — each '
        'workspace has its own sessions, projects, presets, settings, and '
        'pairs with your phone as its own workspace. Adds the Workspaces '
        'settings tab.',
    defaultOn: true,
  ),
  FeatureDefinition(
    key: 'browserMcp',
    title: 'Browser use',
    summary: 'Let agent sessions drive a real browser — open pages, click, '
        'fill forms, and take screenshots. Each session gets its own isolated '
        'browser with no access to your normal browser profile. Browser access '
        'prompts are cooperation controls, not a sandbox against commands '
        'running as your macOS user. Adds the Browser settings tab.',
    defaultOn: true,
    isExperimental: true,
  ),
];

/// Local Features settings panel (Swift: `FeaturesSettingsPanel`).
final class FeaturesSettingsPanel {
  const FeaturesSettingsPanel({
    this.isDefaultInstance = true,
    this.defaultWorkspaceLabel = 'Personal',
    this.hasOwnSettings = false,
    this.values = const {},
  });

  final bool isDefaultInstance;
  final String defaultWorkspaceLabel;
  final bool hasOwnSettings;
  final Map<String, bool> values;

  List<FeatureDefinition> get shipped =>
      allFeatures.where((f) => !f.isExperimental).toList();

  List<FeatureDefinition> get experimental =>
      allFeatures.where((f) => f.isExperimental).toList();

  UiNode _featureRow(FeatureDefinition feature) {
    return UiColumn('feature-${feature.key}-labeled', [
      base.SettingsToggle(
        id: 'feature-${feature.key}',
        label: feature.title,
        value: values[feature.key] ?? feature.defaultOn,
      ).fallback(),
      UiText('feature-${feature.key}-summary', feature.summary),
    ]);
  }

  UiNode build() {
    final sections = <UiNode>[
      const SettingsPaneHeader(
        title: 'Features',
        description: 'Turn Supercli\'s optional features on or off — no restart needed.',
      ).build(),
    ];
    if (!isDefaultInstance) {
      sections.add(UiColumn('features-inherit', [
        SettingsSectionHeader(
          title: 'Inherits from $defaultWorkspaceLabel',
          description: 'This workspace uses the default workspace\'s features '
              'until a toggle below is changed. Revert drops its own values.',
        ).build(),
        UiButton('features-use-inherited',
            'Use $defaultWorkspaceLabel\'s features'),
      ]));
    }
    if (allFeatures.isEmpty) {
      sections.add(const UiText('features-empty',
          'No optional features right now. Check back after an update.'));
    } else {
      sections.add(UiColumn('features-shipped', [
        for (final f in shipped) _featureRow(f),
      ]));
      if (experimental.isNotEmpty) {
        sections.add(UiColumn('features-experimental', [
          const SettingsSectionHeader(
            title: 'Experimental',
            description: experimentalSectionDescription,
          ).build(),
          for (final f in experimental) _featureRow(f),
        ]));
      }
    }
    return UiColumn('features-settings', sections);
  }
}
