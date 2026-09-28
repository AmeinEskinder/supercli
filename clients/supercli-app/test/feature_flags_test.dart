import 'package:test/test.dart';

import 'package:supercli_app/feature_flags.dart';

/// In-memory [FeatureFlagStore] for tests.
class _MemoryStore implements FeatureFlagStore {
  final _map = <String, bool>{};

  @override
  bool? getBool(String key) => _map[key];

  @override
  void setBool(String key, bool value) => _map[key] = value;

  @override
  void remove(String key) => _map.remove(key);
}

void main() {
  group('AppFeature definitions', () {
    test('all lists five features in display order', () {
      expect(AppFeature.all.map((f) => f.key), [
        'remoteWorkspaces',
        'worktrees',
        'sessionsMcp',
        'profiles',
        'browserMcp',
      ]);
    });

    test('defaultsKey uses the shipped supercli.experimental prefix', () {
      expect(
        AppFeature.worktrees.defaultsKey,
        'supercli.experimental.worktrees',
      );
      // The workspaces feature keeps its immutable shipped key.
      expect(AppFeature.workspaces.key, 'profiles');
      expect(
        AppFeature.workspaces.defaultsKey,
        'supercli.experimental.profiles',
      );
    });

    test('legacy env overrides are included', () {
      expect(AppFeature.workspaces.envOverrides, [
        'SUPERCLI_DEV_WORKSPACES',
        'SUPERCLI_DEV_PROFILES',
      ]);
      expect(AppFeature.worktrees.envOverrides, ['SUPERCLI_DEV_WORKTREES']);
    });

    test('computerUse is retired and not in all', () {
      expect(AppFeature.all, isNot(contains(AppFeature.computerUse)));
      expect(SupercliFeatureFlags.computerUseAvailable, isFalse);
      expect(SupercliFeatureFlags.isAvailable(AppFeature.computerUse), isFalse);
    });

    test('experimental section description is shared copy', () {
      expect(
        AppFeature.experimentalSectionDescription,
        contains('Early features'),
      );
    });
  });

  group('available feature lists', () {
    test('availableFeatures excludes the retired feature', () {
      expect(
        SupercliFeatureFlags.availableFeatures,
        orderedEquals(AppFeature.all),
      );
    });

    test('shipped vs experimental split', () {
      final shipped = SupercliFeatureFlags.availableShippedFeatures;
      final experimental = SupercliFeatureFlags.availableExperimentalFeatures;
      expect(shipped.map((f) => f.key), [
        'remoteWorkspaces',
        'worktrees',
        'sessionsMcp',
        'profiles',
      ]);
      expect(experimental.map((f) => f.key), ['browserMcp']);
      expect(
        shipped.length + experimental.length,
        SupercliFeatureFlags.availableFeatures.length,
      );
    });
  });

  group('isEnabled', () {
    test('retired feature is never enabled', () {
      expect(
        SupercliFeatureFlags.isEnabled(
          AppFeature.computerUse,
          env: {'SUPERCLI_DEV_WORKTREES': '1'},
        ),
        isFalse,
      );
    });

    test('env override force-enables', () {
      expect(
        SupercliFeatureFlags.isEnabled(
          AppFeature.browserMcp,
          env: {'SUPERCLI_DEV_BROWSER_MCP': '1'},
        ),
        isTrue,
      );
      // Legacy spellings also work.
      expect(
        SupercliFeatureFlags.isEnabled(
          AppFeature.workspaces,
          env: {'SUPERCLI_DEV_PROFILES': '1'},
        ),
        isTrue,
      );
    });

    test('env value other than "1" does not enable', () {
      expect(
        SupercliFeatureFlags.isEnabled(
          AppFeature.browserMcp,
          env: {'SUPERCLI_DEV_BROWSER_MCP': 'true'},
        ),
        isTrue, // defaultOn is true for browserMcp
      );
      final store = _MemoryStore();
      store.setBool(AppFeature.browserMcp.defaultsKey, false);
      expect(
        SupercliFeatureFlags.isEnabled(
          AppFeature.browserMcp,
          env: {'SUPERCLI_DEV_BROWSER_MCP': 'true'},
          ownStore: store,
        ),
        isFalse,
      );
    });

    test('own store value wins over default', () {
      final store = _MemoryStore();
      // browserMcp defaults on; an explicit false wins.
      expect(
        SupercliFeatureFlags.isEnabled(AppFeature.browserMcp, ownStore: store),
        isTrue,
      );
      store.setBool(AppFeature.browserMcp.defaultsKey, false);
      expect(
        SupercliFeatureFlags.isEnabled(AppFeature.browserMcp, ownStore: store),
        isFalse,
      );
    });

    test('non-default instance inherits the default workspace value', () {
      final own = _MemoryStore();
      final inherited = _MemoryStore();
      inherited.setBool(AppFeature.worktrees.defaultsKey, false);
      expect(
        SupercliFeatureFlags.isEnabled(
          AppFeature.worktrees,
          ownStore: own,
          inheritedStore: inherited,
          isDefaultInstance: false,
        ),
        isFalse,
      );
      // Own value beats the inherited one.
      own.setBool(AppFeature.worktrees.defaultsKey, true);
      expect(
        SupercliFeatureFlags.isEnabled(
          AppFeature.worktrees,
          ownStore: own,
          inheritedStore: inherited,
          isDefaultInstance: false,
        ),
        isTrue,
      );
    });

    test('falls back to built-in default', () {
      // computerUse is the only default-off feature in `all`... it is
      // retired, so use a synthetic check via browserMcp (defaultOn: true).
      expect(SupercliFeatureFlags.isEnabled(AppFeature.browserMcp), isTrue);
    });
  });

  group('persistence helpers', () {
    test('setEnabled then hasOwnSetting', () {
      final store = _MemoryStore();
      expect(
        SupercliFeatureFlags.hasOwnSetting(
          AppFeature.worktrees,
          ownStore: store,
        ),
        isFalse,
      );
      SupercliFeatureFlags.setEnabled(
        false,
        AppFeature.worktrees,
        ownStore: store,
      );
      expect(
        SupercliFeatureFlags.hasOwnSetting(
          AppFeature.worktrees,
          ownStore: store,
        ),
        isTrue,
      );
      expect(
        SupercliFeatureFlags.isEnabled(AppFeature.worktrees, ownStore: store),
        isFalse,
      );
    });

    test('setEnabled ignores the retired feature', () {
      final store = _MemoryStore();
      SupercliFeatureFlags.setEnabled(
        true,
        AppFeature.computerUse,
        ownStore: store,
      );
      expect(
        SupercliFeatureFlags.hasOwnSetting(
          AppFeature.computerUse,
          ownStore: store,
        ),
        isFalse,
      );
    });

    test('revertToInheritedBaseline drops every own value', () {
      final store = _MemoryStore();
      SupercliFeatureFlags.setEnabled(
        false,
        AppFeature.worktrees,
        ownStore: store,
      );
      SupercliFeatureFlags.setEnabled(
        false,
        AppFeature.browserMcp,
        ownStore: store,
      );
      SupercliFeatureFlags.revertToInheritedBaseline(ownStore: store);
      expect(
        SupercliFeatureFlags.hasOwnSetting(
          AppFeature.worktrees,
          ownStore: store,
        ),
        isFalse,
      );
      expect(
        SupercliFeatureFlags.hasOwnSetting(
          AppFeature.browserMcp,
          ownStore: store,
        ),
        isFalse,
      );
      // Defaults apply again.
      expect(
        SupercliFeatureFlags.isEnabled(AppFeature.worktrees, ownStore: store),
        isTrue,
      );
    });
  });

  group('mobileRemoteControlEnabled', () {
    test('env override force-enables', () {
      expect(
        SupercliFeatureFlags.mobileRemoteControlEnabled(
          env: {'SUPERCLI_DEV_MOBILE_REMOTE': '1'},
        ),
        isTrue,
      );
    });

    test('defaults to true when unset', () {
      expect(SupercliFeatureFlags.mobileRemoteControlEnabled(), isTrue);
    });

    test('stored false disables', () {
      final store = _MemoryStore();
      store.setBool('supercli.dev.mobileRemoteControl', false);
      expect(
        SupercliFeatureFlags.mobileRemoteControlEnabled(ownStore: store),
        isFalse,
      );
    });
  });

  group('remoteSupercliClientEnabled', () {
    test('env override force-enables', () {
      expect(
        SupercliFeatureFlags.remoteSupercliClientEnabled(
          env: {'SUPERCLI_REMOTE_ATTACH': '1'},
        ),
        isTrue,
      );
    });

    test('defaults to false', () {
      expect(SupercliFeatureFlags.remoteSupercliClientEnabled(), isFalse);
    });
  });

  group('WorkspaceFeature.pickerEnabled', () {
    test('either surface enables the picker', () {
      expect(
        WorkspaceFeature.pickerEnabled(
          localWorkspacesEnabled: true,
          remoteHostPickerEnabled: false,
        ),
        isTrue,
      );
      expect(
        WorkspaceFeature.pickerEnabled(
          localWorkspacesEnabled: false,
          remoteHostPickerEnabled: true,
        ),
        isTrue,
      );
      expect(
        WorkspaceFeature.pickerEnabled(
          localWorkspacesEnabled: false,
          remoteHostPickerEnabled: false,
        ),
        isFalse,
      );
    });
  });
}
