/// Bundled Host service lifecycle management.
///
/// Port of the Swift `HostServiceManager` (launchd integration). Covers
/// checklist row 197: "Bundled Host service lifecycle management via
/// launchd".
///
/// The desktop app ships a bundled `supercli serve` binary. This module
/// owns its lifecycle: start/stop/restart/status, plus generating the
/// platform service definitions (launchd plist on macOS, systemd unit on
/// Linux) so the Host can run in the background and survive restarts.
///
/// Platform execution (actually loading the plist / enabling the unit)
/// goes through the Host; this is the model + definition generator with
/// a testable state machine.
library;

/// Lifecycle state of the bundled Host service.
enum HostServiceState {
  stopped,
  starting,
  running,
  stopping,
  failed,
}

/// A bundled Host service instance.
final class HostServiceManager {
  HostServiceManager({
    required this.label,
    required this.executablePath,
    this.arguments = const [],
    this.workingDirectory,
    this.environment = const {},
  });

  /// Reverse-DNS service label, e.g. `li.superc.host`.
  final String label;
  final String executablePath;
  final List<String> arguments;
  final String? workingDirectory;
  final Map<String, String> environment;

  HostServiceState _state = HostServiceState.stopped;
  HostServiceState get state => _state;

  String? _lastError;
  String? get lastError => _lastError;

  /// Request a start. Returns false if a start is already in progress
  /// or the service is already running.
  bool requestStart() {
    if (_state == HostServiceState.starting ||
        _state == HostServiceState.running) {
      return false;
    }
    _state = HostServiceState.starting;
    _lastError = null;
    return true;
  }

  /// Request a stop. Returns false if already stopped/stopping.
  bool requestStop() {
    if (_state == HostServiceState.stopped ||
        _state == HostServiceState.stopping) {
      return false;
    }
    _state = HostServiceState.stopping;
    return true;
  }

  /// The platform layer calls this when the process is confirmed up.
  void markRunning() {
    _state = HostServiceState.running;
    _lastError = null;
  }

  /// The platform layer calls this when the process exited.
  void markStopped() {
    _state = HostServiceState.stopped;
  }

  /// The platform layer calls this when the process failed to start
  /// or crashed.
  void markFailed(String error) {
    _state = HostServiceState.failed;
    _lastError = error;
  }

  bool get isRunning => _state == HostServiceState.running;

  /// Generates the macOS launchd plist for this service.
  ///
  /// Mirrors the Swift implementation: `RunAtLoad`, `KeepAlive` with
  /// `SuccessfulExit: false` (restart on crash, not on clean exit),
  /// stdout/stderr log paths under `~/Library/Logs`.
  String launchdPlist() {
    final args = [executablePath, ...arguments]
        .map((a) => '    <string>${_xmlEscape(a)}</string>')
        .join('\n');
    final env = environment.entries
        .map((e) =>
            '    <key>${_xmlEscape(e.key)}</key>\n    <string>${_xmlEscape(e.value)}</string>')
        .join('\n');
    return '''<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${_xmlEscape(label)}</string>
  <key>ProgramArguments</key>
  <array>
$args
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>StandardOutPath</key>
  <string>\$HOME/Library/Logs/${_xmlEscape(label)}.out.log</string>
  <key>StandardErrorPath</key>
  <string>\$HOME/Library/Logs/${_xmlEscape(label)}.err.log</string>${workingDirectory == null ? '' : '\n  <key>WorkingDirectory</key>\n  <string>${_xmlEscape(workingDirectory!)}</string>'}${environment.isEmpty ? '' : '\n  <key>EnvironmentVariables</key>\n  <dict>\n$env\n  </dict>'}
</dict>
</plist>
''';
  }

  /// Generates the Linux systemd user unit for this service.
  String systemdUnit() {
    final exec = [executablePath, ...arguments].join(' ');
    final env = environment.entries
        .map((e) => 'Environment="${e.key}=${e.value}"')
        .join('\n');
    return '''[Unit]
Description=supercli Host service ($label)
After=network.target

[Service]
Type=simple
ExecStart=$exec${workingDirectory == null ? '' : '\nWorkingDirectory=$workingDirectory'}${environment.isEmpty ? '' : '\n$env'}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
''';
  }

  static String _xmlEscape(String s) => s
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;');
}
