/// Remote Host scope connection with Direct → Link automatic fallback.
///
/// Covers checklist row 217: "Remote Host scope uses the same
/// sidebar/content UI with Direct→Link automatic fallback".
///
/// When the scope is a remote Host, the app first tries a direct
/// connection (LAN/Tailscale address); if that fails it automatically
/// falls back to the Link relay, and promotes back to direct when the
/// direct path recovers. The sidebar/content UI is transport-agnostic:
/// it only observes [activeTransport].
library;

/// Which transport to prefer for a remote Host scope.
enum HostConnectionPreference {
  /// Try direct, fall back to Link automatically.
  auto,

  /// Direct only; fail instead of falling back.
  directOnly,

  /// Link relay only.
  linkOnly,
}

/// The transport currently carrying the Host session.
enum HostTransport {
  direct,
  link,
}

/// Connection state machine for one remote Host scope.
final class RemoteHostConnection {
  RemoteHostConnection({
    this.preference = HostConnectionPreference.auto,
  });

  final HostConnectionPreference preference;

  HostTransport _active = HostTransport.direct;
  HostTransport get activeTransport => _active;

  bool _directHealthy = true;
  bool get directHealthy => _directHealthy;

  int _fallbackCount = 0;
  int get fallbackCount => _fallbackCount;

  /// Report a direct-transport failure. In `auto` mode this flips to
  /// Link; otherwise the failure just marks direct unhealthy.
  void directFailed() {
    _directHealthy = false;
    if (preference == HostConnectionPreference.auto &&
        _active == HostTransport.direct) {
      _active = HostTransport.link;
      _fallbackCount++;
    }
  }

  /// Report that the direct path recovered. In `auto` mode this
  /// promotes back to direct.
  void directRecovered() {
    _directHealthy = true;
    if (preference == HostConnectionPreference.auto &&
        _active == HostTransport.link) {
      _active = HostTransport.direct;
    }
  }

  /// Effective transport given the preference: `linkOnly` pins Link,
  /// `directOnly` pins direct, `auto` follows [_active].
  HostTransport get effectiveTransport => switch (preference) {
        HostConnectionPreference.linkOnly => HostTransport.link,
        HostConnectionPreference.directOnly => HostTransport.direct,
        HostConnectionPreference.auto => _active,
      };

  /// Human-readable status for the scope header.
  String get statusLabel => switch (effectiveTransport) {
        HostTransport.direct =>
          _directHealthy ? 'Direct' : 'Direct (unhealthy)',
        HostTransport.link => _fallbackCount > 0
            ? 'Link (fallback)'
            : 'Link',
      };
}
