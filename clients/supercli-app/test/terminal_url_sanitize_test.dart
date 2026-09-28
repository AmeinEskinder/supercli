/// Tests for [sanitizedUrl].
///
/// Port of the `webLinksKeepSignificantPunctuation` and
/// `webLinksRequireAHost` cases from `TerminalLinkRegressionTests.swift` in
/// `native/SupercliNative/Tests/SupercliNativeTests`. The `ClickablePath`
/// cases from that file are covered by the Rust port
/// (`crates/supercli-core/src/clickable_path.rs`).
library;

import 'package:test/test.dart';

import '../lib/terminal/terminal_url_sanitize.dart';

void main() {
  group('sanitizedUrl', () {
    test('web links keep significant punctuation', () {
      final cases = {
        'https://en.wikipedia.org/wiki/Function_(mathematics)':
            'https://en.wikipedia.org/wiki/Function_(mathematics)',
        '(https://example.com/a_(b)).': 'https://example.com/a_(b)',
        '`<https://example.com>`': 'https://example.com',
        'https://example.com/search?q=why?':
            'https://example.com/search?q=why?',
        'https://example.com/hello!': 'https://example.com/hello!',
        'http://localhost:3000/path': 'http://localhost:3000/path',
        'localhost:3000/path': 'http://localhost:3000/path',
        'https://example.com/\n  a?x=1&y=2':
            'https://example.com/a?x=1&y=2',
      };
      for (final entry in cases.entries) {
        expect(
          sanitizedUrl(entry.key)?.toString(),
          entry.value,
          reason: 'input: ${entry.key}',
        );
      }
    });

    test('web links require a host', () {
      expect(sanitizedUrl('https://'), isNull);
      expect(sanitizedUrl('http:///report'), isNull);
    });

    test('disallowed schemes are rejected', () {
      expect(sanitizedUrl('javascript:alert(1)'), isNull);
      expect(sanitizedUrl('file:///tmp/notes.md'), isNull);
    });

    test('mailto links pass through', () {
      expect(
        sanitizedUrl('mailto:person@example.com')?.toString(),
        'mailto:person@example.com',
      );
    });
  });
}
