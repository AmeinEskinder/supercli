/// Tests for the stream frame reconciler (port of StreamFrameReconciler.swift).
library;

import 'package:supercli_app/ios/stream_frame.dart';
import 'package:test/test.dart';

StreamFrameAction action({
  required int held,
  required int frameOffset,
  required int frameLength,
}) => reconcileStreamFrameAction(
  held: held,
  frameOffset: frameOffset,
  frameLength: frameLength,
);

void main() {
  group('StreamFrameReconciler.action', () {
    test('contiguous frame feeds', () {
      expect(
        action(held: 1000, frameOffset: 1000, frameLength: 50),
        const StreamFrameFeed(),
      );
      expect(
        action(held: 0, frameOffset: 0, frameLength: 1),
        const StreamFrameFeed(),
      );
    });

    test('forward gaps fill instead of restart', () {
      // Exact (held, frame) pairs pulled from the disconnect trace.
      const cases = [
        (20441339, 20445456),
        (20481496, 20484469),
        (20486528, 20487252),
        (20487810, 20490434),
        (20500391, 20502861),
        (20505283, 20505591),
      ];
      for (final (held, frame) in cases) {
        expect(
          action(held: held, frameOffset: frame, frameLength: 200),
          StreamFrameFillGap(held, frame),
          reason: 'held=$held frame=$frame must fill the gap, not restart',
        );
      }
    });

    test('stale duplicate frame is skipped', () {
      // Frame entirely below held (a replayed frame after reconnect).
      expect(
        action(held: 2000, frameOffset: 1500, frameLength: 300),
        const StreamFrameSkip(),
      ); // 1500+300=1800 <= 2000
      expect(
        action(held: 2000, frameOffset: 2000 - 10, frameLength: 10),
        const StreamFrameSkip(),
      ); // ends exactly at held
    });

    test('overlapping frame feeds only new tail', () {
      // Frame starts before held but runs past it: keep the new bytes only.
      expect(
        action(held: 2000, frameOffset: 1950, frameLength: 100),
        const StreamFrameFeedSuffix(50),
      ); // covers 1950..2050
      expect(
        action(held: 500, frameOffset: 400, frameLength: 250),
        const StreamFrameFeedSuffix(100),
      ); // covers 400..650
    });

    test('zero-length frame below held is skipped', () {
      expect(
        action(held: 1000, frameOffset: 900, frameLength: 0),
        const StreamFrameSkip(),
      );
    });

    test('fill gap bounds are exact', () {
      final result = action(held: 100, frameOffset: 4200, frameLength: 32);
      expect(result, isA<StreamFrameFillGap>());
      final gap = result as StreamFrameFillGap;
      expect(gap.from, 100);
      expect(gap.upTo, 4200);
      expect(gap.upTo - gap.from, 4100); // the exact byte count to fetch
    });

    test('never returns a restart decision', () {
      // Every offset relationship maps to feed / feedSuffix / skip / fillGap.
      for (final held in [0, 500, 1000]) {
        for (final offset in [0, 499, 500, 501, 1500]) {
          final result = action(
            held: held,
            frameOffset: offset,
            frameLength: 64,
          );
          expect(
            result is StreamFrameFeed ||
                result is StreamFrameFeedSuffix ||
                result is StreamFrameSkip ||
                result is StreamFrameFillGap,
            isTrue,
            reason: 'held=$held offset=$offset',
          );
        }
      }
    });
  });
}
