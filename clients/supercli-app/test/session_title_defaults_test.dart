/// Tests for `session_title_defaults.dart` — port of `SessionTitleDefaultsTests.swift`.
library;

import 'package:supercli_app/session_title_defaults.dart';
import 'package:test/test.dart';

void main() {
  test('blank terminal uses abbreviated working folder', () {
    expect(
      SessionTitleDefaults.abbreviatedPath(
        '/Users/test/Dev/supercli',
        home: '/Users/test',
      ),
      '~/Dev/supercli',
    );
  });

  test('agent command remains the initial label', () {
    expect(
      SessionTitleDefaults.initialLabel(command: 'claude', cwd: '/tmp/project'),
      'claude',
    );
  });

  test('missing working folder keeps compatibility fallback', () {
    expect(
      SessionTitleDefaults.initialLabel(command: '', cwd: ''),
      'Terminal',
    );
  });

  test('home prefix must end at a path boundary', () {
    expect(
      SessionTitleDefaults.abbreviatedPath(
        '/Users/testing/project',
        home: '/Users/test',
      ),
      '/Users/testing/project',
    );
  });
}
