/// URL sanitization for terminal link handling.
///
/// Port of `GhosttyTerminalPane.sanitizedURL(from:)` from
/// `native/SupercliNative/Sources/SupercliNative/GhosttyBridge.swift`.
/// The `ClickablePath` cases from `TerminalLinkRegressionTests.swift` are
/// covered by the Rust port (`crates/supercli-core/src/clickable_path.rs`).
library;

/// Sanitizes a raw string into a URL, or returns `null` if it cannot be
/// made into a safe, openable URL.
///
/// - Drops internal whitespace/newlines (wrapped links pick these up from
///   the terminal grid).
/// - Peels matched wrapping brackets/quotes, then trailing punctuation.
/// - Adds a scheme for bare hosts (`localhost` → http, dotted → https).
/// - Only allows http/https/mailto/tel/ftp/ftps schemes.
/// - Requires a non-empty host for http/https/ftp/ftps.
Uri? sanitizedUrl(String raw) {
  // A URL never legitimately contains whitespace.
  var s = raw.replaceAll(RegExp(r'\s+'), '');
  if (s.isEmpty) return null;

  const wrappers = [
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('<', '>'),
    ('"', '"'),
    ("'", "'"),
    ('`', '`'),
  ];
  while (s.isNotEmpty) {
    final first = s[0];
    final last = s[s.length - 1];
    if (s.length >= 2 &&
        wrappers.any((w) => first == w.$1 && last == w.$2)) {
      // A URL itself never starts with a wrapping delimiter.
      s = s.substring(1, s.length - 1);
    } else if ('.,;"\'>'.contains(last)) {
      s = s.substring(0, s.length - 1);
    } else {
      final unmatched = wrappers.where((w) => w.$2 == last).firstOrNull;
      if (unmatched != null) {
        final openCount = s.split(unmatched.$1).length - 1;
        final closeCount = s.split(unmatched.$2).length - 1;
        if (closeCount > openCount) {
          s = s.substring(0, s.length - 1);
          continue;
        }
      }
      break;
    }
  }
  if (s.isEmpty) return null;

  // Add a scheme for bare hosts so `www.example.com` / `example.com/x`
  // still open in the browser instead of being treated as a file path.
  final lower = s.toLowerCase();
  if (!s.contains('://') &&
      !lower.startsWith('mailto:') &&
      !lower.startsWith('tel:')) {
    final host = s.split('/').first;
    if (host == 'localhost' || host.startsWith('localhost:')) {
      s = 'http://$s';
    } else if (host.contains('.')) {
      s = 'https://$s';
    }
  }

  // Build the URI, percent-encoding leftover illegal characters if the
  // strict parse fails.
  Uri? parsed;
  try {
    parsed = Uri.parse(s);
    // Uri.parse is lenient; verify it round-trips the essential parts.
    if (parsed.scheme.isEmpty && !s.contains(':')) parsed = null;
  } catch (_) {
    parsed = null;
  }
  parsed ??= _tryPercentEncoded(s);
  if (parsed == null) return null;

  final scheme = parsed.scheme.toLowerCase();
  const allowed = {'http', 'https', 'mailto', 'tel', 'ftp', 'ftps'};
  if (!allowed.contains(scheme)) return null;
  if ({'http', 'https', 'ftp', 'ftps'}.contains(scheme)) {
    if (parsed.host.isEmpty) return null;
  }
  return parsed;
}

Uri? _tryPercentEncoded(String s) {
  try {
    final encoded = Uri.encodeFull(s);
    final parsed = Uri.parse(encoded);
    return parsed.scheme.isEmpty ? null : parsed;
  } catch (_) {
    return null;
  }
}
