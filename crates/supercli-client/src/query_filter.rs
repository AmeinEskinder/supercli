//! Terminal query-request stripping filter.
//!
//! Port of `TerminalQueryFilter.swift` (`ios/SupercliIOS`). Strips terminal
//! *query request* sequences from remote output before it is fed to the local
//! render surface.
//!
//! The local surface must never answer terminal queries embedded in the
//! remote output: the real terminal already answered them, and the surface's
//! reply is routed upstream as spurious *input* (e.g. Grok's XTVERSION query
//! yields `>|ghostty 1.3.1` typed into its prompt, re-emitted on every
//! focus-driven replay). Queries carry no display content, so removing them
//! is invisible.
//!
//! The filter is **stateful across chunks**: a query split across a chunk
//! boundary would otherwise pass through in two innocent-looking halves,
//! reassemble inside the surface's parser, and get answered. An incomplete
//! trailing sequence that could still become a query is withheld and
//! prepended to the next chunk. A withheld run that outgrows any real query
//! is dropped rather than emitted; an unterminated DCS query flips a discard
//! flag instead of buffering, so its payload never accumulates.
//!
//! Stripped: CSI DA (`…c`), DSR (`…n`), XTVERSION (`CSI > … q`), DECRQM
//! (`CSI … $ p`), and DCS XTGETTCAP/DECRQSS requests (`ESC P +q…` / `$q…`).
//! `ESC c` (RIS) and DECSCUSR (`CSI … SP q`) are deliberately preserved.

/// A withheld (unterminated) CSI prefix longer than this is not a real
/// query — drop it entirely rather than emitting a reassembly hazard.
const MAXIMUM_CARRY_BYTES: usize = 96;

const ESC: u8 = 0x1B;
const BEL: u8 = 0x07;

/// Stateful filter; one instance per renderer.
#[derive(Debug, Default)]
pub struct TerminalQueryFilter {
    carry: Vec<u8>,
    /// Inside an unterminated DCS query: discard bytes until ST/BEL.
    discarding_dcs_query: bool,
}

#[derive(Debug)]
enum DcsScan {
    /// Index just past BEL / ESC \.
    Terminated(usize),
    /// Chunk ends with a lone ESC (maybe a split ST).
    TrailingEsc,
    /// No terminator in this chunk.
    Exhausted,
}

impl TerminalQueryFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears carried state. Call wherever the byte stream restarts from
    /// scratch (reset/clear replays).
    pub fn reset(&mut self) {
        self.carry.clear();
        self.discarding_dcs_query = false;
    }

    pub fn strip_requests(&mut self, input: &[u8]) -> Vec<u8> {
        if self.carry.is_empty() && !self.discarding_dcs_query && !input.contains(&ESC) {
            return input.to_vec();
        }
        let mut bytes = std::mem::take(&mut self.carry);
        bytes.extend_from_slice(input);
        let n = bytes.len();
        let mut out: Vec<u8> = Vec::with_capacity(n);
        let mut i = 0;

        if self.discarding_dcs_query {
            match Self::scan_dcs_terminator(&bytes, 0) {
                DcsScan::Terminated(end) => {
                    self.discarding_dcs_query = false;
                    i = end;
                }
                DcsScan::TrailingEsc => {
                    self.carry = vec![ESC]; // split ST — hold the ESC for the next chunk
                    return Vec::new();
                }
                DcsScan::Exhausted => {
                    return Vec::new(); // whole chunk is query payload
                }
            }
        }

        while i < n {
            let b = bytes[i];
            if b != ESC {
                out.push(b);
                i += 1;
                continue;
            }
            if i + 1 >= n {
                self.carry = vec![ESC]; // lone trailing ESC — may begin a query
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
                    if n - i <= MAXIMUM_CARRY_BYTES {
                        self.carry = bytes[i..n].to_vec();
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
                if strip {
                    i = j + 1;
                } else {
                    out.extend_from_slice(&bytes[i..=j]);
                    i = j + 1;
                }
            } else if next == 0x50 {
                // DCS: ESC P
                if i + 3 >= n {
                    // Too short to classify as '+q'/'$q' — withhold the tail.
                    self.carry = bytes[i..n].to_vec();
                    break;
                }
                let d0 = bytes[i + 2];
                let d1 = bytes[i + 3];
                let is_query = (d0 == 0x2B || d0 == 0x24) && d1 == 0x71; // '+q' / '$q'
                match Self::scan_dcs_terminator(&bytes, i + 2) {
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
                            self.discarding_dcs_query = true;
                            self.carry = vec![ESC]; // split ST — hold the ESC for the next chunk
                        } else {
                            out.extend_from_slice(&bytes[i..n]);
                        }
                        i = n;
                    }
                    DcsScan::Exhausted => {
                        if is_query {
                            self.discarding_dcs_query = true; // swallow payload chunk-by-chunk
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

    fn scan_dcs_terminator(bytes: &[u8], from: usize) -> DcsScan {
        let n = bytes.len();
        let mut j = from;
        while j < n {
            if bytes[j] == BEL {
                return DcsScan::Terminated(j + 1);
            }
            if bytes[j] == ESC {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(s: &str) -> String {
        String::from_utf8(TerminalQueryFilter::new().strip_requests(s.as_bytes()))
            .expect("valid utf8")
    }

    /// Feeds each piece as its own chunk through one filter instance and
    /// returns the concatenated output — the split-across-chunks scenario.
    fn strip_chunks(chunks: &[&str]) -> String {
        let mut filter = TerminalQueryFilter::new();
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend_from_slice(&filter.strip_requests(chunk.as_bytes()));
        }
        String::from_utf8(out).expect("valid utf8")
    }

    #[test]
    fn strips_xtversion_request() {
        // The observed bug: XTVERSION query (CSI > q) makes the local surface
        // reply `>|ghostty 1.3.1` into the app's input. It must be removed.
        assert_eq!(strip("A\x1b[>qB"), "AB");
        assert_eq!(strip("A\x1b[>0qB"), "AB");
    }

    #[test]
    fn strips_device_attributes_and_status_requests() {
        assert_eq!(strip("x\x1b[cy"), "xy"); // primary DA
        assert_eq!(strip("x\x1b[0cy"), "xy"); // primary DA, explicit 0
        assert_eq!(strip("x\x1b[>cy"), "xy"); // secondary DA
        assert_eq!(strip("x\x1b[6ny"), "xy"); // DSR cursor position
        assert_eq!(strip("x\x1b[?6ny"), "xy"); // DSR, private
    }

    #[test]
    fn strips_decrqm_but_not_decstr() {
        assert_eq!(strip("a\x1b[?2026$pb"), "ab"); // DECRQM query
                                                   // DECSTR (soft reset, CSI ! p) has no '$' — must be preserved.
        assert_eq!(strip("a\x1b[!pb"), "a\x1b[!pb");
    }

    #[test]
    fn strips_dcs_queries_xtgettcap_and_decrqss() {
        assert_eq!(strip("a\x1bP+q544e\x1b\\b"), "ab"); // XTGETTCAP
        assert_eq!(strip("a\x1bP$qm\x1b\\b"), "ab"); // DECRQSS
    }

    #[test]
    fn preserves_ris_and_decscusr_and_ordinary_sequences() {
        // ESC c (RIS reset) — 'c' after a bare ESC, not a CSI: keep.
        assert_eq!(strip("a\x1bcb"), "a\x1bcb");
        // DECSCUSR: CSI Ps SP q (set cursor style) — keep (only '>…q' is XTVERSION).
        assert_eq!(strip("a\x1b[2 qb"), "a\x1b[2 qb");
        // Colour + cursor move must be untouched.
        assert_eq!(
            strip("\x1b[31mhi\x1b[0m\x1b[2J"),
            "\x1b[31mhi\x1b[0m\x1b[2J"
        );
    }

    #[test]
    fn strips_query_split_across_chunks() {
        // The leak: a query cut at the chunk boundary passed through in two
        // innocent halves, reassembled inside the surface's parser, and was
        // answered as typed input.
        assert_eq!(strip_chunks(&["ok\x1b[>0", "qdone"]), "okdone");
        // The observed DECRQM 2026 probe shape, split mid-parameter.
        assert_eq!(strip_chunks(&["a\x1b[?20", "26$pb"]), "ab");
        // Split immediately after the ESC.
        assert_eq!(strip_chunks(&["x\x1b", "[6ny"]), "xy");
    }

    #[test]
    fn emits_non_query_split_sequence_intact() {
        // A withheld trailing fragment that turns out to be an ordinary
        // sequence must come out whole, in order, on the next chunk.
        assert_eq!(strip_chunks(&["\x1b[3", "1mred"]), "\x1b[31mred");
        assert_eq!(strip_chunks(&["a\x1b", "cb"]), "a\x1bcb"); // RIS survives a split
    }

    #[test]
    fn strips_dcs_query_split_across_chunks() {
        // XTGETTCAP split three ways, including a split ST (ESC in one
        // chunk, backslash in the next).
        assert_eq!(strip_chunks(&["a\x1bP+q54", "4e\x1b", "\\b"]), "ab");
        // Classification split: only 'ESC P' at the boundary.
        assert_eq!(strip_chunks(&["a\x1bP", "$qm\x1b\\b"]), "ab");
        // Long unterminated query payload is discarded chunk-by-chunk, then
        // output resumes after the terminator.
        let long_payload = "5".repeat(4096);
        assert_eq!(strip_chunks(&["a\x1bP+q", &long_payload, "\x1b\\b"]), "ab");
    }

    #[test]
    fn oversized_incomplete_csi_is_dropped_not_emitted() {
        // A withheld run longer than any real query is not a query — drop it
        // rather than emit a reassembly hazard.
        let junk = "\x1b[".to_string() + &"1;".repeat(128);
        assert_eq!(strip_chunks(&["ok", &junk, "later"]), "oklater");
    }

    #[test]
    fn reset_clears_carried_state() {
        let mut filter = TerminalQueryFilter::new();
        let _ = filter.strip_requests("a\x1b[>0".as_bytes()); // withholds the tail
        filter.reset();
        assert_eq!(
            String::from_utf8(filter.strip_requests("fresh".as_bytes())).unwrap(),
            "fresh"
        );
    }

    #[test]
    fn no_escape_is_unchanged() {
        assert_eq!(strip("plain text 123"), "plain text 123");
    }
}
