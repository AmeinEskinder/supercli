/// Device presence tracking — which devices are viewing session terminals.
///
/// Port of `ViewerPresence.swift`
/// (`clients/legacy/native/SupercliNative/Sources/SupercliNative/ViewerPresence.swift`).
///
/// Two feeds converge here:
/// - File feed: the Rust remote server writes
///   `~/.supercli/remote/presence.json` whenever remote viewers change:
///   `{"version":1,"updated_at":ms,"sessions":{"&lt;id&gt;":[{"ip","kind":"ws"|"poll",
///   "device":"Name (id)"|null,"last_seen":ms}]}}`. Poll viewers have a 15s
///   TTL server-side, so entries whose last_seen is older than ~20s are
///   treated as stale on read.
/// - Mobile feed: authenticated Direct/Link output leases published beside
///   it as `mobile-presence.json` (same TTL rules).
///
/// This file ports the synchronous decision kernel: presence-file parsing,
/// per-feed TTL expiry, cross-feed merge with newest-wins, sort order,
/// connection-change announcements, and grid re-assert candidacy.
///
/// Fidelity gaps (platform runtime machinery — not ported):
/// - GAP-VIEWERPRESENCE-1: the DispatchSource directory watcher on the
///   `remote/` directory and the 5s fallback Timer are not ported. Callers
///   must call [ViewerPresenceStore.refresh] after the files change. The
///   `automaticallyUpdates` constructor flag is accepted for API parity but
///   only controls whether the initial load runs (it always does).
/// - GAP-VIEWERPRESENCE-2: `ViewerPresenceStore.shared` (which toasts via
///   ToastCenter) is not ported; inject [ViewerPresenceStore.onConnection].
/// - GAP-VIEWERPRESENCE-3: sort uses Dart's [String.toLowerCase] rather than
///   NSString's locale-aware `localizedCaseInsensitiveCompare`; ordering of
///   exotic Unicode names may differ from the Swift original.
library;

import 'dart:convert';
import 'dart:io';

/// One device currently viewing a session's terminal.
///
/// Presence is device-level observation, not human membership or a terminal
/// control lease.
class ViewerInfo {
  /// Stable viewer key: `device:<id>` for authenticated Controllers,
  /// `legacy:<source>:<identity>` for IP-only/legacy viewers.
  final String id;

  /// Stable paired-device id when this viewer is an authenticated
  /// Controller. Keeping it separate from the display label lets push
  /// suppression target only the phone that is actually watching instead
  /// of silencing every paired phone (or a remote Mac) at once.
  final String? deviceID;
  final String displayName;
  final DateTime lastSeen;

  const ViewerInfo({
    required this.id,
    required this.deviceID,
    required this.displayName,
    required this.lastSeen,
  });

  @override
  bool operator ==(Object other) =>
      other is ViewerInfo &&
      other.id == id &&
      other.deviceID == deviceID &&
      other.displayName == displayName &&
      other.lastSeen == lastSeen;

  @override
  int get hashCode => Object.hash(id, deviceID, displayName, lastSeen);

  @override
  String toString() =>
      'ViewerInfo(id: $id, deviceID: $deviceID, displayName: $displayName, '
      'lastSeen: $lastSeen)';
}

/// Tracks which devices are currently viewing session terminals, so pane
/// headers can show presence chips.
///
/// Both output feeds count as presence, including mobile viewers that may
/// resize the shared PTY. Observation alone does not prove who sized it.
class ViewerPresenceStore {
  /// File-feed staleness cutoff. The remote server prunes poll viewers
  /// after 15s; anything older than this on disk is a leftover from a dead
  /// server.
  static const Duration fileEntryTtl = Duration(seconds: 20);

  /// Host Direct/Link output lease TTL. The legacy filename is mobile, but
  /// this feed also carries paired Mac Controllers.
  static const Duration mobileEntryTtl = Duration(seconds: 15);

  final String presencePath;

  /// Authenticated Direct/Link output leases. Intentionally sits beside
  /// `presence.json` so the same directory watcher (GAP-VIEWERPRESENCE-1)
  /// covers both terminal data planes.
  final String mobilePresencePath;

  /// Called once per newly-arrived device id with its display name. The
  /// initial population is seeded silently (no toast for viewers already
  /// present at launch); a reconnect after the device drops re-announces.
  final void Function(String displayName)? onConnection;

  Map<String, List<ViewerInfo>> _viewers = {};
  Map<String, List<ViewerInfo>> _fileViewers = {};
  Map<String, List<ViewerInfo>> _mobileFileViewers = {};

  /// Session ids a remote viewer has been seen on at some point this app
  /// run. A remote controller can resize the *shared hosted PTY* while the
  /// local surface stays put (no local resize event ever fires). Consumed
  /// one candidacy per session by the desktop grid re-assert.
  final Set<String> _gridReassertCandidates = {};

  final Set<String> _announcedDeviceIDs = {};
  bool _didSeedAnnouncedDevices = false;

  ViewerPresenceStore({
    required this.presencePath,
    this.onConnection,

    /// Accepted for API parity with the Swift original; no watcher or
    /// timer is installed regardless (GAP-VIEWERPRESENCE-1).
    // ignore: avoid_unused_constructor_parameters
    bool automaticallyUpdates = true,
  }) : mobilePresencePath = _siblingPath(presencePath, 'mobile-presence.json') {
    _reloadPresenceFile();
  }

  static String _siblingPath(String path, String name) {
    final idx = path.lastIndexOf('/');
    return idx < 0 ? name : '${path.substring(0, idx)}/$name';
  }

  /// Session id → current viewers, already de-staled and sorted.
  Map<String, List<ViewerInfo>> get viewers => _viewers;

  /// Both output feeds count as presence, including mobile viewers that
  /// may resize the shared PTY.
  bool hasViewers(String sessionID) => !(_viewers[sessionID]?.isEmpty ?? true);

  /// Whether one exact paired Controller is currently rendering this
  /// session. Phone pushes are fanned out per target, so a foreground iPad
  /// must not suppress a background iPhone, and a remote Mac must not
  /// suppress either one.
  bool isDeviceViewing(String sessionID, String deviceID) =>
      _viewers[sessionID]?.any((v) => v.deviceID == deviceID) ?? false;

  /// Preserve the repair until all viewers have left and the Host's
  /// explicit fit has cleared. A present device must never lose its grid
  /// merely because another viewer disconnected.
  bool consumeGridReassertCandidate(
    String sessionID, {
    required bool hasActiveFit,
  }) {
    if (hasActiveFit || hasViewers(sessionID)) return false;
    return _gridReassertCandidates.remove(sessionID);
  }

  /// Re-read both feeds and prune expired entries. The Swift original also
  /// re-arms the directory watcher here when it lapsed
  /// (GAP-VIEWERPRESENCE-1).
  void refresh({DateTime? now}) => _reloadPresenceFile(now: now);

  void _reloadPresenceFile({DateTime? now}) {
    final at = now ?? DateTime.now();
    final data = _readFile(presencePath);
    if (data != null) {
      _fileViewers = parsePresence(data: data, source: 'terminal');
    } else if (_fileViewers.isNotEmpty) {
      // Missing/unreadable file simply means "no remote viewers".
      _fileViewers = {};
    }
    final mobileData = _readFile(mobilePresencePath);
    if (mobileData != null) {
      _mobileFileViewers = parsePresence(
        data: mobileData,
        source: 'direct-link',
      );
    } else if (_mobileFileViewers.isNotEmpty) {
      _mobileFileViewers = {};
    }
    _rebuild(now: at);
  }

  static List<int>? _readFile(String path) {
    try {
      return File(path).readAsBytesSync();
    } catch (_) {
      return null;
    }
  }

  void _rebuild({required DateTime now}) {
    final Map<String, Map<String, ViewerInfo>> bySession = {};
    // Expire each source before merging: a stale lease in one transport
    // must not hide the same device's live lease in the other.
    final feeds = [
      (_fileViewers, fileEntryTtl),
      (_mobileFileViewers, mobileEntryTtl),
    ];
    for (final feed in feeds) {
      final feedViewers = feed.$1;
      final ttl = feed.$2;
      for (final sessionEntry in feedViewers.entries) {
        for (final viewer in sessionEntry.value) {
          if (now.difference(viewer.lastSeen) > ttl) continue;
          final session = bySession.putIfAbsent(sessionEntry.key, () => {});
          final previous = session[viewer.id];
          if (previous != null && !viewer.lastSeen.isAfter(previous.lastSeen)) {
            continue;
          }
          session[viewer.id] = viewer;
        }
      }
    }
    final merged = <String, List<ViewerInfo>>{};
    for (final sessionEntry in bySession.entries) {
      final list = sessionEntry.value.values.toList()
        ..sort((a, b) {
          final order = a.displayName.toLowerCase().compareTo(
            b.displayName.toLowerCase(),
          );
          return order != 0 ? order : a.id.compareTo(b.id);
        });
      merged[sessionEntry.key] = list;
    }
    // Latch before publishing: whoever is viewing now may resize the
    // shared PTY at any point while present, so candidacy is set on
    // sight and only cleared by the consuming re-assert.
    _gridReassertCandidates.addAll(merged.keys);
    if (!_viewersEqual(_viewers, merged)) {
      _viewers = merged;
    }
    _announceConnectionChanges(merged);
  }

  static bool _viewersEqual(
    Map<String, List<ViewerInfo>> a,
    Map<String, List<ViewerInfo>> b,
  ) {
    if (a.length != b.length) return false;
    for (final key in a.keys) {
      final la = a[key]!;
      final lb = b[key];
      if (lb == null || la.length != lb.length) return false;
      for (var i = 0; i < la.length; i++) {
        if (la[i] != lb[i]) return false;
      }
    }
    return true;
  }

  /// Device ids currently present, so a viewer appearing (across any
  /// session, either transport) fires a one-shot "connected" toast rather
  /// than the only cue being the small title-bar avatar chips. Reconnect
  /// after the device drops re-announces.
  void _announceConnectionChanges(Map<String, List<ViewerInfo>> merged) {
    final live = <String, String>{}; // viewer id → display name
    for (final list in merged.values) {
      for (final viewer in list) {
        live[viewer.id] = viewer.displayName;
      }
    }
    final liveIDs = live.keys.toSet();
    // Suppress the initial population (app just launched with a phone
    // already viewing) — only announce genuinely new arrivals.
    if (!_didSeedAnnouncedDevices) {
      _announcedDeviceIDs
        ..clear()
        ..addAll(liveIDs);
      _didSeedAnnouncedDevices = true;
      return;
    }
    final arrivals = liveIDs.difference(_announcedDeviceIDs).toList()..sort();
    for (final id in arrivals) {
      onConnection?.call(live[id] ?? 'A device');
    }
    _announcedDeviceIDs
      ..clear()
      ..addAll(liveIDs);
  }

  /// Parse one presence feed file. Malformed input yields no viewers
  /// (callers treat it as "no remote viewers", never as an error).
  static Map<String, List<ViewerInfo>> parsePresence({
    required List<int> data,
    required String source,
  }) {
    final dynamic decoded;
    try {
      decoded = jsonDecode(utf8.decode(data));
    } catch (_) {
      return {};
    }
    if (decoded is! Map<String, dynamic>) return {};
    final sessions = decoded['sessions'];
    if (sessions is! Map) return {};

    final result = <String, List<ViewerInfo>>{};
    for (final sessionEntry in sessions.entries) {
      final entries = sessionEntry.value;
      if (entries is! List) continue;
      final list = <ViewerInfo>[];
      for (final raw in entries) {
        if (raw is! Map) continue;
        final device = raw['device'] is String ? raw['device'] as String : null;
        final ip = raw['ip'] is String ? raw['ip'] as String : null;
        final identity = device ?? ip ?? 'remote';
        final deviceID = deviceIDFromDevice(device);
        final lastSeenMs = raw['last_seen'] is num
            ? (raw['last_seen'] as num).toInt()
            : 0;
        list.add(
          ViewerInfo(
            // Keep duplicates until the timestamp-aware merge. The first
            // connection in the file may be older than another live one.
            id: deviceID != null
                ? 'device:$deviceID'
                : 'legacy:$source:$identity',
            deviceID: deviceID,
            displayName: displayNameFromDevice(device, ip),
            // Epoch millis are inherently UTC.
            lastSeen: DateTime.fromMillisecondsSinceEpoch(
              lastSeenMs,
              isUtc: true,
            ),
          ),
        );
      }
      if (list.isNotEmpty) result[sessionEntry.key.toString()] = list;
    }
    return result;
  }

  /// The remote server records `device` as "Name (id)"; show just the name.
  static String displayNameFromDevice(String? device, String? ip) {
    if (device != null && device.isNotEmpty) {
      if (device.endsWith(')')) {
        final open = device.lastIndexOf(' (');
        if (open >= 0) {
          final name = device.substring(0, open);
          if (name.isNotEmpty) return name;
        }
      }
      return device;
    }
    return ip ?? 'Remote viewer';
  }

  /// The remote server records authenticated Controllers as "Name (id)".
  /// An IP-only/legacy viewer has no stable device identity and therefore
  /// cannot suppress a particular phone's APNs target.
  static String? deviceIDFromDevice(String? device) {
    if (device == null || !device.endsWith(')')) return null;
    final open = device.lastIndexOf(' (');
    if (open < 0) return null;
    final start = open + 2;
    final end = device.length - 1;
    if (start >= end) return null;
    final id = device.substring(start, end);
    return id.isEmpty ? null : id;
  }
}
