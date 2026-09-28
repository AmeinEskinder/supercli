/// Feature flag definitions and evaluation logic.
///
/// Port of `FeatureFlags.swift` (the pure-logic half).
///
/// An [AppFeature] is a user-facing optional feature, toggleable in
/// Settings > Features. Adding a feature is a single static entry: it
/// automatically gets a toggle row in the Features tab and an [isEnabled]
/// check gates can use. A feature is either shipped (the plain Features
/// list) or `experimental` (the tab's Experimental section).
///
/// Persistence is a platform key-value overlay (never app-state.json),
/// keyed by [AppFeature.defaultsKey] — the `supercli.experimental.` prefix
/// is the shipped spelling for every feature, graduated or not. The
/// platform layer provides that overlay through [FeatureFlagStore];
/// this library stays free of `dart:io` and platform plugins so the
/// evaluation logic is unit-testable.
library;

/// A user-facing optional feature, toggleable in Settings > Features.
final class AppFeature {
  const AppFeature({
    required this.key,
    required this.title,
    required this.summary,
    this.envOverride,
    this.legacyEnvOverrides = const [],
    this.defaultOn = false,
    this.experimental = false,
  });

  /// Stable id; also the key suffix. Never rename once shipped.
  final String key;

  final String title;
  final String summary;

  /// `supercli.experimental.<key>` — the shipped spelling for every
  /// feature, graduated or not.
  String get defaultsKey => 'supercli.experimental.$key';

  /// Dev escape hatch: force-enables the feature when the env var == "1".
  final String? envOverride;

  /// Older env spellings that still force-enable the feature.
  final List<String> legacyEnvOverrides;

  final bool defaultOn;

  /// Still being shaped: listed under the Features tab's Experimental
  /// section instead of the shipped list.
  final bool experimental;

  /// All env vars (current + legacy) that force-enable this feature.
  List<String> get envOverrides => [
    if (envOverride != null) envOverride!,
    ...legacyEnvOverrides,
  ];

  // --- Feature definitions ---

  /// Run sessions in isolated git worktrees so multiple agents can work the
  /// same repo in parallel.
  static const worktrees = AppFeature(
    key: 'worktrees',
    title: 'Git worktrees',
    summary:
        'Run sessions in an isolated git worktree of a project so '
        'multiple agents can work the same repo in parallel without touching '
        "each other's files. Adds worktree controls to the project menu, "
        'sidebar, and the Worktrees settings tab.',
    envOverride: 'SUPERCLI_DEV_WORKTREES',
    defaultOn: true,
  );

  /// Sessions MCP: agent sessions can read other sessions and request write
  /// access to explicit targets.
  static const sessionsMcp = AppFeature(
    key: 'sessionsMcp',
    title: 'Sessions use',
    summary:
        'Let an agent session see your other sessions: it can read them '
        'all, and asks before writing to another session unless you already '
        'approved that pair. These are cooperation controls, not a sandbox '
        'against commands running as your macOS user. Adds the Sessions '
        'settings tab. Applies when a session starts, so already-running '
        'sessions pick it up after a restart.',
    envOverride: 'SUPERCLI_DEV_SESSIONS_MCP',
    defaultOn: true,
  );

  /// Workspaces: use additional, fully isolated Supercli homes on this Mac.
  /// The persisted key is deliberately still `profiles`: shipped
  /// experimental-feature keys are immutable.
  static const workspaces = AppFeature(
    key: 'profiles',
    title: 'Workspaces',
    summary:
        'Use extra, fully separate workspaces on this Mac — each '
        'workspace has its own sessions, projects, presets, settings, and '
        'pairs with your phone as its own workspace. Adds the Workspaces '
        'settings tab.',
    envOverride: 'SUPERCLI_DEV_WORKSPACES',
    legacyEnvOverrides: ['SUPERCLI_DEV_PROFILES'],
    defaultOn: true,
  );

  /// Legacy preference identity retained for decoding saved settings.
  /// Retired in every build; never offered as a toggle.
  static const computerUse = AppFeature(
    key: 'computerUse',
    title: 'Computer use',
    summary: 'Supercli computer use has been retired.',
    experimental: true,
  );

  /// Browser MCP: agent sessions get an isolated real browser.
  static const browserMcp = AppFeature(
    key: 'browserMcp',
    title: 'Browser use',
    summary:
        'Let agent sessions drive a real browser — open pages, click, '
        'fill forms, and take screenshots. Each session gets its own '
        'isolated browser with no access to your normal browser profile. '
        'Browser access prompts are cooperation controls, not a sandbox '
        'against commands running as your macOS user. Adds the Browser '
        'settings tab.',
    envOverride: 'SUPERCLI_DEV_BROWSER_MCP',
    defaultOn: true,
    experimental: true,
  );

  /// Remote workspaces in the released app: the Host picker, Share This
  /// Mac…, Add Workspace… > Nearby/code and SSH.
  static const remoteWorkspaces = AppFeature(
    key: 'remoteWorkspaces',
    title: 'Remote workspaces',
    summary:
        'Add and control workspaces on other machines — pair another '
        'Mac, a headless `supercli serve` box, or an SSH host — and share '
        'this Mac with other devices. Direct connections are for your own '
        'network or VPN; Supercli Link carries the encrypted path when you '
        'are away.',
    envOverride: 'SUPERCLI_DEV_REMOTE_WORKSPACES',
    defaultOn: true,
  );

  /// Everything shown in Settings > Features, in display order (shipped
  /// features first; the panel then groups the experimental ones under
  /// their own section).
  static const List<AppFeature> all = [
    remoteWorkspaces,
    worktrees,
    sessionsMcp,
    workspaces,
    browserMcp,
  ];

  /// Header copy for the Features tab's Experimental section, shared by the
  /// local, per-workspace, and remote Host panels.
  static const String experimentalSectionDescription =
      'Early features that are still being shaped. They can change or '
      'disappear between releases. Turn one off here if it gets in the way.';

  @override
  bool operator ==(Object other) => other is AppFeature && other.key == key;

  @override
  int get hashCode => key.hashCode;

  @override
  String toString() => 'AppFeature($key)';
}

/// Key-value persistence for feature flags.
///
/// The platform layer implements this (UserDefaults on macOS). Kept as a
/// tiny interface so the evaluation logic stays testable without platform
/// plugins.
abstract interface class FeatureFlagStore {
  /// Returns the stored bool, or null when the key was never written.
  bool? getBool(String key);

  void setBool(String key, bool value);

  void remove(String key);
}

/// Feature flag queries. Mirrors `SupercliFeatureFlags`.
abstract final class SupercliFeatureFlags {
  /// Kept for old saved settings; this feature is retired in every build.
  static bool get computerUseAvailable => false;

  static bool isAvailable(AppFeature feature) =>
      feature != AppFeature.computerUse;

  /// Every feature this build offers a toggle for, in display order.
  static List<AppFeature> get availableFeatures =>
      AppFeature.all.where(isAvailable).toList();

  /// The shipped features: the Features tab's plain list.
  static List<AppFeature> get availableShippedFeatures =>
      availableFeatures.where((f) => !f.experimental).toList();

  /// The features still marked experimental: the tab's Experimental section.
  static List<AppFeature> get availableExperimentalFeatures =>
      availableFeatures.where((f) => f.experimental).toList();

  /// Whether a feature is currently enabled — env override first (dev
  /// escape hatch), then this workspace's own stored preference, then the
  /// default workspace's value (a local workspace with no setting of its
  /// own inherits the default's), then the feature's built-in default.
  ///
  /// [env] is the process environment (defaults to an empty map; pass
  /// `Platform.environment` from `dart:io` in production). [ownStore] is
  /// this workspace's overlay; [inheritedStore] is the default workspace's
  /// overlay, consulted only when [isDefaultInstance] is false.
  static bool isEnabled(
    AppFeature feature, {
    Map<String, String> env = const {},
    FeatureFlagStore? ownStore,
    FeatureFlagStore? inheritedStore,
    bool isDefaultInstance = true,
  }) {
    if (!isAvailable(feature)) return false;
    if (feature.envOverrides.any((name) => env[name] == '1')) return true;
    final own = ownStore?.getBool(feature.defaultsKey);
    if (own != null) return own;
    if (!isDefaultInstance) {
      final inherited = inheritedStore?.getBool(feature.defaultsKey);
      if (inherited != null) return inherited;
    }
    return feature.defaultOn;
  }

  /// Whether this workspace records its OWN value for the feature — the
  /// revert-to-default button's enablement.
  static bool hasOwnSetting(AppFeature feature, {FeatureFlagStore? ownStore}) =>
      ownStore?.getBool(feature.defaultsKey) != null;

  /// Revert-to-inherited baseline for feature flags: drop every own value
  /// so this workspace inherits the default workspace's flags again.
  static void revertToInheritedBaseline({FeatureFlagStore? ownStore}) {
    final store = ownStore;
    if (store == null) return;
    for (final feature in AppFeature.all) {
      store.remove(feature.defaultsKey);
    }
  }

  /// Persist a user preference for a feature.
  static void setEnabled(
    bool enabled,
    AppFeature feature, {
    FeatureFlagStore? ownStore,
  }) {
    if (!isAvailable(feature)) return;
    ownStore?.setBool(feature.defaultsKey, enabled);
  }

  static bool mobileRemoteControlEnabled({
    Map<String, String> env = const {},
    FeatureFlagStore? ownStore,
  }) {
    if (env['SUPERCLI_DEV_MOBILE_REMOTE'] == '1') return true;
    const key = 'supercli.dev.mobileRemoteControl';
    final stored = ownStore?.getBool(key);
    if (stored == null) return true;
    return stored;
  }

  /// Mac-as-client: connect this Supercli to another Supercli's remote
  /// server and attach to its sessions. Experimental; pairs with the
  /// Rust-side SUPERCLI_REMOTE_ATTACH=1 gate on the attach CLI.
  static bool remoteSupercliClientEnabled({
    Map<String, String> env = const {},
    FeatureFlagStore? ownStore,
  }) {
    if (env['SUPERCLI_REMOTE_ATTACH'] == '1') return true;
    return ownStore?.getBool('supercli.dev.remoteSupercliClient') ?? false;
  }
}

/// Desktop workspace switching gates. Mirrors `WorkspaceFeature`.
abstract final class WorkspaceFeature {
  /// Desktop workspace switching is a local feature, even though its
  /// transport reuses the Host client stack.
  static bool pickerEnabled({
    required bool localWorkspacesEnabled,
    required bool remoteHostPickerEnabled,
  }) => localWorkspacesEnabled || remoteHostPickerEnabled;
}
