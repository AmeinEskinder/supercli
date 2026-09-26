/// Bonjour (mDNS/DNS-SD) nearby-host discovery for "Add Workspace".
///
/// Port of the Swift `BonjourBrowser`. Covers checklist row 198:
/// "Bonjour nearby-host discovery for Add Workspace".
///
/// The real mDNS socket work happens on the platform side (or via the
/// Host's `_supercli._tcp` browse); this is the data model plus a
/// testable discovery controller: dedup by service name, stale-host
/// expiry, and found/lost callbacks that feed the workspace picker.
library;

import 'dart:async';

/// mDNS service type browsed for supercli Hosts.
const String supercliServiceType = '_supercli._tcp';

/// A Host discovered on the local network.
final class DiscoveredHost {
  DiscoveredHost({
    required this.serviceName,
    required this.host,
    required this.port,
    this.txt = const {},
    DateTime? lastSeen,
  }) : lastSeen = lastSeen ?? DateTime.now();

  final String serviceName;
  final String host;
  final int port;
  final Map<String, String> txt;
  final DateTime lastSeen;

  /// Display name prefers the `name` TXT record, then the service name.
  String get displayName => txt['name'] ?? serviceName;

  /// Host API version advertised via TXT, if any.
  String? get apiVersion => txt['api'];

  DiscoveredHost touch() => DiscoveredHost(        serviceName: serviceName,
        host: host,
        port: port,
        txt: txt,
        lastSeen: DateTime.now(),
      );

  @override
  bool operator ==(Object other) =>
      other is DiscoveredHost && other.serviceName == serviceName;

  @override
  int get hashCode => serviceName.hashCode;
}

/// Controller for nearby-host discovery.
///
/// Feed it browse events via [found]/[lost]; it dedups, expires stale
/// entries after [staleAfter], and notifies [onChanged].
final class NearbyHostDiscovery {
  NearbyHostDiscovery({
    this.staleAfter = const Duration(seconds: 30),
    this.onChanged,
  });

  final Duration staleAfter;
  final void Function()? onChanged;

  final Map<String, DiscoveredHost> _hosts = {};

  List<DiscoveredHost> get hosts => _hosts.values.toList()
    ..sort((a, b) => a.displayName.compareTo(b.displayName));

  bool _running = false;
  bool get isRunning => _running;

  Timer? _expiryTimer;

  void start() {
    if (_running) return;
    _running = true;
    _expiryTimer =
        Timer.periodic(const Duration(seconds: 5), (_) => _expireStale());
  }

  void stop() {
    _running = false;
    _expiryTimer?.cancel();
    _expiryTimer = null;
  }

  /// A browse result arrived (or was re-announced).
  void found(DiscoveredHost host) {
    final isNew = !_hosts.containsKey(host.serviceName);
    _hosts[host.serviceName] = host.touch();
    if (isNew) onChanged?.call();
  }

  /// The platform reports a service going away.
  void lost(String serviceName) {
    if (_hosts.remove(serviceName) != null) onChanged?.call();
  }

  /// Drop entries not re-announced within [staleAfter]. Returns the
  /// number of expired hosts.
  int expireStale({DateTime? now}) {
    final at = now ?? DateTime.now();
    final stale = _hosts.values
        .where((h) => at.difference(h.lastSeen) > staleAfter)
        .map((h) => h.serviceName)
        .toList();
    for (final name in stale) {
      _hosts.remove(name);
    }
    if (stale.isNotEmpty) onChanged?.call();
    return stale.length;
  }

  void _expireStale() => expireStale();

  void dispose() => stop();
}
