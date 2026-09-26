/// Live session terminal output for the attached pane.
///
/// Reads the Host's per-session output journal (`output.bin`) the same way
/// `supercli-attach` does: a snapshot tail for the initial paint, then
/// incremental tail reads for live following. See
/// `crates/supercli-attach/src/lib.rs` (`read_replay_tail`).
///
/// The journal holds raw VT bytes; [stripAnsi] reduces them to plain text
/// because the P0-8 [TerminalState] has no VT parser (`writeString` is the
/// plain-text entry point). The text is rendered through the P0-8
/// [TerminalPane] RLE fallback grid.
///
/// When the session dir is unavailable (remote session, Host on another
/// machine, journal not yet created) the reader returns null and the pane
/// falls back to its placeholder.
library;

import 'dart:convert';
import 'dart:io';

/// History replayed for the initial snapshot paint (matches the attach
/// client's "prefer the host's live stream, recover from the journal" split:
/// enough for several viewports, cheap to parse).
const int snapshotTailBytes = 32 * 1024;

/// Per-session raw terminal output journal (see session_host.rs).
const String outputBinName = 'output.bin';

/// Retention marker naming the earliest valid journal offset.
const String outputRetentionName = 'output-retention.json';

/// A read of session output: the raw journal bytes plus the journal offset
/// to continue tailing from.
final class SessionOutputRead {
  const SessionOutputRead(this.bytes, this.nextOffset);

  /// Raw VT bytes read.
  final List<int> bytes;

  /// Journal offset just past [bytes]; pass back for the next tail read.
  final int nextOffset;

  bool get isEmpty => bytes.isEmpty;
}

/// Resolves the Supercli state dir: `$SUPERCLI_HOME`, else `~/.supercli`.
/// Mirrors `supercli-core/src/app_paths.rs` `supercli_home()`.
String? supercliHome() {
  final override = Platform.environment['SUPERCLI_HOME'];
  if (override != null && override.isNotEmpty) return override;
  final home = Platform.environment['HOME'];
  if (home == null || home.isEmpty) return null;
  return '$home/.supercli';
}

/// Candidate output journal paths for [sessionId], in preference order.
/// Bootstrap/mobile sessions live under `sessions/`; app sessions under
/// `app-sessions/` (see `session_host.rs` `session_dir`).
List<String> candidateOutputPaths(String sessionId) {
  final home = supercliHome();
  if (home == null) return const [];
  return [
    '$home/sessions/$sessionId/$outputBinName',
    '$home/app-sessions/$sessionId/$outputBinName',
  ];
}

/// First existing journal path for [sessionId], or null when the session
/// has no local output journal.
String? resolveOutputPath(String sessionId) {
  for (final path in candidateOutputPaths(sessionId)) {
    if (File(path).existsSync()) return path;
  }
  return null;
}

/// Earliest valid offset for [outputBinPath], from its sibling
/// `output-retention.json`. Returns 0 when the marker is missing or invalid
/// (matches `supercli-attach`'s fallback).
int retainedFromFor(String outputBinPath) {
  try {
    final sep = outputBinPath.lastIndexOf(Platform.pathSeparator);
    final dir = sep < 0 ? '.' : outputBinPath.substring(0, sep);
    final marker = File('$dir/$outputRetentionName');
    if (!marker.existsSync()) return 0;
    final json = jsonDecode(marker.readAsStringSync());
    if (json is! Map) return 0;
    final value = (json['retained_from'] as num?)?.toInt() ?? 0;
    return value < 0 ? 0 : value;
  } catch (_) {
    return 0;
  }
}

/// Lookback for the replay-alignment scan (mirrors
/// `REPLAY_TAIL_ALIGNMENT_LOOKBACK_BYTES`).
const int _alignLookbackBytes = 16 * 1024;

/// Aligns [desiredStart] to a safe replay boundary: never inside an ANSI
/// escape sequence or a multi-byte UTF-8 character.
///
/// Faithful port of `supercli-attach`'s `align_tail_start_in_window`:
/// scans [window] (bytes at absolute offsets `[scanStart, desiredStart)`)
/// with the VT state machine and returns the last safe boundary at or
/// before [desiredStart].
int alignStart(List<int> window, int scanStart, int desiredStart) {
  var initialIndex = 0;
  while (initialIndex < window.length &&
      (window[initialIndex] & 0xC0) == 0x80) {
    initialIndex++;
  }

  var lastBoundary = scanStart + initialIndex;
  // 0=ground, 1=escape, 2=escapeIntermediate, 3=csi, 4=osc, 5=oscEscape,
  // 6=dcs, 7=dcsEscape, 8=sosPmApc, 9=sosPmApcEscape
  var state = 0;

  for (var index = initialIndex; index < window.length; index++) {
    final absolute = scanStart + index;
    if (absolute >= desiredStart) break;
    final b = window[index];
    if (b == 0x18 || b == 0x1A) {
      lastBoundary = absolute + 1;
      state = 0;
      continue;
    }
    if (b == 0x1B && (state == 1 || state == 2 || state == 3)) {
      lastBoundary = absolute;
      state = 1;
      continue;
    }
    switch (state) {
      case 0: // ground
        if (b == 0x1B) {
          lastBoundary = absolute;
          state = 1;
        } else if (b == 0x0A || b == 0x0D) {
          lastBoundary = absolute + 1;
        }
      case 1: // escape
        if (b == 0x5B) {
          state = 3;
        } else if (b == 0x5D) {
          state = 4;
        } else if (b == 0x50) {
          state = 6;
        } else if (b == 0x58 || b == 0x5E || b == 0x5F) {
          state = 8;
        } else if (b >= 0x20 && b <= 0x2F) {
          state = 2;
        } else {
          lastBoundary = absolute + 1;
          state = 0;
        }
      case 2: // escapeIntermediate
        if (b >= 0x30 && b <= 0x7E) {
          lastBoundary = absolute + 1;
          state = 0;
        }
      case 3: // csi
        if (b >= 0x40 && b <= 0x7E) {
          lastBoundary = absolute + 1;
          state = 0;
        }
      case 4: // osc
        if (b == 0x07) {
          lastBoundary = absolute + 1;
          state = 0;
        } else if (b == 0x1B) {
          state = 5;
        }
      case 5: // oscEscape
        if (b == 0x5C) {
          lastBoundary = absolute + 1;
          state = 0;
        } else {
          state = 4;
        }
      case 6: // dcs
        if (b == 0x1B) state = 7;
      case 7: // dcsEscape
        if (b == 0x5C) {
          lastBoundary = absolute + 1;
          state = 0;
        } else {
          state = 6;
        }
      case 8: // sosPmApc
        if (b == 0x1B) state = 9;
      case 9: // sosPmApcEscape
        if (b == 0x5C) {
          lastBoundary = absolute + 1;
          state = 0;
        } else {
          state = 8;
        }
    }
  }

  return lastBoundary < desiredStart ? lastBoundary : desiredStart;
}

/// Reads the snapshot tail for [sessionId]: the last [maxBytes] of the
/// output journal, clamped to the retained floor and aligned to a safe
/// boundary. Returns null when no local journal exists.
SessionOutputRead? readSnapshotTail(
  String sessionId, {
  int maxBytes = snapshotTailBytes,
}) {
  final path = resolveOutputPath(sessionId);
  if (path == null) return null;
  return readSnapshotTailAt(path, maxBytes: maxBytes);
}

/// Path-based core of [readSnapshotTail], testable without a session dir.
SessionOutputRead? readSnapshotTailAt(
  String outputBinPath, {
  int maxBytes = snapshotTailBytes,
}) {
  try {
    final file = File(outputBinPath);
    final length = file.lengthSync();
    if (length == 0) return const SessionOutputRead([], 0);
    final retainedFrom = retainedFromFor(outputBinPath).clamp(0, length);
    var desiredStart = length - maxBytes;
    if (desiredStart < retainedFrom) desiredStart = retainedFrom;
    if (desiredStart < 0) desiredStart = 0;
    // Alignment scan window [scanStart, desiredStart), mirroring
    // supercli-attach's 16 KiB lookback.
    var scanStart = desiredStart - _alignLookbackBytes;
    if (scanStart < retainedFrom) scanStart = retainedFrom;
    if (scanStart < 0) scanStart = 0;
    final all = file.readAsBytesSync();
    final start = alignStart(
      all.sublist(scanStart, desiredStart),
      scanStart,
      desiredStart,
    );
    final bytes = all.sublist(start);
    return SessionOutputRead(bytes, length);
  } catch (_) {
    return null;
  }
}

/// Reads journal bytes appended since [fromOffset] for [sessionId].
/// When the journal shrank below [fromOffset] (rotation/hole-punching),
/// falls back to a fresh snapshot. Returns null when no journal exists.
SessionOutputRead? readTail(String sessionId, int fromOffset) {
  final path = resolveOutputPath(sessionId);
  if (path == null) return null;
  return readTailAt(path, fromOffset);
}

/// Path-based core of [readTail], testable without a session dir.
SessionOutputRead? readTailAt(String outputBinPath, int fromOffset) {
  try {
    final file = File(outputBinPath);
    final length = file.lengthSync();
    if (length < fromOffset) {
      // Journal rotated or reclaimed: re-snapshot.
      return readSnapshotTailAt(outputBinPath);
    }
    if (length == fromOffset) {
      return SessionOutputRead(const [], length);
    }
    final all = file.readAsBytesSync();
    return SessionOutputRead(all.sublist(fromOffset), length);
  } catch (_) {
    return null;
  }
}

/// Strips ANSI/VT escape sequences from [input], returning plain text.
///
/// - CSI (`ESC [ ... final`), OSC (`ESC ] ... BEL`/`ESC \`), DCS/SOS and
///   single-character escapes are removed.
/// - `\r\n` and lone `\r` become `\n` (a progress-bar overwrite renders as
///   successive lines in the text fallback rather than garbage).
/// - Tabs expand to 8 spaces; other C0 controls are dropped.
/// - Invalid byte sequences are replaced (the journal is bytes; the pane
///   works in strings).
String stripAnsi(String input) {
  final buf = StringBuffer();
  var i = 0;
  while (i < input.length) {
    final c = input.codeUnitAt(i);
    if (c == 0x1B) {
      i = _skipEscapeCodeUnits(input, i);
    } else if (c == 0x07) {
      i++; // BEL
    } else if (c == 0x09) {
      buf.write('        ');
      i++;
    } else if (c < 0x20 && c != 0x0A) {
      // Other C0 controls: CRLF collapses to one LF; lone CR becomes LF
      // (a progress-bar overwrite renders as successive lines in the text
      // fallback rather than garbage); the rest are dropped.
      if (c == 0x0D) {
        buf.write('\n');
        if (i + 1 < input.length && input.codeUnitAt(i + 1) == 0x0A) i++;
      }
      i++;
    } else {
      buf.writeCharCode(c);
      i++;
    }
  }
  return buf.toString();
}

/// String-level version of [_skipEscape] (input[i] == ESC).
int _skipEscapeCodeUnits(String s, int i) {
  i++;
  if (i >= s.length) return i;
  final next = s.codeUnitAt(i);
  if (next == 0x5B) {
    i++;
    while (i < s.length) {
      final b = s.codeUnitAt(i);
      i++;
      if (b >= 0x40 && b <= 0x7E) break;
    }
  } else if (next == 0x5D || next == 0x50 || next == 0x58) {
    i++;
    while (i < s.length) {
      final b = s.codeUnitAt(i);
      if (b == 0x07) {
        i++;
        break;
      }
      if (b == 0x1B && i + 1 < s.length && s.codeUnitAt(i + 1) == 0x5C) {
        i += 2;
        break;
      }
      i++;
    }
  } else {
    i++;
  }
  return i;
}

/// Decodes journal [bytes] to stripped plain text for the pane.
String decodeOutputText(List<int> bytes) {
  // allowMalformed: the journal may split a multi-byte char across reads.
  return stripAnsi(utf8.decode(bytes, allowMalformed: true));
}
