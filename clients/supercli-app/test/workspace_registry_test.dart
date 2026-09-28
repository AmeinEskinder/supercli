/// Tests for `workspace_registry.dart` — port of `UnpeelWorkspaceRegistry.swift`.
library;

import 'package:supercli_app/screens/workspace_registry.dart';
import 'package:test/test.dart';

/// In-memory fake for [WorkspaceRegistryIo].
class FakeIo implements WorkspaceRegistryIo {
  List<int>? registryData;
  final Set<String> dirs = {};
  final Set<String> deleted = {};
  int _uuidCounter = 0;

  @override
  List<int>? readRegistry() => registryData;

  @override
  void writeRegistry(List<int> data) {
    registryData = data;
  }

  @override
  void createDirectory(String path) {
    dirs.add(path);
  }

  @override
  bool pathExists(String path) => dirs.contains(path);

  @override
  void deleteDirectory(String path) {
    deleted.add(path);
    dirs.remove(path);
  }

  @override
  int nowMs() => 1700000000000;

  @override
  String newUuid() => 'uuid-${_uuidCounter++}';
}

void main() {
  group('SupercliWorkspaceRegistry (UnpeelWorkspaceRegistry.swift)', () {
    test('slugify matches Swift', () {
      expect(SupercliWorkspaceRegistry.slugify('My Workspace'), 'my-workspace');
      expect(SupercliWorkspaceRegistry.slugify('  Leading'), 'leading');
      expect(SupercliWorkspaceRegistry.slugify('Trailing  '), 'trailing');
      expect(SupercliWorkspaceRegistry.slugify('a--b'), 'a-b');
      expect(SupercliWorkspaceRegistry.slugify(''), 'workspace');
      expect(SupercliWorkspaceRegistry.slugify('---'), 'workspace');
      expect(SupercliWorkspaceRegistry.slugify('Café'), 'caf');
    });

    test('encode/decode round-trips with "profiles" key', () {
      final records = [
        const SupercliWorkspaceRecord(
            id: 'a', name: 'One', home: '/h/one', createdAt: 1),
        const SupercliWorkspaceRecord(
            id: 'b', name: 'Two', home: '/h/two', createdAt: 2),
      ];
      final data = SupercliWorkspaceRegistry.encodeRegistry(records);
      // Released contract: the JSON key is "profiles".
      expect(String.fromCharCodes(data), contains('"profiles"'));
      final decoded = SupercliWorkspaceRegistry.decodeRegistry(data);
      expect(decoded, records);
    });

    test('load returns empty on missing/corrupt', () {
      final io = FakeIo();
      expect(SupercliWorkspaceRegistry.load(io), isEmpty);
      io.registryData = [1, 2, 3];
      expect(SupercliWorkspaceRegistry.load(io), isEmpty);
    });

    test('create mints record with unique slug', () {
      final io = FakeIo();
      final r1 = SupercliWorkspaceRegistry.create(io, '/real', 'My WS');
      expect(r1.name, 'My WS');
      expect(r1.home, '/real/profiles/my-ws');
      expect(io.dirs, contains('/real/profiles/my-ws'));

      final r2 = SupercliWorkspaceRegistry.create(io, '/real', 'My WS');
      expect(r2.home, '/real/profiles/my-ws-2');
    });

    test('create rejects empty name', () {
      final io = FakeIo();
      expect(() => SupercliWorkspaceRegistry.create(io, '/real', '  '),
          throwsA(isA<SupercliWorkspaceError>()));
    });

    test('rename updates name, ignores unknown/empty', () {
      final io = FakeIo();
      final r = SupercliWorkspaceRegistry.create(io, '/real', 'Old');
      SupercliWorkspaceRegistry.rename(io, r.id, 'New');
      expect(SupercliWorkspaceRegistry.load(io).single.name, 'New');
      SupercliWorkspaceRegistry.rename(io, 'nope', 'X');
      SupercliWorkspaceRegistry.rename(io, r.id, '   ');
      expect(SupercliWorkspaceRegistry.load(io).single.name, 'New');
    });

    test('remove deletes home only under managed root', () {
      final io = FakeIo();
      final r = SupercliWorkspaceRegistry.create(io, '/real', 'Gone');
      SupercliWorkspaceRegistry.remove(io, '/real', r.id,
          deleteData: true);
      expect(SupercliWorkspaceRegistry.load(io), isEmpty);
      expect(io.deleted, contains(r.home));
    });

    test('remove without deleteData keeps home', () {
      final io = FakeIo();
      final r = SupercliWorkspaceRegistry.create(io, '/real', 'Kept');
      SupercliWorkspaceRegistry.remove(io, '/real', r.id,
          deleteData: false);
      expect(io.deleted, isEmpty);
    });
  });

  group('WorkspaceListOrder', () {
    test('apply sorts by saved position, unsaved keep build order', () {
      final rows = ['a', 'b', 'c', 'd'];
      final ordered = WorkspaceListOrder.apply(
          rows, ['c', 'a'], (s) => s);
      expect(ordered, ['c', 'a', 'b', 'd']);
    });

    test('key prefixes never collide', () {
      final local =
          WorkspaceListOrder.localKey(home: '/h/x');
      final paired = WorkspaceListOrder.pairedKey(hostId: 'abc');
      final ssh = WorkspaceListOrder.sshKey(id: 'abc');
      expect({local, paired, ssh}.length, 3);
      expect(local.startsWith('local:'), isTrue);
      expect(paired.startsWith('host:'), isTrue);
      expect(ssh.startsWith('ssh:'), isTrue);
    });
  });
}
