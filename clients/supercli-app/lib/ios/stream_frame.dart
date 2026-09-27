/// Terminal-output stream frame reconciliation.
///
/// Port of `StreamFrameReconciler.swift` (`clients/legacy/ios/SupercliIOS`).
/// Decides what to do with an incoming terminal-output frame given the byte
/// offset the client has already consumed (`held`). Extracted as a pure
/// decision so the streaming resync logic is deterministically testable —
/// the source of the "disconnected, can't reconnect" loop was the WS handler
/// treating *every* offset mismatch as a full teardown+reconnect.
library;

/// What to do with an incoming terminal-output frame.
sealed class StreamFrameAction {
  const StreamFrameAction._();
}

/// Frame continues exactly where we are — feed the whole payload.
final class StreamFrameFeed extends StreamFrameAction {
  const StreamFrameFeed() : super._();

  @override
  bool operator ==(Object other) => other is StreamFrameFeed;

  @override
  int get hashCode => 0;

  @override
  String toString() => 'StreamFrameAction.feed';
}

/// Frame starts before `held` but extends past it (overlap on resume):
/// feed only the new tail, dropping the first [dropLeading] bytes.
final class StreamFrameFeedSuffix extends StreamFrameAction {
  const StreamFrameFeedSuffix(this.dropLeading) : super._();
  final int dropLeading;

  @override
  bool operator ==(Object other) =>
      other is StreamFrameFeedSuffix && other.dropLeading == dropLeading;

  @override
  int get hashCode => dropLeading.hashCode;

  @override
  String toString() =>
      'StreamFrameAction.feedSuffix(dropLeading: $dropLeading)';
}

/// Frame is entirely bytes we already have (a stale/duplicate frame after
/// a reconnect): drop it, no restart.
final class StreamFrameSkip extends StreamFrameAction {
  const StreamFrameSkip() : super._();

  @override
  bool operator ==(Object other) => other is StreamFrameSkip;

  @override
  int get hashCode => 1;

  @override
  String toString() => 'StreamFrameAction.skip';
}

/// Frame is *ahead* of us — we fell behind the live broadcaster and
/// missed `[from, upTo)`. Fetch that gap out-of-band, feed it, then feed
/// this frame — instead of tearing the whole connection down.
final class StreamFrameFillGap extends StreamFrameAction {
  const StreamFrameFillGap(this.from, this.upTo) : super._();
  final int from;
  final int upTo;

  @override
  bool operator ==(Object other) =>
      other is StreamFrameFillGap && other.from == from && other.upTo == upTo;

  @override
  int get hashCode => Object.hash(from, upTo);

  @override
  String toString() => 'StreamFrameAction.fillGap(from: $from, upTo: $upTo)';
}

/// Decide how to reconcile a frame at [frameOffset] (length [frameLength])
/// against the consumed offset [held]. Never returns "restart": a forward
/// gap is filled, a stale frame is skipped, an overlap is trimmed — the
/// live connection stays up.
StreamFrameAction reconcileStreamFrameAction({
  required int held,
  required int frameOffset,
  required int frameLength,
}) {
  if (frameOffset == held) return const StreamFrameFeed();

  if (frameOffset > held) {
    // We're behind: bytes [held, frameOffset) were skipped by the
    // live stream. Fill them before feeding this frame.
    return StreamFrameFillGap(held, frameOffset);
  }

  // frameOffset < held — the frame starts in bytes we already have.
  final frameEnd = frameOffset + (frameLength < 0 ? 0 : frameLength);
  if (frameEnd <= held) {
    // Entirely old — a duplicate/replayed frame. Drop it.
    return const StreamFrameSkip();
  }
  // Straddles `held`: keep only the part we haven't seen.
  return StreamFrameFeedSuffix(held - frameOffset);
}
