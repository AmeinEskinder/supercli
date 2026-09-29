//! Port of `StreamFrameReconciler.swift` (iOS, Foundation-only).
//!
//! Decides what to do with an incoming terminal-output frame given the byte
//! offset the client has already consumed (`held`). Extracted as a pure
//! decision so the streaming resync logic is deterministically testable —
//! the source of the "disconnected, can't reconnect" loop was the WS handler
//! treating *every* offset mismatch as a full teardown+reconnect.

/// What to do with an incoming terminal-output frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFrameAction {
    /// Frame continues exactly where we are — feed the whole payload.
    Feed,
    /// Frame starts before `held` but extends past it (overlap on resume):
    /// feed only the new tail, dropping the first `drop_leading` bytes.
    FeedSuffix { drop_leading: usize },
    /// Frame is entirely bytes we already have (a stale/duplicate frame
    /// after a reconnect): drop it, no restart.
    Skip,
    /// Frame is *ahead* of us — we fell behind the live broadcaster and
    /// missed `[from, up_to)`. Fetch that gap out-of-band, feed it, then feed
    /// this frame — instead of tearing the whole connection down.
    FillGap { from: u64, up_to: u64 },
}

/// Pure reconciler: decide how to reconcile a frame at `frame_offset`
/// (length `frame_length`) against the consumed offset `held`. Never returns
/// "restart": a forward gap is filled, a stale frame is skipped, an overlap
/// is trimmed — the live connection stays up.
pub fn action(held: u64, frame_offset: u64, frame_length: usize) -> StreamFrameAction {
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

    #[test]
    fn exact_continuation_feeds_whole_frame() {
        assert_eq!(action(100, 100, 50), StreamFrameAction::Feed);
    }

    #[test]
    fn zero_offsets_feed() {
        assert_eq!(action(0, 0, 10), StreamFrameAction::Feed);
    }

    #[test]
    fn ahead_frame_requests_gap_fill() {
        assert_eq!(
            action(100, 150, 50),
            StreamFrameAction::FillGap {
                from: 100,
                up_to: 150
            }
        );
    }

    #[test]
    fn fully_stale_frame_is_skipped() {
        // Frame [50, 90) entirely below held=100.
        assert_eq!(action(100, 50, 40), StreamFrameAction::Skip);
    }

    #[test]
    fn frame_ending_exactly_at_held_is_skipped() {
        // Frame [50, 100), held=100: nothing new.
        assert_eq!(action(100, 50, 50), StreamFrameAction::Skip);
    }

    #[test]
    fn overlapping_frame_feeds_suffix() {
        // Frame [80, 130), held=100: drop first 20 bytes.
        assert_eq!(
            action(100, 80, 50),
            StreamFrameAction::FeedSuffix { drop_leading: 20 }
        );
    }

    #[test]
    fn one_byte_overlap_feeds_rest() {
        // Frame [99, 149), held=100: drop 1 byte.
        assert_eq!(
            action(100, 99, 50),
            StreamFrameAction::FeedSuffix { drop_leading: 1 }
        );
    }

    #[test]
    fn empty_frame_at_held_feeds() {
        assert_eq!(action(100, 100, 0), StreamFrameAction::Feed);
    }

    #[test]
    fn empty_stale_frame_is_skipped() {
        assert_eq!(action(100, 50, 0), StreamFrameAction::Skip);
    }

    #[test]
    fn large_offsets_do_not_overflow() {
        let held = u64::MAX - 10;
        // frame_end saturates instead of wrapping.
        assert_eq!(
            action(held, held - 5, usize::MAX),
            StreamFrameAction::FeedSuffix { drop_leading: 5 }
        );
    }

    #[test]
    fn trace_forward_gaps_fill_instead_of_restart() {
        // Exact (held, frame) pairs pulled from the disconnect trace — every
        // one of these used to trigger a full reconnect (the churn loop).
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
