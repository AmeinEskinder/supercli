/// Tests for `session_artifacts.dart` — port of `SessionArtifactsTests.swift`.
library;

import 'dart:io';

import 'package:supercli_app/screens/session_artifacts.dart';
import 'package:test/test.dart';

void main() {
  group('SessionArtifactStore (SessionArtifacts.swift)', () {
    late Directory tmp;

    setUp(() {
      tmp = Directory.systemTemp.createTempSync('artifacts_test');
    });

    tearDown(() {
      tmp.deleteSync(recursive: true);
    });

    test('listedKinds has the four gallery kinds', () {
      expect(SessionArtifactStore.listedKinds,
          ['screenshots', 'downloads', 'uploads', 'computer']);
    });

    test('captureKinds excludes user-added kinds', () {
      expect(SessionArtifactStore.captureKinds, ['screenshots', 'computer']);
    });

    test('kindDir maps kinds to on-disk dirs', () {
      final base = tmp.path;
      expect(
          SessionArtifactStore.kindDir(base, 's1', 'screenshots'),
          '$base/s1/artifacts/browser/screenshots');
      expect(SessionArtifactStore.kindDir(base, 's1', 'downloads'),
          '$base/s1/artifacts/browser/downloads');
      expect(SessionArtifactStore.kindDir(base, 's1', 'uploads'),
          '$base/s1/artifacts/uploads');
      expect(SessionArtifactStore.kindDir(base, 's1', 'computer'),
          '$base/s1/artifacts/computer/screenshots');
      expect(SessionArtifactStore.kindDir(base, 's1', 'bogus'), isNull);
    });

    test('thumbsDir is a sibling of kind dirs, not a listed kind', () {
      final dir = SessionArtifactStore.thumbsDir(tmp.path, 's1');
      expect(dir, '${tmp.path}/s1/artifacts/thumbs');
      expect(SessionArtifactStore.listedKinds, isNot(contains('thumbs')));
    });

    test('list returns artifacts newest-first across kinds', () {
      final upDir = Directory(
          SessionArtifactStore.kindDir(tmp.path, 's1', 'uploads')!);
      upDir.createSync(recursive: true);
      final old = File('${upDir.path}/old.png')..writeAsStringSync('a');
      // Ensure distinct mtimes.
      sleep(const Duration(milliseconds: 1100));
      final fresh = File('${upDir.path}/new.png')..writeAsStringSync('bb');

      final artifacts = SessionArtifactStore.list(tmp.path, 's1');
      expect(artifacts.length, 2);
      expect(artifacts[0].name, 'new.png');
      expect(artifacts[1].name, 'old.png');
      expect(artifacts[0].id, 'uploads/new.png');
      expect(artifacts[0].size, 2);
      expect(fresh.existsSync() && old.existsSync(), isTrue);
    });

    test('list skips hidden files', () {
      final upDir = Directory(
          SessionArtifactStore.kindDir(tmp.path, 's1', 'uploads')!);
      upDir.createSync(recursive: true);
      File('${upDir.path}/.hidden').writeAsStringSync('x');
      File('${upDir.path}/visible.png').writeAsStringSync('y');

      final artifacts = SessionArtifactStore.list(tmp.path, 's1');
      expect(artifacts.map((a) => a.name), ['visible.png']);
    });

    test('isImage matches image extensions', () {
      SessionArtifact mk(String name) => SessionArtifact(
            kind: 'uploads',
            name: name,
            path: '/x/$name',
            size: 1,
            modifiedAt: DateTime.now(),
          );
      expect(mk('a.png').isImage, isTrue);
      expect(mk('a.JPG').isImage, isTrue);
      expect(mk('a.webp').isImage, isTrue);
      expect(mk('a.txt').isImage, isFalse);
      expect(mk('a').isImage, isFalse);
    });

    test('delete removes file and is idempotent', () {
      final upDir = Directory(
          SessionArtifactStore.kindDir(tmp.path, 's1', 'uploads')!);
      upDir.createSync(recursive: true);
      final file = File('${upDir.path}/gone.png')
        ..writeAsStringSync('data');

      SessionArtifactStore.delete(tmp.path, 's1', 'uploads', 'gone.png');
      expect(file.existsSync(), isFalse);
      // Second delete is a no-op success.
      SessionArtifactStore.delete(tmp.path, 's1', 'uploads', 'gone.png');
    });

    test('delete with unknown kind is a no-op', () {
      SessionArtifactStore.delete(tmp.path, 's1', 'bogus', 'x.png');
    });

    test('latestCaptureUnixMs returns 0 when no captures', () {
      expect(SessionArtifactStore.latestCaptureUnixMs(tmp.path, 's1'), 0);
    });

    test('latestCaptureUnixMs finds newest capture mtime', () {
      final dir = Directory(
          SessionArtifactStore.kindDir(tmp.path, 's1', 'screenshots')!);
      dir.createSync(recursive: true);
      File('${dir.path}/cap.png').writeAsStringSync('x');
      final latest =
          SessionArtifactStore.latestCaptureUnixMs(tmp.path, 's1');
      expect(latest, greaterThan(0));
    });
  });
}
