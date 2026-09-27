/// Terminal query-request stripping filter.
///
/// Port of `TerminalQueryFilter.swift` (`clients/legacy/ios/SupercliIOS`).
/// Strips terminal *query request* sequences from remote output before it is
/// fed to the local render surface.
///
/// The local surface must never answer terminal queries embedded in the
/// remote output: the real terminal already answered them, and the surface's
/// reply is routed upstream as spurious *input*. Queries carry no display
/// content, so removing them is invisible.
///
/// The filter is **stateful across chunks** (one instance per renderer): a
/// query split across a chunk boundary would otherwise pass through in two
/// innocent-looking halves, reassemble inside the surface's parser, and get
/// answered. An incomplete trailing sequence that could still become a query
/// is withheld and prepended to the next chunk; this is display-equivalent,
/// since the surface's own parser would buffer the same bytes without
/// rendering anything. A withheld run that outgrows any real query is dropped
/// rather than emitted; an unterminated DCS query flips a discard flag
/// instead of buffering, so its payload never accumulates.
///
/// Stripped: CSI DA (`…c`), DSR (`…n`), XTVERSION (`CSI > … q`), DECRQM
/// (`CSI … $ p`), and DCS XTGETTCAP/DECRQSS requests (`ESC P +q…` / `$q…`).
/// `ESC c` (RIS) and DECSCUSR (`CSI … SP q`) are deliberately preserved.
///
/// Dart is single-threaded: unlike the Swift original (which guards state
/// with an `NSLock` because ghostty may call from its IO thread), no locking
/// is needed here. One instance per renderer, called only from the UI isolate.
library;

/// A withheld (unterminated) CSI prefix longer than this is not a real
/// query — drop it entirely rather than emitting a reassembly hazard.
const int _maximumCarryBytes = 96;

const int _esc = 0x1B;
const int _bel = 0x07;

/// Result of scanning a DCS payload for its terminator.
enum _DcsScan {
  /// Index just past BEL / ESC \.
  terminated,

  /// Chunk ends with a lone ESC (maybe a split ST).
  trailingEsc,

  /// No terminator in this chunk.
  exhausted,
}

/// Stateful filter; one instance per renderer.
final class TerminalQueryFilter {
  List<int> _carry = <int>[];

  /// Inside an unterminated DCS query: discard bytes until ST/BEL.
  bool _discardingDcsQuery = false;

  /// Clears carried state. Call wherever the byte stream restarts from
  /// scratch (reset/clear replays).
  void reset() {
    _carry = <int>[];
    _discardingDcsQuery = false;
  }

  List<int> stripRequests(List<int> input) {
    if (_carry.isEmpty && !_discardingDcsQuery && !input.contains(_esc)) {
      return List<int>.of(input);
    }
    final bytes = <int>[..._carry, ...input];
    _carry = <int>[];
    final n = bytes.length;
    final out = <int>[];
    var i = 0;

    if (_discardingDcsQuery) {
      final scan = _scanDcsTerminator(bytes, 0);
      if (scan._kind == _DcsScan.terminated) {
        _discardingDcsQuery = false;
        i = scan.end;
      } else if (scan._kind == _DcsScan.trailingEsc) {
        _carry = [_esc]; // split ST — hold the ESC for the next chunk
        return <int>[];
      } else {
        return <int>[]; // whole chunk is query payload
      }
    }

    while (i < n) {
      final b = bytes[i];
      if (b != _esc) {
        out.add(b);
        i += 1;
        continue;
      }
      if (i + 1 >= n) {
        _carry = [_esc]; // lone trailing ESC — may begin a query
        break;
      }
      final next = bytes[i + 1];
      if (next == 0x5B) {
        // CSI: ESC [
        var j = i + 2;
        int? priv;
        if (j < n && bytes[j] >= 0x3C && bytes[j] <= 0x3F) {
          priv = bytes[j];
          j += 1;
        }
        var hasDollar = false;
        while (j < n && !(bytes[j] >= 0x40 && bytes[j] <= 0x7E)) {
          if (bytes[j] == 0x24) hasDollar = true;
          j += 1;
        }
        if (j >= n) {
          // No final byte yet: withhold so a split query cannot
          // reassemble in the surface. Oversized ⇒ not a query; drop.
          if (n - i <= _maximumCarryBytes) {
            _carry = bytes.sublist(i, n);
          }
          break;
        }
        final finalByte = bytes[j];
        final bool strip;
        switch (finalByte) {
          case 0x63: // 'c' — Device Attributes
            strip = true;
          case 0x6E: // 'n' — Device Status Report
            strip = true;
          case 0x71: // '>…q' — XTVERSION (not DECSCUSR)
            strip = priv == 0x3E;
          case 0x70: // '…$p' — DECRQM (not '!p' DECSTR)
            strip = hasDollar;
          default:
            strip = false;
        }
        if (strip) {
          i = j + 1;
        } else {
          out.addAll(bytes.sublist(i, j + 1));
          i = j + 1;
        }
      } else if (next == 0x50) {
        // DCS: ESC P
        if (i + 3 >= n) {
          // Too short to classify as '+q'/'$q' — withhold the tail.
          _carry = bytes.sublist(i, n);
          break;
        }
        final d0 = bytes[i + 2], d1 = bytes[i + 3];
        final isQuery = (d0 == 0x2B || d0 == 0x24) && d1 == 0x71; // '+q' / '$q'
        final scan = _scanDcsTerminator(bytes, i + 2);
        if (scan._kind == _DcsScan.terminated) {
          if (isQuery) {
            i = scan.end;
          } else {
            out.addAll(bytes.sublist(i, scan.end));
            i = scan.end;
          }
        } else if (scan._kind == _DcsScan.trailingEsc) {
          if (isQuery) {
            _discardingDcsQuery = true;
            _carry = [_esc]; // split ST — hold the ESC for the next chunk
          } else {
            out.addAll(bytes.sublist(i, n));
          }
          i = n;
        } else {
          if (isQuery) {
            _discardingDcsQuery = true; // swallow payload chunk-by-chunk
          } else {
            out.addAll(bytes.sublist(i, n));
          }
          i = n;
        }
      } else {
        out.add(b);
        i += 1; // ESC + other (e.g. RIS 'ESC c') — preserve
      }
    }
    return out;
  }

  _DcsScanResult _scanDcsTerminator(List<int> bytes, int start) {
    var j = start;
    final n = bytes.length;
    while (j < n) {
      if (bytes[j] == _bel) return _DcsScanResult(_DcsScan.terminated, j + 1);
      if (bytes[j] == _esc) {
        if (j + 1 >= n) return _DcsScanResult(_DcsScan.trailingEsc, j);
        if (bytes[j + 1] == 0x5C) {
          return _DcsScanResult(_DcsScan.terminated, j + 2); // ESC \
        }
      }
      j += 1;
    }
    return _DcsScanResult(_DcsScan.exhausted, n);
  }
}

final class _DcsScanResult {
  const _DcsScanResult(this._kind, this.end);
  final _DcsScan _kind;
  final int end;
}
