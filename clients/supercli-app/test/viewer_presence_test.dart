// Tests for [ViewerPresenceStore] — port of `ViewerPresenceTests.swift`
// (`clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ViewerPresenceTests.swift`).
//
// All 7 XCTest cases are ported 1:1. The Swift `@MainActor` isolation is
// dropped (the Dart port is a synchronous decision kernel); the
// DispatchSource directory watcher and 5s fallback Timer are not ported
// (GAP-VIEWERPRESENCE-1), so tests drive [ViewerPresenceStore.refresh]
// explicitly with a fixed `now`.
import 'dart:convert';
import 'dart:io';

import 'package:supercli_app/viewer_presence.dart';
import 'package:test/test.dart';

void main() {
  group('ViewerPresenceStore', () {
    test(
      'one device across transports uses its newest name and lease',
      () async {
        final files = await PresenceFiles.create();
        addTearDown(files.dispose);
        final now = DateTime.now();
        await files.write([
          files.entry(
            'Old name (phone-a)',
            now.subtract(const Duration(seconds: 5)),
          ),
          files.entry('Renamed phone (phone-a)', now),
        ]);
        await files.write([
          files.entry(
            'Phone (phone-a)',
            now.subtract(const Duration(seconds: 2)),
          ),
        ], mobile: true);
        final store = files.makeStore();
        store.refresh(now: now);

        final viewers = store.viewers['s1'];
        expect(viewers, isNotNull);
        expect(viewers!.length, 1);
        expect(viewers.first.id, 'device:phone-a');
        expect(viewers.first.displayName, 'Renamed phone');
        expect(store.isDeviceViewing('s1', 'phone-a'), isTrue);
      },
    );

    test(
      'matching names and addresses do not merge different devices',
      () async {
        final files = await PresenceFiles.create();
        addTearDown(files.dispose);
        final now = DateTime.now();
        await files.write([
          files.entry('Mac (mac-a)', now, ip: '192.0.2.1'),
          files.entry('Mac (mac-b)', now, ip: '192.0.2.1'),
          files.entry(null, now, ip: '192.0.2.1'),
        ]);
        final store = files.makeStore();
        expect(store.viewers['s1']?.length, 3);
        expect(store.viewers['s1']?.where((v) => v.deviceID == null).length, 1);
        expect(store.isDeviceViewing('s1', '192.0.2.1'), isFalse);
        expect(store.hasViewers('another-session'), isFalse);
      },
    );

    test('each transport expires before devices are merged', () async {
      final files = await PresenceFiles.create();
      addTearDown(files.dispose);
      final now = DateTime.now();
      await files.write([files.entry('Stream (device-a)', now)]);
      await files.write([
        files.entry('Direct (device-a)', now.add(const Duration(seconds: 1))),
      ], mobile: true);
      final store = files.makeStore();
      store.refresh(now: now.add(const Duration(seconds: 17)));
      expect(store.viewers['s1']?.first.displayName, 'Stream');
      store.refresh(now: now.add(const Duration(seconds: 21)));
      expect(store.hasViewers('s1'), isFalse);
    });

    test(
      'grid repair waits for last viewer and explicit fit then runs once',
      () async {
        final files = await PresenceFiles.create();
        addTearDown(files.dispose);
        final now = DateTime.now();
        await files.write([files.entry('Mac (mac-a)', now)]);
        await files.write([files.entry('iPad (ipad-a)', now)], mobile: true);
        final store = files.makeStore();
        expect(
          store.consumeGridReassertCandidate('s1', hasActiveFit: false),
          isFalse,
        );

        await files.write([]);
        store.refresh(now: now);
        expect(store.hasViewers('s1'), isTrue);
        expect(
          store.consumeGridReassertCandidate('s1', hasActiveFit: false),
          isFalse,
        );

        store.refresh(now: now.add(const Duration(seconds: 16)));
        expect(store.hasViewers('s1'), isFalse);
        expect(
          store.consumeGridReassertCandidate('s1', hasActiveFit: true),
          isFalse,
        );
        expect(
          store.consumeGridReassertCandidate('s1', hasActiveFit: false),
          isTrue,
        );
        expect(
          store.consumeGridReassertCandidate('s1', hasActiveFit: false),
          isFalse,
        );
        expect(
          store.consumeGridReassertCandidate(
            'never-viewed',
            hasActiveFit: false,
          ),
          isFalse,
        );
      },
    );

    test(
      'transport and session changes do not announce another connection',
      () async {
        final files = await PresenceFiles.create();
        addTearDown(files.dispose);
        final now = DateTime.now();
        final arrivals = <String>[];
        final store = files.makeStore(onConnection: arrivals.add);
        await files.write([files.entry('iPhone (phone-a)', now)], mobile: true);
        store.refresh(now: now);
        expect(arrivals, ['iPhone']);

        await files.write([
          files.entry(
            'Renamed iPhone (phone-a)',
            now.add(const Duration(seconds: 1)),
          ),
        ], session: 's2');
        await files.write([], mobile: true);
        store.refresh(now: now.add(const Duration(seconds: 1)));
        expect(arrivals, ['iPhone']);
        expect(store.isDeviceViewing('s2', 'phone-a'), isTrue);

        store.refresh(now: now.add(const Duration(seconds: 22)));
        await files.write([
          files.entry(
            'Renamed iPhone (phone-a)',
            now.add(const Duration(seconds: 23)),
          ),
        ]);
        store.refresh(now: now.add(const Duration(seconds: 23)));
        expect(arrivals, ['iPhone', 'Renamed iPhone']);
      },
    );

    test('existing viewers are seeded without a connection toast', () async {
      final files = await PresenceFiles.create();
      addTearDown(files.dispose);
      await files.write([
        files.entry('iPad (ipad-a)', DateTime.now()),
      ], mobile: true);
      final arrivals = <String>[];
      final store = files.makeStore(onConnection: arrivals.add);
      expect(store.hasViewers('s1'), isTrue);
      expect(arrivals, isEmpty);
    });

    test('stores stay scoped to their own host files', () async {
      final first = await PresenceFiles.create();
      addTearDown(first.dispose);
      final second = await PresenceFiles.create();
      addTearDown(second.dispose);
      await first.write([
        first.entry('iPad (ipad-a)', DateTime.now()),
      ], mobile: true);
      final firstStore = first.makeStore();
      final secondStore = second.makeStore();
      expect(firstStore.hasViewers('s1'), isTrue);
      expect(secondStore.hasViewers('s1'), isFalse);
      await File('${first.directory.path}/mobile-presence.json')
          .writeAsString('malformed');
      firstStore.refresh();
      expect(firstStore.hasViewers('s1'), isFalse);
    });
  });

  group('device identity parsing', () {
    test('deviceIDFromDevice extracts the parenthesised id', () {
      expect(ViewerPresenceStore.deviceIDFromDevice('Mac (mac-a)'), 'mac-a');
      expect(ViewerPresenceStore.deviceIDFromDevice('Mac'), isNull);
      expect(ViewerPresenceStore.deviceIDFromDevice('Mac ()'), isNull);
      expect(ViewerPresenceStore.deviceIDFromDevice(null), isNull);
      expect(ViewerPresenceStore.deviceIDFromDevice('Mac (a) (b)'), 'b');
    });

    test('displayNameFromDevice strips the device id suffix', () {
      expect(
        ViewerPresenceStore.displayNameFromDevice('Mac (mac-a)', null),
        'Mac',
      );
      expect(
        ViewerPresenceStore.displayNameFromDevice('Mac', '1.2.3.4'),
        'Mac',
      );
      expect(
        ViewerPresenceStore.displayNameFromDevice(null, '1.2.3.4'),
        '1.2.3.4',
      );
      expect(
        ViewerPresenceStore.displayNameFromDevice(null, null),
        'Remote viewer',
      );
    });

    test('parsePresence tolerates malformed input', () {
      expect(
        ViewerPresenceStore.parsePresence(
          data: utf8.encode('not json'),
          source: 'terminal',
        ),
        isEmpty,
      );
      expect(
        ViewerPresenceStore.parsePresence(
          data: utf8.encode('{"version":1}'),
          source: 'terminal',
        ),
        isEmpty,
      );
    });
  });
}

/// Test helper mirroring the Swift `PresenceFiles` fixture: a temp directory
/// holding `presence.json` / `mobile-presence.json`.
class PresenceFiles {
  final Directory directory;

  PresenceFiles._(this.directory);

  static Future<PresenceFiles> create() async {
    final dir = await Directory.systemTemp.createTemp('presence-');
    return PresenceFiles._(dir);
  }

  Future<void> dispose() async {
    try {
      await directory.delete(recursive: true);
    } catch (_) {}
  }

  Map<String, dynamic> entry(String? device, DateTime at, {String? ip}) => {
    'last_seen': at.millisecondsSinceEpoch,
    'device': device,
    'ip': ip,
  };

  Future<void> write(
    List<Map<String, dynamic>> entries, {
    bool mobile = false,
    String session = 's1',
  }) async {
    final data = jsonEncode({
      'version': 1,
      'sessions': {session: entries},
    });
    await File(
      '${directory.path}/${mobile ? 'mobile-presence.json' : 'presence.json'}',
    ).writeAsString(data);
  }

  ViewerPresenceStore makeStore({void Function(String)? onConnection}) =>
      ViewerPresenceStore(
        presencePath: '${directory.path}/presence.json',
        automaticallyUpdates: false,
        onConnection: onConnection,
      );
}
