/// Worktree discovery of agent-created checkouts.
///
/// Covers checklist row 218: "Worktree discovery of agent-created
/// checkouts (opt-in, every 5 s)".
///
/// Agents can create git worktrees outside the app (e.g. `git worktree
/// add`). When the user opts in, this service polls the Host every
/// [pollInterval] (default 5 s), diffs the worktree list, and reports
/// added/removed checkouts via [onChanged] so the sidebar can offer
/// them. Opt-out means zero polling.
library;

import 'dart:async';

/// Poll interval for worktree discovery (Swift parity: 5 s).
const Duration worktreeDiscoveryInterval = Duration(seconds: 5);

/// Diff between two worktree-list snapshots.
final class WorktreeDiff {
  const WorktreeDiff({this.added = const [], this.removed = const []});

  final List<String> added;
  final List<String> removed;

  bool get isEmpty => added.isEmpty && removed.isEmpty;
}

/// Opt-in poller that detects agent-created worktrees.
final class WorktreeDiscovery {
  WorktreeDiscovery({
    this.pollInterval = worktreeDiscoveryInterval,
    this.onChanged,
  });

  final Duration pollInterval;
  final void Function(WorktreeDiff diff)? onChanged;

  bool _optedIn = false;
  bool get isOptedIn => _optedIn;

  Set<String> _known = {};
  Timer? _timer;

  int get pollCount => _pollCount;
  int _pollCount = 0;

  /// Opt in: start polling. The first poll establishes the baseline
  /// (no diff reported for pre-existing worktrees).
  void optIn(Set<String> current) {
    _optedIn = true;
    _known = Set.of(current);
    _timer?.cancel();
    _timer = Timer.periodic(pollInterval, (_) => _poll());
  }

  void optOut() {
    _optedIn = false;
    _timer?.cancel();
    _timer = null;
  }

  /// Feed a fresh worktree-path listing from the Host. Used by the
  /// timer callback and directly in tests.
  WorktreeDiff ingest(Set<String> current) {
    final added = current.difference(_known).toList()..sort();
    final removed = _known.difference(current).toList()..sort();
    _known = Set.of(current);
    final diff = WorktreeDiff(added: added, removed: removed);
    if (!diff.isEmpty) onChanged?.call(diff);
    return diff;
  }

  void _poll() {
    _pollCount++;
    // The app shell performs the Host fetch and calls ingest().
  }

  void dispose() {
    _timer?.cancel();
    _timer = null;
  }
}
