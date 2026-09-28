/// Tests for [paneWorkingDirectory].
///
/// Port of the `paneWorkingDirectory` cases from
/// `PaneWorkingDirectoryTests.swift` in
/// `native/SupercliNative/Tests/SupercliNativeTests`.
/// The `ClickablePath.resolveFile` cases are covered by the Rust port
/// (`crates/supercli-core/src/clickable_path.rs`); the JSON `cwd` decoding
/// cases belong to the DTO/store workers.
library;

import 'package:test/test.dart';

import '../lib/terminal/pane_working_directory.dart';

void main() {
  group('paneWorkingDirectory', () {
    test('session cwd wins over project path', () {
      expect(
        paneWorkingDirectory(
          sessionCwd: '/Users/me/Dev/flatsome',
          projectPath: '/Users/me/Dev/somewhere-else',
        ),
        '/Users/me/Dev/flatsome',
      );
    });

    test('absent cwd falls back to project path', () {
      expect(
        paneWorkingDirectory(
          sessionCwd: null,
          projectPath: '/Users/me/Dev/flatsome',
        ),
        '/Users/me/Dev/flatsome',
      );
    });

    test('blank cwd from a Host is no cwd at all', () {
      expect(
        paneWorkingDirectory(
          sessionCwd: '  ',
          projectPath: '/Users/me/Dev/flatsome',
        ),
        '/Users/me/Dev/flatsome',
      );
      expect(
        paneWorkingDirectory(sessionCwd: '\n\t ', projectPath: '/x'),
        '/x',
      );
    });

    test('nil when both are absent', () {
      expect(
        paneWorkingDirectory(sessionCwd: null, projectPath: null),
        isNull,
      );
    });
  });
}
