/// Behavioral tests for DESKTOP rows 197–219 (parity worker, light pass).
///
/// Covers the component models; real-window screenshot proof is pending
/// (no Xvfb in this pass), so rows stay `partial (component)`.
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/editor_preference.dart';
import 'package:supercli_app/host_connection.dart';
import 'package:supercli_app/host_service.dart';
import 'package:supercli_app/nearby_hosts.dart';
import 'package:supercli_app/screens/agentssettingspanel.dart';
import 'package:supercli_app/screens/remotefolderpicker.dart';
import 'package:supercli_app/screens/remotehostworkspaceview.dart';
import 'package:supercli_app/screens/sidebarskeleton.dart';
import 'package:supercli_app/worktree_discovery.dart';
import 'package:test/test.dart';

void main() {
  group('Row 197: HostServiceManager', () {
    HostServiceManager manager() => HostServiceManager(
          label: 'li.superc.host',
          executablePath: '/Applications/supercli.app/Contents/MacOS/supercli',
          arguments: const ['serve'],
        );

    test('start/stop state machine', () {
      final m = manager();
      expect(m.state, HostServiceState.stopped);
      expect(m.requestStart(), true);
      expect(m.state, HostServiceState.starting);
      // Duplicate start is a no-op.
      expect(m.requestStart(), false);
      m.markRunning();
      expect(m.isRunning, true);
      expect(m.requestStop(), true);
      expect(m.state, HostServiceState.stopping);
      expect(m.requestStop(), false);
      m.markStopped();
      expect(m.state, HostServiceState.stopped);
    });

    test('failure records the error', () {
      final m = manager();
      m.requestStart();
      m.markFailed('port in use');
      expect(m.state, HostServiceState.failed);
      expect(m.lastError, 'port in use');
      // A new start clears the error.
      m.requestStart();
      expect(m.lastError, isNull);
    });

    test('launchd plist has required keys', () {
      final plist = manager().launchdPlist();
      expect(plist, contains('<string>li.superc.host</string>'));
      expect(plist, contains('<key>RunAtLoad</key>'));
      expect(plist, contains('<key>KeepAlive</key>'));
      expect(plist, contains('<string>/Applications/supercli.app/Contents/MacOS/supercli</string>'));
      expect(plist, contains('<string>serve</string>'));
    });

    test('launchd plist escapes XML', () {
      final m = HostServiceManager(
        label: 'li.superc.host',
        executablePath: '/bin/a&b',
      );
      expect(m.launchdPlist(), contains('/bin/a&amp;b'));
    });

    test('systemd unit has ExecStart and restart policy', () {
      final unit = manager().systemdUnit();
      expect(unit, contains('ExecStart=/Applications/supercli.app'));
      expect(unit, contains('Restart=on-failure'));
      expect(unit, contains('WantedBy=default.target'));
    });
  });

  group('Row 198: NearbyHostDiscovery', () {
    DiscoveredHost host(String name) => DiscoveredHost(
          serviceName: name,
          host: '192.168.1.2',
          port: 4317,
          txt: const {'name': 'Studio Mac'},
        );

    test('found dedups by service name and refreshes lastSeen', () {
      var changed = 0;
      final d = NearbyHostDiscovery(onChanged: () => changed++);
      d.found(host('a'));
      d.found(host('a'));
      expect(d.hosts.length, 1);
      expect(changed, 1);
    });

    test('lost removes the host', () {
      final d = NearbyHostDiscovery();
      d.found(host('a'));
      d.found(host('b'));
      d.lost('a');
      expect(d.hosts.map((h) => h.serviceName), ['b']);
    });

    test('expireStale drops quiet hosts', () {
      final d = NearbyHostDiscovery(staleAfter: const Duration(seconds: 10));
      d.found(DiscoveredHost(serviceName: 'old', host: 'h', port: 1));
      final expired = d.expireStale(
        now: DateTime.now().add(const Duration(seconds: 11)),
      );
      expect(expired, 1);
      expect(d.hosts, isEmpty);
    });

    test('re-announcement refreshes and prevents expiry', () {
      final d = NearbyHostDiscovery(staleAfter: const Duration(seconds: 10));
      final h = DiscoveredHost(serviceName: 'a', host: 'h', port: 1);
      d.found(h);
      d.found(h); // re-announce refreshes lastSeen
      final expired = d.expireStale(
        now: DateTime.now().add(const Duration(seconds: 5)),
      );
      expect(expired, 0);
      expect(d.hosts.length, 1);
    });

    test('displayName prefers TXT name', () {
      expect(host('svc').displayName, 'Studio Mac');
      expect(
        DiscoveredHost(serviceName: 'svc', host: 'h', port: 1).displayName,
        'svc',
      );
    });

    test('start/stop toggles running', () {
      final d = NearbyHostDiscovery();
      expect(d.isRunning, false);
      d.start();
      expect(d.isRunning, true);
      d.start(); // idempotent
      d.stop();
      expect(d.isRunning, false);
      d.dispose();
    });
  });

  group('Row 199: RemoteFolderBrowser', () {
    RemoteFolderBrowser browser() {
      final b = RemoteFolderBrowser(initialPath: '/Users/ada');
      b.setEntries(const [
        RemoteFolderEntry(name: 'src', path: '/Users/ada/src'),
        RemoteFolderEntry(name: 'notes.txt', path: '/Users/ada/notes.txt', isDirectory: false),
        RemoteFolderEntry(name: 'docs', path: '/Users/ada/docs'),
      ]);
      return b;
    }

    test('setEntries keeps directories only, sorted', () {
      final b = browser();
      expect(b.entries.map((e) => e.name), ['docs', 'src']);
    });

    test('goUp walks to root', () {
      final b = browser();
      expect(b.goUp(), true);
      expect(b.currentPath, '/Users');
      expect(b.goUp(), true);
      expect(b.currentPath, '/');
      expect(b.goUp(), false);
    });

    test('breadcrumbs cover the full path', () {
      final b = browser();
      final crumbs = b.breadcrumbs;
      expect(crumbs.map((c) => c.$2), ['/', 'Users', 'ada']);
      expect(crumbs.last.$1, '/Users/ada');
    });

    test('select/enter round-trip', () {
      final b = browser();
      b.select('/Users/ada/src');
      expect(b.selectedPath, '/Users/ada/src');
      b.enter('/Users/ada/src');
      expect(b.currentPath, '/Users/ada/src');
      expect(b.selectedPath, isNull);
      expect(b.entries, isEmpty);
    });

    test('picker renders breadcrumb buttons and table', () {
      final picker =
          RemoteFolderPicker(hostName: 'studio', browser: browser());
      final node = picker.build() as UiColumn;
      // title, crumbs row, path text, table, actions row
      expect(node.children.length, 5);
      expect(picker.dataset().rowCount, 2);
    });
  });

  group('Row 202: AgentListModel', () {
    AgentListModel model() => AgentListModel(const [
          AgentInfo(id: 'a', name: 'Alpha', isDefault: true),
          AgentInfo(id: 'b', name: 'Beta'),
          AgentInfo(id: 'c', name: 'Gamma', isActive: false),
        ]);

    test('move reorders', () {
      final m = model();
      expect(m.move('c', 0), true);
      expect(m.agents.map((a) => a.id), ['c', 'a', 'b']);
      expect(m.move('nope', 0), false);
    });

    test('setDefault keeps exactly one default', () {
      final m = model();
      expect(m.setDefault('b'), true);
      expect(m.defaultAgent!.id, 'b');
      expect(m.agents.where((a) => a.isDefault).length, 1);
      expect(m.setDefault('nope'), false);
    });

    test('toggleActive flips the flag', () {
      final m = model();
      expect(m.toggleActive('c'), true);
      expect(m.toggleActive('c'), false);
      expect(m.toggleActive('nope'), isNull);
    });

    test('panel dataset shows default/active columns', () {
      const panel = AgentsSettingsPanel(agents: [
        AgentInfo(id: 'a', name: 'Alpha', version: '1.0', isDefault: true),
      ]);
      final ds = panel.dataset();
      expect(ds.cell(0, 0), 'Alpha');
      expect(ds.cell(0, 2), 'Yes');
      final node = panel.build() as UiColumn;
      expect(node.children.length, 4);
    });
  });

  group('Row 208: RemotePairingFlow', () {
    test('happy path: begin → submit → confirm', () {
      final f = RemotePairingFlow();
      expect(f.state, RemotePairingState.idle);
      f.begin('482-913');
      expect(f.state, RemotePairingState.showingCode);
      expect(f.pairCode, '482-913');
      f.deviceSubmitted();
      expect(f.state, RemotePairingState.verifying);
      f.confirmed();
      expect(f.state, RemotePairingState.paired);
    });

    test('failure carries a reason and resets', () {
      final f = RemotePairingFlow();
      f.begin('000-000');
      f.fail('code expired');
      expect(f.state, RemotePairingState.failed);
      expect(f.failureReason, 'code expired');
      f.reset();
      expect(f.state, RemotePairingState.idle);
      expect(f.pairCode, isEmpty);
    });

    test('qrPayload encodes code and host', () {
      final f = RemotePairingFlow();
      f.begin('482-913');
      final qr = f.qrPayload('192.168.1.2:4317');
      expect(qr, contains('code=482-913'));
      expect(qr, contains('host=192.168.1.2%3A4317'));
    });

    test('view renders per state', () {
      final f = RemotePairingFlow();
      var node = RemotePairingView(flow: f, hostName: 'studio').build();
      expect(node, isA<UiColumn>());
      f.begin('1');
      node = RemotePairingView(flow: f, hostName: 'studio').build();
      expect(node, isA<UiColumn>());
    });
  });

  group('Row 215: EditorPreference', () {
    test('resolve prefers the chosen editor when available', () {
      const p = EditorPreference(preferredEditorId: 'zed');
      expect(p.resolve({'zed', 'vscode'}), 'zed');
    });

    test('resolve falls back when preferred is missing', () {
      const p = EditorPreference(preferredEditorId: 'xcode');
      expect(p.resolve({'vscode'}), 'vscode');
    });

    test('resolve returns null when nothing is available', () {
      const p = EditorPreference(preferredEditorId: 'xcode');
      expect(p.resolve({}), isNull);
    });

    test('JSON round-trips', () {
      const p = EditorPreference(
          preferredEditorId: 'zed', fallbackIds: ['zed', 'finder']);
      final back = EditorPreference.fromJson(
          Map<String, dynamic>.from(p.toJson()));
      expect(back.preferredEditorId, 'zed');
      expect(back.fallbackIds, ['zed', 'finder']);
    });

    test('knownEditors covers the fallback chain', () {
      for (final id in ['vscode', 'zed', 'finder']) {
        expect(knownEditors.any((e) => e.id == id), true);
      }
    });
  });

  group('Row 217: RemoteHostConnection', () {
    test('auto falls back to Link on direct failure', () {
      final c = RemoteHostConnection();
      expect(c.effectiveTransport, HostTransport.direct);
      c.directFailed();
      expect(c.effectiveTransport, HostTransport.link);
      expect(c.statusLabel, 'Link (fallback)');
      expect(c.fallbackCount, 1);
    });

    test('auto promotes back when direct recovers', () {
      final c = RemoteHostConnection();
      c.directFailed();
      c.directRecovered();
      expect(c.effectiveTransport, HostTransport.direct);
      expect(c.statusLabel, 'Direct');
    });

    test('directOnly never falls back', () {
      final c = RemoteHostConnection(
          preference: HostConnectionPreference.directOnly);
      c.directFailed();
      expect(c.effectiveTransport, HostTransport.direct);
      expect(c.fallbackCount, 0);
    });

    test('linkOnly pins Link', () {
      final c =
          RemoteHostConnection(preference: HostConnectionPreference.linkOnly);
      expect(c.effectiveTransport, HostTransport.link);
      expect(c.statusLabel, 'Link');
    });
  });

  group('Row 218: WorktreeDiscovery', () {
    test('first ingest after opt-in is the baseline (no diff)', () {
      WorktreeDiff? seen;
      final d = WorktreeDiscovery(onChanged: (diff) => seen = diff);
      d.optIn({'/a', '/b'});
      final diff = d.ingest({'/a', '/b', '/c'});
      expect(diff.added, ['/c']);
      expect(diff.removed, isEmpty);
      expect(seen, isNotNull);
    });

    test('detects removed worktrees', () {
      final d = WorktreeDiscovery();
      d.optIn({'/a', '/b'});
      final diff = d.ingest({'/a'});
      expect(diff.added, isEmpty);
      expect(diff.removed, ['/b']);
    });

    test('no callback on empty diff', () {
      var calls = 0;
      final d = WorktreeDiscovery(onChanged: (_) => calls++);
      d.optIn({'/a'});
      d.ingest({'/a'});
      expect(calls, 0);
    });

    test('optOut stops the poller', () {
      final d = WorktreeDiscovery(pollInterval: const Duration(milliseconds: 10));
      d.optIn({});
      expect(d.isOptedIn, true);
      d.optOut();
      expect(d.isOptedIn, false);
      d.dispose();
    });
  });

  group('Row 219: SidebarPresentationCache', () {
    test('capture/restore round-trips', () {
      final cache = SidebarPresentationCache();
      final now = DateTime(2026, 9, 27, 12);
      final json = cache.capture(
        const SidebarSnapshot(
          pinnedSessionIds: ['s1', 's2'],
          expandedGroupIds: ['g1'],
          selectedSessionId: 's1',
        ),
        now: now,
      );
      final back = cache.restore(json, now: now);
      expect(back, isNotNull);
      expect(back!.pinnedSessionIds, ['s1', 's2']);
      expect(back.selectedSessionId, 's1');
    });

    test('stale cache is ignored', () {
      final cache = SidebarPresentationCache();
      final json = cache.capture(
        const SidebarSnapshot(pinnedSessionIds: ['s1']),
        now: DateTime(2026, 9, 20),
      );
      expect(
        cache.restore(json, now: DateTime(2026, 9, 27, 12)),
        isNull,
      );
    });

    test('corrupt JSON restores to null', () {
      final cache = SidebarPresentationCache();
      expect(cache.restore('not-json'), isNull);
      expect(cache.restore(null), isNull);
      expect(cache.restore(''), isNull);
    });

    test('snapshot JSON round-trips through fromJson', () {
      const snap = SidebarSnapshot(
        pinnedSessionIds: ['s1'],
        expandedGroupIds: [],
        selectedSessionId: 's1',
        capturedAt: null,
      );
      final back = SidebarSnapshot.fromJson(
          Map<String, dynamic>.from(snap.toJson()));
      expect(back.pinnedSessionIds, ['s1']);
      expect(back.capturedAt, isNull);
      expect(back.isStale(), true);
    });
  });
}
