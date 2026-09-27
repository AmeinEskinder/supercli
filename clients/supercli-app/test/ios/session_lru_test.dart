/// Tests for the session LRU index and lease trackers (port of the
/// UI-agnostic structs in TerminalSessionCache.swift).
library;

import 'package:supercli_app/ios/session_lru.dart';
import 'package:test/test.dart';

void main() {
  group('SessionLruIndex', () {
    test('lookup marks most recently used', () {
      final index = SessionLruIndex<String>(capacity: 3);
      index.insert('a', id: 'a');
      index.insert('b', id: 'b');
      index.insert('c', id: 'c');
      expect(index.lookup('a'), 'a');
      expect(index.keys, ['b', 'c', 'a']);
    });

    test('lookup of missing id returns null', () {
      final index = SessionLruIndex<String>(capacity: 3);
      expect(index.lookup('nope'), isNull);
    });

    test('peek does not touch recency', () {
      final index = SessionLruIndex<String>(capacity: 3);
      index.insert('a', id: 'a');
      index.insert('b', id: 'b');
      expect(index.peek('a'), 'a');
      expect(index.keys, ['a', 'b']);
    });

    test('insert evicts least recently used beyond capacity', () {
      final index = SessionLruIndex<String>(capacity: 2);
      index.insert('a', id: 'a');
      index.insert('b', id: 'b');
      final evicted = index.insert('c', id: 'c');
      expect(evicted.length, 1);
      expect(evicted.single.id, 'a');
      expect(evicted.single.entry, 'a');
      expect(index.keys, ['b', 'c']);
    });

    test('insert never evicts the just-inserted entry', () {
      final index = SessionLruIndex<String>(capacity: 1);
      index.insert('a', id: 'a');
      final evicted = index.insert('b', id: 'b');
      expect(evicted.single.id, 'a');
      expect(index.lookup('b'), 'b');
    });

    test('replacing an entry keeps capacity', () {
      final index = SessionLruIndex<String>(capacity: 2);
      index.insert('a1', id: 'a');
      index.insert('b', id: 'b');
      final evicted = index.insert('a2', id: 'a');
      expect(evicted, isEmpty);
      expect(index.lookup('a'), 'a2');
      expect(index.keys, ['b', 'a']);
    });

    test('retainOnly spares visible sessions', () {
      final index = SessionLruIndex<String>(capacity: 5);
      index.insert('a', id: 'a');
      index.insert('b', id: 'b');
      index.insert('c', id: 'c');
      // 'b' is on screen but transiently missing from the session list.
      final removed = index.retainOnly({'a'}, keeping: 'b');
      expect(removed.map((e) => e.id), ['c']);
      expect(index.keys, ['a', 'b']);
    });

    test('removeAllExcept keeps only the visible session', () {
      final index = SessionLruIndex<String>(capacity: 5);
      index.insert('a', id: 'a');
      index.insert('b', id: 'b');
      index.insert('c', id: 'c');
      final removed = index.removeAllExcept('b');
      expect(removed.map((e) => e.id).toSet(), {'a', 'c'});
      expect(index.keys, ['b']);
    });

    test('remove returns the entry', () {
      final index = SessionLruIndex<String>(capacity: 3);
      index.insert('a', id: 'a');
      expect(index.remove('a'), 'a');
      expect(index.remove('a'), isNull);
      expect(index.count, 0);
    });

    test('capacity floors at 1', () {
      final index = SessionLruIndex<String>(capacity: 0);
      index.insert('a', id: 'a');
      index.insert('b', id: 'b');
      expect(index.keys, ['b']);
    });
  });

  group('TerminalVisibilityLeaseTracker', () {
    test('release requires the owning token', () {
      final tracker = TerminalVisibilityLeaseTracker();
      final ownerA = Object();
      final ownerB = Object();
      tracker.acquire(sessionID: 's1', owner: ownerA);
      expect(tracker.sessionID, 's1');
      // Wrong owner cannot release.
      expect(tracker.release(sessionID: 's1', owner: ownerB), isFalse);
      expect(tracker.sessionID, 's1');
      // Wrong session cannot release.
      expect(tracker.release(sessionID: 's2', owner: ownerA), isFalse);
      // Rightful owner releases.
      expect(tracker.release(sessionID: 's1', owner: ownerA), isTrue);
      expect(tracker.sessionID, isNull);
    });

    test('remount acquires over the old owner', () {
      final tracker = TerminalVisibilityLeaseTracker();
      final old = Object();
      final replacement = Object();
      tracker.acquire(sessionID: 's1', owner: old);
      tracker.acquire(sessionID: 's1', owner: replacement);
      // The old disappear must not hide the replacement.
      expect(tracker.release(sessionID: 's1', owner: old), isFalse);
      expect(tracker.sessionID, 's1');
      expect(tracker.release(sessionID: 's1', owner: replacement), isTrue);
    });
  });

  group('TerminalStreamLeaseTracker', () {
    test('acquire is idempotent per owner', () {
      final tracker = TerminalStreamLeaseTracker();
      final owner = Object();
      expect(tracker.acquire(owner), isTrue); // transition: start renderer
      expect(tracker.acquire(owner), isFalse); // already held
      expect(tracker.isEmpty, isFalse);
    });

    test('late disappear from old mount cannot stop the replacement', () {
      final tracker = TerminalStreamLeaseTracker();
      final old = Object();
      final replacement = Object();
      expect(tracker.acquire(old), isTrue);
      expect(tracker.acquire(replacement), isFalse); // already streaming
      // Old mount disappears: not the last owner, keep streaming.
      expect(tracker.release(old), isFalse);
      expect(tracker.isEmpty, isFalse);
      // Replacement disappears: last owner, stop.
      expect(tracker.release(replacement), isTrue);
      expect(tracker.isEmpty, isTrue);
    });

    test('releasing an unknown owner returns false', () {
      final tracker = TerminalStreamLeaseTracker();
      expect(tracker.release(Object()), isFalse);
    });

    test('removeAll clears owners', () {
      final tracker = TerminalStreamLeaseTracker();
      tracker.acquire(Object());
      tracker.removeAll();
      expect(tracker.isEmpty, isTrue);
    });
  });
}
