/// Nearby Host discovery catalog: candidate parsing and merging.
///
/// Port of `NearbyHostBrowser.swift` (the pure-logic half).
/// Bonjour is only a hint: choosing a row never grants access, and the
/// sealed one-time pairing code still authenticates the Host identity and
/// endpoint.
///
/// The platform browser itself (Apple Network.framework `NWBrowser`) is not
/// portable. Discovery results are fed in through [NearbyHostDiscovery]:
/// the platform layer calls [onDiscovered] with raw service name + TXT
/// records, and this library parses, deduplicates, excludes, and sorts.
///
/// Backs the "Nearby Hosts" table in the Add Workspace sheet
/// (`screens/hostpickerview.dart`).
library;

/// A single discovered Host candidate.
final class NearbyHostCandidate {
  const NearbyHostCandidate({required this.hostId, required this.name});

  /// Stable Host identity from the TXT `macid` record.
  final String hostId;

  /// Human-readable service name, or "Supercli Host" when blank.
  final String name;

  @override
  bool operator ==(Object other) =>
      other is NearbyHostCandidate &&
      other.hostId == hostId &&
      other.name == name;

  @override
  int get hashCode => Object.hash(hostId, name);

  @override
  String toString() => 'NearbyHostCandidate(hostId: $hostId, name: $name)';
}

/// Pure catalog functions for nearby-Host discovery results.
abstract final class NearbyHostCatalog {
  /// Bonjour service type advertised by Supercli Hosts.
  static const String serviceType = '_supercli-remote._tcp';

  /// Parse one discovered service into a candidate.
  ///
  /// Returns null when the TXT record has no usable `macid`.
  /// Mirrors `NearbyHostCatalog.candidate(serviceName:txt:)`.
  static NearbyHostCandidate? candidate({
    required String serviceName,
    required Map<String, String> txt,
  }) {
    final hostId = (txt['macid'] ?? '').trim();
    if (hostId.isEmpty) return null;
    final name = serviceName.trim();
    return NearbyHostCandidate(
      hostId: hostId,
      name: name.isEmpty ? 'Supercli Host' : name,
    );
  }

  /// Merge discovered candidates: deduplicate by Host ID (case-insensitive),
  /// drop the excluded Host, and sort by name (case-insensitive) then ID.
  ///
  /// Mirrors `NearbyHostCatalog.merging(_:excludingHostID:)`.
  static List<NearbyHostCandidate> merging(
    List<NearbyHostCandidate> candidates, {
    String? excludingHostId,
  }) {
    final excluded = excludingHostId?.trim().toLowerCase();
    final byId = <String, NearbyHostCandidate>{};
    for (final c in candidates) {
      final key = c.hostId.toLowerCase();
      if (key == excluded || byId.containsKey(key)) continue;
      byId[key] = c;
    }
    final merged = byId.values.toList();
    merged.sort((a, b) {
      final order = a.name.toLowerCase().compareTo(b.name.toLowerCase());
      if (order != 0) return order;
      return a.hostId.compareTo(b.hostId);
    });
    return merged;
  }
}

/// Discovery lifecycle state.
///
/// Mirrors `NearbyHostBrowser.State`.
enum NearbyDiscoveryState {
  /// Not browsing.
  idle,

  /// Actively browsing for `_supercli-remote._tcp`.
  searching,

  /// Browsing failed; carries the platform error message.
  unavailable,
}

/// Platform hook for Bonjour/mDNS discovery.
///
/// The platform implementation starts/stops the OS browser and forwards
/// raw discoveries to [onDiscovered]. [state] mirrors the browser lifecycle.
/// This keeps the catalog logic above unit-testable without Network.framework.
abstract class NearbyHostDiscovery {
  /// Current lifecycle state.
  NearbyDiscoveryState get state;

  /// Called with freshly parsed candidates after each browse update.
  ///
  /// The platform layer parses raw results via [NearbyHostCatalog.candidate]
  /// and merges via [NearbyHostCatalog.merging], then delivers the list here.
  void Function(List<NearbyHostCandidate> candidates)? onDiscovered;

  /// Called when browsing fails; carries the platform error message.
  void Function(String message)? onUnavailable;

  /// Begin browsing. Idempotent.
  void start();

  /// Stop browsing and clear candidates. Idempotent.
  void stop();
}
