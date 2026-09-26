/// Add iPhone/iPad to a remote Host (controller-assisted pairing).
///
/// Port of `RemotePairingView.swift`. Covers checklist row 208:
/// "Add iPhone/iPad to a remote Host (controller-assisted pairing)".
///
/// Flow: the desktop app (acting as controller) shows a short pairing
/// code + QR payload; the iOS device scans/enters it, then the Host
/// verifies and the flow completes. This is the state machine plus the
/// pairing sheet; code issuance/verification go through the Host.
library;

import 'package:gpuidart/gpuidart.dart';

/// Pairing flow states.
enum RemotePairingState {
  idle,
  showingCode,
  verifying,
  paired,
  failed,
}

/// Controller-assisted pairing flow for adding an iOS device to a
/// remote Host.
final class RemotePairingFlow {
  RemotePairingFlow();

  RemotePairingState _state = RemotePairingState.idle;
  RemotePairingState get state => _state;

  String _pairCode = '';
  String get pairCode => _pairCode;

  String? _failureReason;
  String? get failureReason => _failureReason;

  /// Begin pairing: the Host issues a code (fed in via [code]).
  void begin(String code) {
    _pairCode = code;
    _failureReason = null;
    _state = RemotePairingState.showingCode;
  }

  /// The device submitted the code; waiting on Host verification.
  void deviceSubmitted() {
    if (_state == RemotePairingState.showingCode) {
      _state = RemotePairingState.verifying;
    }
  }

  /// Host confirmed the pairing.
  void confirmed() {
    if (_state == RemotePairingState.verifying ||
        _state == RemotePairingState.showingCode) {
      _state = RemotePairingState.paired;
      _failureReason = null;
    }
  }

  /// Pairing failed (wrong code, timeout, Host unreachable).
  void fail(String reason) {
    _state = RemotePairingState.failed;
    _failureReason = reason;
  }

  void reset() {
    _state = RemotePairingState.idle;
    _pairCode = '';
    _failureReason = null;
  }

  /// QR payload scanned by the iOS device: `supercli://pair?code=…&host=…`.
  String qrPayload(String hostAddress) =>
      'supercli://pair?code=$_pairCode&host=${Uri.encodeComponent(hostAddress)}';
}

/// Pairing sheet UI.
final class RemotePairingView {
  const RemotePairingView({
    required this.flow,
    this.hostName = '',
  });

  final RemotePairingFlow flow;
  final String hostName;

  UiNode build() {
    final state = flow.state;
    return UiColumn('remote-pairing', [
      UiText('rp-title', 'Add iPhone/iPad to $hostName'),
      switch (state) {
        RemotePairingState.idle => const UiText(
            'rp-idle', 'Tap "Generate Code" to begin pairing.'),
        RemotePairingState.showingCode => UiColumn('rp-code', [
            const UiText('rp-instr',
                'On your iPhone/iPad, scan the QR code or enter the code:'),
            UiText('rp-code-value', flow.pairCode),
            const UiButton('rp-regenerate', 'Regenerate Code'),
          ]),
        RemotePairingState.verifying => const UiText(
            'rp-verifying', 'Verifying… check your device.'),
        RemotePairingState.paired =>
          const UiText('rp-paired', 'Device paired successfully.'),
        RemotePairingState.failed => UiColumn('rp-failed', [
            UiText('rp-fail-reason', 'Pairing failed: ${flow.failureReason}'),
            const UiButton('rp-retry', 'Try Again'),
          ]),
      },
      UiRow('rp-actions', [
        if (state == RemotePairingState.idle)
          const UiButton('rp-generate', 'Generate Code'),
        const UiButton('rp-cancel', 'Cancel'),
      ]),
    ]);
  }
}
