/// Pure decisions behind the Direct (LAN) `/mobile` transport and the
/// relay's request budget.
///
/// Port of the socket-free logic in `RemoteDirectTransport.swift`
/// (`clients/legacy/ios/SupercliIOS`):
/// - which scheme a paired Host gets (pinned HTTPS vs legacy plaintext) and
///   how that decision is learned from bootstrap / pairing;
/// - whether a plaintext `/mobile` reply is the Host refusing the bearer;
/// - the canonical spelling of persisted `/mobile` endpoints;
/// - the bootstrap deadline per transport;
/// - the push-token registration route for each paired Mac.
///
/// Everything here is socket-free and unit-tested. The per-fingerprint
/// URLSession cache (`RemotePinnedURLSessionCache`) is Foundation-bound and
/// stays out, as does `applying` (it folds into `PairedMacRecord`, which has
/// no Dart equivalent yet).
///
/// Wire constants mirror `RemoteControlProtocol` in the legacy shared Swift
/// package.
library;

/// Host capability advertising TLS on `/mobile`.
const String kMobileTlsCapability = 'host.mobile.tls';

/// Minimum server version that serves TLS on `/mobile`.
const String kMobileTlsMinimumServerVersion = '0.5.3';

/// Semantic server version (`"0.5.3"`, `"0.10.0-beta.2"`). Pre-release and
/// build suffixes are ignored: `0.5.3-beta.1` is 0.5.3 for gating purposes
/// because the feature ships with the release line, not the tag.
///
/// Port of `RemoteServerVersion`.
final class RemoteServerVersion implements Comparable<RemoteServerVersion> {
  const RemoteServerVersion(this.major, this.minor, this.patch);

  final int major;
  final int minor;
  final int patch;

  /// Parse `"0.5.3"`, `"v0.5.3"`, `"0.5.3-beta.1"`, `"1"`. Returns null
  /// for anything else (empty, non-numeric parts, more than 3 components).
  static RemoteServerVersion? parse(String? raw) {
    if (raw == null) return null;
    var core = raw.trim();
    if (core.isEmpty) return null;
    if (core.startsWith('v') || core.startsWith('V')) {
      core = core.substring(1);
    }
    // Cut pre-release (`-`) and build (`+`) suffixes.
    final dash = core.indexOf('-');
    final plus = core.indexOf('+');
    var cut = core.length;
    if (dash >= 0) cut = dash;
    if (plus >= 0 && plus < cut) cut = plus;
    core = core.substring(0, cut);
    final parts = core.split('.');
    if (parts.isEmpty || parts.length > 3) return null;
    final numbers = <int>[0, 0, 0];
    for (var i = 0; i < parts.length; i++) {
      final part = parts[i];
      if (part.isEmpty) return null;
      final value = int.tryParse(part);
      if (value == null || value < 0) return null;
      numbers[i] = value;
    }
    return RemoteServerVersion(numbers[0], numbers[1], numbers[2]);
  }

  @override
  int compareTo(RemoteServerVersion other) {
    if (major != other.major) return major.compareTo(other.major);
    if (minor != other.minor) return minor.compareTo(other.minor);
    return patch.compareTo(other.patch);
  }

  @override
  bool operator ==(Object other) =>
      other is RemoteServerVersion &&
      other.major == major &&
      other.minor == minor &&
      other.patch == patch;

  @override
  int get hashCode => Object.hash(major, minor, patch);

  @override
  String toString() => '$major.$minor.$patch';
}

/// What a Host said about its Direct transport, extracted at the wire
/// boundary from a bootstrap snapshot or a sealed pairing response.
///
/// Port of `RemoteDirectTransportAdvertisement`.
final class RemoteDirectTransportAdvertisement {
  const RemoteDirectTransportAdvertisement({
    this.certificateFingerprint,
    this.serverVersion,
    this.hostCapabilities,
  });

  /// Lowercase hex SHA-256 of the Host's self-signed TLS leaf.
  final String? certificateFingerprint;
  final String? serverVersion;

  /// `hostProtocol.capabilities`; null on a pre-ledger Host.
  final Set<String>? hostCapabilities;
}

/// The scheme a paired Host's Direct `/mobile` requests use.
///
/// Port of `RemoteDirectTransportDecision`.
sealed class RemoteDirectTransportDecision {
  const RemoteDirectTransportDecision._();
}

/// Pinned HTTPS.
final class DirectTransportTls extends RemoteDirectTransportDecision {
  const DirectTransportTls(this.fingerprint) : super._();
  final String fingerprint;

  @override
  bool operator ==(Object other) =>
      other is DirectTransportTls && other.fingerprint == fingerprint;

  @override
  int get hashCode => fingerprint.hashCode;
}

/// The Host said it serves TLS but advertised no certificate to pin to.
/// Stay on whatever the record already uses; never send the bearer token to
/// an unpinned TLS endpoint, and never assume plaintext is acceptable.
final class DirectTransportTlsUnpinnable extends RemoteDirectTransportDecision {
  const DirectTransportTlsUnpinnable() : super._();

  @override
  bool operator ==(Object other) => other is DirectTransportTlsUnpinnable;

  @override
  int get hashCode => 0;
}

/// The Host conclusively predates TLS on `/mobile` (a version below the
/// minimum). Plaintext is the only transport it accepts.
final class DirectTransportPlaintext extends RemoteDirectTransportDecision {
  const DirectTransportPlaintext() : super._();

  @override
  bool operator ==(Object other) => other is DirectTransportPlaintext;

  @override
  int get hashCode => 1;
}

/// The Host said nothing either way (pre-version, pre-ledger). Keep the
/// record's current transport.
final class DirectTransportUnknown extends RemoteDirectTransportDecision {
  const DirectTransportUnknown() : super._();

  @override
  bool operator ==(Object other) => other is DirectTransportUnknown;

  @override
  int get hashCode => 2;
}

/// Strict-but-liberal fingerprint normalization for the transport decision:
/// trims whitespace, lowercases; empty becomes null.
String? normalizedFingerprint(String? raw) {
  if (raw == null) return null;
  final trimmed = raw.trim().toLowerCase();
  return trimmed.isEmpty ? null : trimmed;
}

/// The transport decision for one Host: the capability flag wins outright;
/// otherwise a reported server version at/after the minimum means TLS and a
/// lower one means plaintext. No signal keeps the current transport.
///
/// Port of `RemoteDirectTransportPolicy.decision`.
RemoteDirectTransportDecision transportDecision(
  RemoteDirectTransportAdvertisement advertisement,
) {
  final fingerprint = normalizedFingerprint(
    advertisement.certificateFingerprint,
  );
  if (advertisement.hostCapabilities?.contains(kMobileTlsCapability) == true) {
    final f = fingerprint;
    return f == null
        ? const DirectTransportTlsUnpinnable()
        : DirectTransportTls(f);
  }
  final version = RemoteServerVersion.parse(advertisement.serverVersion);
  final minimum = RemoteServerVersion.parse(kMobileTlsMinimumServerVersion);
  if (version == null || minimum == null) {
    return const DirectTransportUnknown();
  }
  if (version.compareTo(minimum) < 0) {
    return const DirectTransportPlaintext();
  }
  final f = fingerprint;
  return f == null
      ? const DirectTransportTlsUnpinnable()
      : DirectTransportTls(f);
}

/// Whether a plaintext `/mobile` reply is the Host refusing the bearer
/// over plaintext (the transition-era `426 Upgrade Required`, or a `401`
/// whose message points at HTTPS/TLS). Any other 4xx keeps its meaning.
///
/// Port of `RemoteDirectTransportPolicy.isPlaintextRefusal`.
bool isPlaintextRefusal({required int statusCode, String? serverMessage}) {
  if (statusCode == 426) return true;
  if (statusCode != 401 || serverMessage == null) return false;
  final lowered = serverMessage.toLowerCase();
  return lowered.contains('https') || lowered.contains('tls');
}

/// Persisted `/mobile` endpoints are always spelled `http://` — the pin,
/// not the stored scheme, decides the wire. Normalizing here keeps the
/// endpoint-equality checks in the generation guards stable across a
/// Host that starts advertising `https://`.
///
/// Port of `RemoteDirectTransportPolicy.canonicalStoredEndpoint`.
String canonicalStoredEndpoint(String endpoint) {
  final uri = Uri.tryParse(endpoint);
  if (uri == null) return endpoint;
  if (uri.scheme.toLowerCase() != 'https') return endpoint;
  return uri.replace(scheme: 'http').toString();
}

/// Bootstrap is the connection health signal, so its deadline is short on
/// the LAN. Over the relay a bootstrap crosses the tunnel twice plus the
/// Host's own work; on cellular that legitimately misses 4 s, and each miss
/// used to be read as "connection lost". Budget it from the measured path.
///
/// Port of `RemoteBootstrapDeadline`.
abstract final class RemoteBootstrapDeadline {
  static const double direct = 4.0;
  static const double relayMinimum = 10.0;
  static const double relayMaximum = 20.0;

  /// Multiplier on the last measured relay round-trip. A bootstrap is one
  /// request; giving it several RTTs of headroom absorbs jitter without
  /// letting a genuinely dead path linger past the keepalive limit.
  static const double relayRoundTripMultiplier = 5.0;

  static double seconds({required bool isRelay, double? measuredRoundTrip}) {
    if (!isRelay) return direct;
    final rtt = measuredRoundTrip;
    if (rtt == null || !rtt.isFinite || rtt <= 0) return relayMinimum;
    final scaled = rtt * relayRoundTripMultiplier;
    return scaled.clamp(relayMinimum, relayMaximum);
  }
}

/// How an APNs token reaches one paired Mac.
///
/// Port of `PushTokenRegistrationRoute`.
enum PushTokenRegistrationRoute {
  /// POST over the paired Direct endpoint (pinned HTTPS or legacy HTTP).
  direct,

  /// POST through the connection store's live Link connection for the
  /// active Mac — no second socket, no LAN wait.
  activeRelayClient,

  /// POST through a short-lived Link connection built for this Mac.
  transientRelay,
}

/// Order of attempts for one Mac. The active Mac already on the relay
/// skips the LAN entirely: that attempt is known to fail and used to hold
/// the registration for the full POST timeout before opening a
/// throwaway relay socket next to the live one.
///
/// Port of `PushTokenRegistrationRoute.plan`.
List<PushTokenRegistrationRoute> pushTokenRegistrationPlan({
  required bool isActiveMac,
  required bool usingRelay,
  required bool hasRelayCredentials,
}) {
  if (isActiveMac && usingRelay) {
    return const [PushTokenRegistrationRoute.activeRelayClient];
  }
  final routes = <PushTokenRegistrationRoute>[
    PushTokenRegistrationRoute.direct,
  ];
  if (hasRelayCredentials) {
    routes.add(PushTokenRegistrationRoute.transientRelay);
  }
  return routes;
}
