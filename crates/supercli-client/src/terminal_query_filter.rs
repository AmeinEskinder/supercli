//! Port of `TerminalQueryFilter.swift` (iOS, Foundation-only).
//!
//! Strips terminal *query request* sequences from remote output before it is
//! fed to the phone's local ghostty surface.
//!
//! The filter is **stateful across chunks** (one instance per renderer): a
//! query split across a chunk boundary would otherwise pass through in two
//! innocent-looking halves, reassemble inside the surface's parser, and get
//! answered. An incomplete trailing sequence that could still become a query
//! is withheld and prepended to the next chunk. A withheld run that outgrows
//! any real query is dropped rather than emitted; an unterminated DCS query
//! flips a discard flag instead of buffering, so its payload never
//! accumulates.
//!
//! Stripped: CSI DA (`…c`), DSR (`…n`), XTVERSION (`CSI > … q`), DECRQM
//! (`CSI … $ p`), and DCS XTGETTCAP/DECRQSS requests (`ESC P +q…` / `$q…`).
//! `ESC c` (RIS) and DECSCUSR (`CSI … SP q`) are deliberately preserved.

use std::sync::Mutex;

/// Stateful terminal query-request filter. `Send` via an internal mutex
/// (mirrors the Swift `NSLock` + `@unchecked Sendable`).
pub struct TerminalQueryFilter {
    state: Mutex<FilterState>,
}

#[derive(Default)]
struct FilterState {
    carry: Vec<u8>,
    /// Inside an unterminated DCS query: discard bytes until ST/BEL.
    discarding_dcs_query: bool,
}

impl TerminalQueryFilter {
    /// A withheld (unterminated) CSI prefix longer than this is not a real
    /// query — drop it entirely rather than emitting a reassembly hazard.
    const MAXIMUM_CARRY_BYTES: usize = 96;

    pub fn new() -> Self {
        Self {
            state: Mutex::new(FilterState::default()),
        }
    }

    /// Clears carried state. Call wherever the byte stream restarts from
    /// scratch (reset/clear replays).
    pub fn reset(&self) {
        let mut s = self.state.lock().unwrap();
        s.carry.clear();
        s.discarding_dcs_query = false;
    }

    /// Strip query requests from `input`, returning the display bytes.
    pub fn strip_requests(&self, input: &[u8]) -> Vec<u8> {
        let mut s = self.state.lock().unwrap();
        if s.carry.is_empty() && !s.discarding_dcs_query && !input.contains(&0x1B) {
            return input.to_vec();
        }
        let mut bytes = std::mem::take(&mut s.carry);
        bytes.extend_from_slice(input);
        let n = bytes.len();
        let mut out: Vec<u8> = Vec::with_capacity(n);
        let mut i = 0;
        if s.discarding_dcs_query {
            match scan_dcs_terminator(&bytes, 0) {
                DcsScan::Terminated(end) => {
                    s.discarding_dcs_query = false;
                    i = end;
                }
                DcsScan::TrailingEsc => {
                    s.carry = vec![0x1B]; // split ST — hold the ESC for the next chunk
                    return Vec::new();
                }
                DcsScan::Exhausted => return Vec::new(), // whole chunk is query payload
            }
        }
        while i < n {
            let b = bytes[i];
            if b != 0x1B {
                out.push(b);
                i += 1;
                continue;
            }
            if i + 1 >= n {
                s.carry = vec![0x1B]; // lone trailing ESC — may begin a query
                break;
            }
            let next = bytes[i + 1];
            if next == 0x5B {
                // CSI: ESC [
                let mut j = i + 2;
                let mut priv_byte: Option<u8> = None;
                if j < n && (0x3C..=0x3F).contains(&bytes[j]) {
                    priv_byte = Some(bytes[j]);
                    j += 1;
                }
                let mut has_dollar = false;
                while j < n && !(0x40..=0x7E).contains(&bytes[j]) {
                    if bytes[j] == 0x24 {
                        has_dollar = true;
                    }
                    j += 1;
                }
                if j >= n {
                    // No final byte yet: withhold so a split query cannot
                    // reassemble in the surface. Oversized ⇒ not a query; drop.
                    if n - i <= Self::MAXIMUM_CARRY_BYTES {
                        s.carry = bytes[i..n].to_vec();
                    }
                    break;
                }
                let final_byte = bytes[j];
                let strip = match final_byte {
                    0x63 => true,                    // 'c' — Device Attributes
                    0x6E => true,                    // 'n' — Device Status Report
                    0x71 => priv_byte == Some(0x3E), // '>…q' — XTVERSION (not DECSCUSR)
                    0x70 => has_dollar,              // '…$p' — DECRQM (not '!p' DECSTR)
                    _ => false,
                };
                // Either way the sequence is consumed; stripped ones vanish.
                if !strip {
                    out.extend_from_slice(&bytes[i..=j]);
                }
                i = j + 1;
            } else if next == 0x50 {
                // DCS: ESC P
                if i + 3 >= n {
                    // Too short to classify as '+q'/'$q' — withhold the tail.
                    s.carry = bytes[i..n].to_vec();
                    break;
                }
                let d0 = bytes[i + 2];
                let d1 = bytes[i + 3];
                let is_query = (d0 == 0x2B || d0 == 0x24) && d1 == 0x71; // '+q' / '$q'
                match scan_dcs_terminator(&bytes, i + 2) {
                    DcsScan::Terminated(end) => {
                        if is_query {
                            i = end;
                        } else {
                            out.extend_from_slice(&bytes[i..end]);
                            i = end;
                        }
                    }
                    DcsScan::TrailingEsc => {
                        if is_query {
                            s.discarding_dcs_query = true;
                            s.carry = vec![0x1B]; // split ST — hold the ESC
                        } else {
                            out.extend_from_slice(&bytes[i..n]);
                        }
                        i = n;
                    }
                    DcsScan::Exhausted => {
                        if is_query {
                            s.discarding_dcs_query = true; // swallow payload chunk-by-chunk
                        } else {
                            out.extend_from_slice(&bytes[i..n]);
                        }
                        i = n;
                    }
                }
            } else {
                out.push(b);
                i += 1; // ESC + other (e.g. RIS 'ESC c') — preserve
            }
        }
        out
    }
}

impl Default for TerminalQueryFilter {
    fn default() -> Self {
        Self::new()
    }
}

enum DcsScan {
    Terminated(usize), // index just past BEL / ESC \
    TrailingEsc,       // chunk ends with a lone ESC (maybe split ST)
    Exhausted,         // no terminator in this chunk
}

fn scan_dcs_terminator(bytes: &[u8], start: usize) -> DcsScan {
    let n = bytes.len();
    let mut j = start;
    while j < n {
        if bytes[j] == 0x07 {
            return DcsScan::Terminated(j + 1); // BEL
        }
        if bytes[j] == 0x1B {
            if j + 1 >= n {
                return DcsScan::TrailingEsc;
            }
            if bytes[j + 1] == 0x5C {
                return DcsScan::Terminated(j + 2); // ESC \
            }
        }
        j += 1;
    }
    DcsScan::Exhausted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> TerminalQueryFilter {
        TerminalQueryFilter::new()
    }

    #[test]
    fn plain_text_without_esc_passes_through() {
        assert_eq!(f().strip_requests(b"hello world"), b"hello world");
    }

    #[test]
    fn csi_device_attributes_stripped() {
        // ESC [ c and ESC [ 0 c
        assert_eq!(f().strip_requests(b"a\x1b[c"), b"a");
        assert_eq!(f().strip_requests(b"\x1b[0cb"), b"b");
    }

    #[test]
    fn csi_dsr_stripped() {
        assert_eq!(f().strip_requests(b"\x1b[6nX"), b"X");
    }

    #[test]
    fn xtversion_stripped_but_decscusr_preserved() {
        // CSI > q is XTVERSION (strip); CSI SP q is DECSCUSR (keep).
        assert_eq!(f().strip_requests(b"\x1b[>0q!"), b"!");
        assert_eq!(f().strip_requests(b"\x1b[0 q;"), b"\x1b[0 q;");
    }

    #[test]
    fn decrqm_stripped_but_decstr_preserved() {
        // CSI $ p is DECRQM (strip); CSI ! p is DECSTR (keep).
        assert_eq!(f().strip_requests(b"\x1b[$p."), b".");
        assert_eq!(f().strip_requests(b"\x1b[!p."), b"\x1b[!p.");
    }

    #[test]
    fn ris_preserved() {
        assert_eq!(f().strip_requests(b"\x1bc"), b"\x1bc");
    }

    #[test]
    fn sgr_colors_preserved() {
        assert_eq!(
            f().strip_requests(b"\x1b[31mred\x1b[0m"),
            b"\x1b[31mred\x1b[0m"
        );
    }

    #[test]
    fn dcs_xtgettcap_stripped() {
        // ESC P + q … BEL
        assert_eq!(f().strip_requests(b"a\x1bP+q1234\x07b"), b"ab");
    }

    #[test]
    fn dcs_decrqss_stripped() {
        // ESC P $ q … ESC \
        assert_eq!(f().strip_requests(b"\x1bP$qABC\x1b\\z"), b"z");
    }

    #[test]
    fn non_query_dcs_preserved() {
        // ESC P 1 $ r … BEL is not +q/$q — keep it.
        assert_eq!(f().strip_requests(b"\x1bP1$r0\x07"), b"\x1bP1$r0\x07");
    }

    #[test]
    fn split_csi_query_across_chunks_withheld() {
        let filter = f();
        // First chunk ends mid-query: nothing emitted yet.
        assert_eq!(filter.strip_requests(b"a\x1b["), b"a");
        // Second chunk completes the DA query: stripped.
        assert_eq!(filter.strip_requests(b"c"), b"");
    }

    #[test]
    fn split_dcs_query_discarded_across_chunks() {
        let filter = f();
        assert_eq!(filter.strip_requests(b"\x1bP+q"), b"");
        // Payload chunk swallowed while discarding.
        assert_eq!(filter.strip_requests(b"payload-bytes"), b"");
        // Terminator ends the discard; trailing text passes.
        assert_eq!(filter.strip_requests(b"\x07ok"), b"ok");
    }

    #[test]
    fn oversized_withheld_prefix_dropped() {
        let filter = f();
        // 200 parameter bytes (';') with no final byte: unterminated CSI
        // prefix exceeds the 96-byte carry cap → dropped, not withheld.
        let mut chunk = vec![0x1B, 0x5B];
        chunk.extend(std::iter::repeat_n(b';', 200));
        assert_eq!(filter.strip_requests(&chunk), b"");
        // Filter still usable afterwards.
        assert_eq!(filter.strip_requests(b"ok"), b"ok");
    }

    #[test]
    fn reset_clears_carry_and_discard_state() {
        let filter = f();
        assert_eq!(filter.strip_requests(b"\x1bP+q"), b"");
        filter.reset();
        // After reset the previously-discarding state is gone.
        assert_eq!(filter.strip_requests(b"plain"), b"plain");
    }

    #[test]
    fn lone_trailing_esc_withheld_then_completed() {
        let filter = f();
        assert_eq!(filter.strip_requests(b"x\x1b"), b"x");
        // Next chunk: ESC c is RIS — preserved, not a query.
        assert_eq!(filter.strip_requests(b"c"), b"\x1bc");
    }
}
