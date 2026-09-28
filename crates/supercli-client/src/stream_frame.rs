//! Terminal-output stream frame reconciliation.
//!
//! Port of `StreamFrameReconciler.swift` (`ios/SupercliIOS`). Decides what to
//! do with an incoming terminal-output frame given the byte offset the client
//! has already consumed (`held`). Extracted as a pure decision so the
//! streaming resync logic is deterministically testable — the source of the
//! "disconnected, can't reconnect" loop was the WS handler treating *every*
//! offset mismatch as a full teardown+reconnect.

/// What to do with an incoming terminal-output frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFrameAction {
    /// Frame continues exactly where we are — feed the whole payload.
    Feed,
    /// Frame starts before `held` but extends past it (overlap on resume):
    /// feed only the new tail, dropping the first `drop_leading` bytes.
    FeedSuffix { drop_leading: usize },
    /// Frame is entirely bytes we already have (a stale/duplicate frame after
    /// a reconnect): drop it, no restart.
    Skip,
    /// Frame is *ahead* of us — we fell behind the live broadcaster and
    /// missed `[from, up_to)`. Fetch that gap out-of-band, feed it, then feed
    /// this frame — instead of tearing the whole connection down.
    FillGap { from: u64, up_to: u64 },
}

/// Decide how to reconcile a frame at `frame_offset` (length `frame_length`)
/// against the consumed offset `held`. Never returns "restart": a forward
/// gap is filled, a stale frame is skipped, an overlap is trimmed — the
/// live connection stays up.
pub fn reconcile_action(held: u64, frame_offset: u64, frame_length: usize) -> StreamFrameAction {
    if frame_offset == held {
        return StreamFrameAction::Feed;
    }

    if frame_offset > held {
        // We're behind: bytes [held, frame_offset) were skipped by the
        // live stream. Fill them before feeding this frame.
        return StreamFrameAction::FillGap {
            from: held,
            up_to: frame_offset,
        };
    }

    // frame_offset < held — the frame starts in bytes we already have.
    let frame_end = frame_offset.saturating_add(frame_length as u64);
    if frame_end <= held {
        // Entirely old — a duplicate/replayed frame. Drop it.
        return StreamFrameAction::Skip;
    }
    // Straddles `held`: keep only the part we haven't seen.
    StreamFrameAction::FeedSuffix {
        drop_leading: (held - frame_offset) as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(held: u64, frame_offset: u64, len: usize) -> StreamFrameAction {
        reconcile_action(held, frame_offset, len)
    }

    #[test]
    fn contiguous_frame_feeds() {
        assert_eq!(action(1000, 1000, 50), StreamFrameAction::Feed);
        assert_eq!(action(0, 0, 1), StreamFrameAction::Feed);
    }

    #[test]
    fn forward_gaps_fill_instead_of_restart() {
        // Exact (held, frame) pairs pulled from the disconnect trace.
        let cases: [(u64, u64); 6] = [
            (20441339, 20445456),
            (20481496, 20484469),
            (20486528, 20487252),
            (20487810, 20490434),
            (20500391, 20502861),
            (20505283, 20505591),
        ];
        for (held, frame) in cases {
            assert_eq!(
                action(held, frame, 200),
                StreamFrameAction::FillGap {
                    from: held,
                    up_to: frame
                },
                "held={held} frame={frame} must fill the gap, not restart"
            );
        }
    }

    #[test]
    fn stale_duplicate_frame_is_skipped() {
        // Frame entirely below held (a replayed frame after reconnect).
        assert_eq!(action(2000, 1500, 300), StreamFrameAction::Skip); // 1500+300=1800 <= 2000
        assert_eq!(action(2000, 2000 - 10, 10), StreamFrameAction::Skip); // ends exactly at held
    }

    #[test]
    fn overlapping_frame_feeds_only_new_tail() {
        // Frame starts before held but runs past it: keep the new bytes only.
        assert_eq!(
            action(2000, 1950, 100),
            StreamFrameAction::FeedSuffix { drop_leading: 50 } // covers 1950..2050
        );
        assert_eq!(
            action(500, 400, 250),
            StreamFrameAction::FeedSuffix { drop_leading: 100 } // covers 400..650
        );
    }

    #[test]
    fn zero_length_frame_below_held_is_skipped() {
        assert_eq!(action(1000, 900, 0), StreamFrameAction::Skip);
    }

    #[test]
    fn fill_gap_bounds_are_exact() {
        match action(100, 4200, 32) {
            StreamFrameAction::FillGap { from, up_to } => {
                assert_eq!(from, 100);
                assert_eq!(up_to, 4200);
                assert_eq!(up_to - from, 4100); // the exact byte count to fetch
            }
            other => panic!("expected fillGap, got {other:?}"),
        }
    }
}
