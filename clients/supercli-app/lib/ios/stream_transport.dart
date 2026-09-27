/// Pure protocol/selection logic for the terminal output WebSocket exposed
/// by the Host's remote server.
///
/// Port of `RemoteTerminalStreamTransport.swift`
/// (`clients/legacy/ios/SupercliIOS`): hello/error frame decoding, binary
/// frame parsing (8-byte big-endian output offset prefix), certificate
/// fingerprint normalization, the transport-selection decision (WS candidate
/// vs. the HTTP long-poll fallback), client input framing, and reconnect
/// backoff.
///
/// Deliberately socket-free and side-effect-free so every piece is unit
/// testable; the live connection itself is transport execution and stays
/// out.
library;

import 'dart:convert';

/// The advertised remote-server endpoint, as discovered from the latest
/// bootstrap/pairing response. The port is OS-assigned per server run
/// (never cache it across reconnects — always read the freshest value);
/// the fingerprint is stable across restarts and already normalized.
final class RemoteServerEndpoint {
  const RemoteServerEndpoint({
    required this.port,
    required this.certificateFingerprint,
  });

  final int port;

  /// Normalized lowercase hex SHA-256 of the TLS leaf certificate DER.
  final String certificateFingerprint;

  @override
  bool operator ==(Object other) =>
      other is RemoteServerEndpoint &&
      other.port == port &&
      other.certificateFingerprint == certificateFingerprint;

  @override
  int get hashCode => Object.hash(port, certificateFingerprint);
}

/// Everything needed for one WS connect attempt: host from the paired
/// mobile endpoint, port + pin from discovery, the paired bearer token.
final class RemoteTerminalWebSocketCandidate {
  const RemoteTerminalWebSocketCandidate({
    required this.host,
    required this.port,
    required this.certificateFingerprint,
    required this.token,
  });

  final String host;
  final int port;
  final String certificateFingerprint;
  final String token;

  @override
  bool operator ==(Object other) =>
      other is RemoteTerminalWebSocketCandidate &&
      other.host == host &&
      other.port == port &&
      other.certificateFingerprint == certificateFingerprint &&
      other.token == token;

  @override
  int get hashCode => Object.hash(host, port, certificateFingerprint, token);
}

/// Transport-selection decisions for the terminal output stream.
abstract final class RemoteTerminalTransportSelector {
  static const _hexDigits = '0123456789abcdef';

  /// Strict-but-liberal fingerprint normalization: strips an optional
  /// `sha256:` prefix, colons, and whitespace, lowercases, and requires
  /// exactly 64 hex characters. Anything else is unusable for pinning —
  /// and no pin means no WS (there is deliberately no bypass).
  static String? normalizedFingerprint(String? raw) {
    if (raw == null) return null;
    var value = raw.trim().toLowerCase();
    if (value.startsWith('sha256:')) {
      value = value.substring('sha256:'.length);
    }
    value = value.replaceAll(':', '');
    if (value.length != 64) return null;
    for (var i = 0; i < value.length; i++) {
      if (!_hexDigits.contains(value[i])) return null;
    }
    return value;
  }

  static bool fingerprintsMatch(String? a, String? b) {
    final na = normalizedFingerprint(a);
    final nb = normalizedFingerprint(b);
    if (na == null || nb == null) return false;
    return na == nb;
  }

  /// Discovery: a usable remote-server endpoint requires both a live port
  /// and a valid fingerprint. The dev bridge advertises neither, so it
  /// always resolves null — the HTTP long-poll stays its only transport.
  static RemoteServerEndpoint? endpoint({
    required int? port,
    required String? fingerprint,
  }) {
    if (port == null || port < 1 || port > 65535) return null;
    final normalized = normalizedFingerprint(fingerprint);
    if (normalized == null) return null;
    return RemoteServerEndpoint(port: port, certificateFingerprint: normalized);
  }

  /// The transport decision for one stream attempt: WS when the Host
  /// advertises the remote server AND we hold a paired token to present;
  /// null means HTTP long-poll (dev bridge, server down, or pre-WS build).
  static RemoteTerminalWebSocketCandidate? candidate({
    required RemoteServerEndpoint? endpoint,
    required String host,
    required String? authToken,
  }) {
    if (endpoint == null) return null;
    if (host.isEmpty) return null;
    final token = authToken;
    if (token == null || token.isEmpty) return null;
    return RemoteTerminalWebSocketCandidate(
      host: host,
      port: endpoint.port,
      certificateFingerprint: endpoint.certificateFingerprint,
      token: token,
    );
  }

  /// `wss://<host>:<port>/api/sessions/<id>/output?token=...[&offset=N]`
  static Uri? webSocketOutputUrl({
    required String host,
    required int port,
    required String sessionId,
    required String token,
    required int? offset,
  }) {
    if (host.isEmpty || sessionId.isEmpty) return null;
    final query = <String, String>{'token': token};
    if (offset != null) query['offset'] = '$offset';
    return Uri(
      scheme: 'wss',
      host: host,
      port: port,
      path: '/api/sessions/$sessionId/output',
      queryParameters: query,
    );
  }
}

/// The server's first text frame on a successful upgrade.
final class RemoteTerminalWSHello {
  const RemoteTerminalWSHello({
    required this.protocolVersion,
    required this.sessionId,
    required this.state,
    required this.outputSize,
    required this.requestedOffset,
    required this.startOffset,
    required this.rebased,
    required this.cols,
    required this.rows,
    required this.modePreambleBase64,
  });

  factory RemoteTerminalWSHello.fromJson(Map<String, Object?> json) {
    return RemoteTerminalWSHello(
      protocolVersion: (json['protocol'] as num).toInt(),
      sessionId: json['session_id'] as String,
      state: json['state'] as String,
      outputSize: (json['output_size'] as num).toInt(),
      requestedOffset: (json['requested_offset'] as num?)?.toInt(),
      startOffset: (json['start_offset'] as num).toInt(),
      rebased: json['rebased'] as bool,
      cols: (json['cols'] as num?)?.toInt(),
      rows: (json['rows'] as num?)?.toInt(),
      modePreambleBase64: json['mode_preamble_base64'] as String?,
    );
  }

  final int protocolVersion;
  final String sessionId;
  final String state;

  /// Total output size at connect time.
  final int outputSize;

  /// The offset the client asked for (null on a fresh connect).
  final int? requestedOffset;

  /// Where the binary stream actually begins.
  final int startOffset;

  /// True when the requested offset was unusable and the server restarted
  /// from an aligned tail — the client must clear before feeding, like an
  /// HTTP rebase.
  final bool rebased;
  final int? cols;
  final int? rows;

  /// DEC-mode restore preamble (base64) the client feeds into its freshly
  /// reset VT before the replayed tail. Absent from older Hosts and at the
  /// session origin. Not journal bytes: it never moves `startOffset` or the
  /// resume cursor.
  final String? modePreambleBase64;

  List<int>? get modePreamble {
    final encoded = modePreambleBase64;
    if (encoded == null) return null;
    try {
      final bytes = base64.decode(encoded);
      if (bytes.isEmpty) return null;
      return bytes;
    } on FormatException {
      return null;
    }
  }

  @override
  bool operator ==(Object other) =>
      other is RemoteTerminalWSHello &&
      other.protocolVersion == protocolVersion &&
      other.sessionId == sessionId &&
      other.state == state &&
      other.outputSize == outputSize &&
      other.requestedOffset == requestedOffset &&
      other.startOffset == startOffset &&
      other.rebased == rebased &&
      other.cols == cols &&
      other.rows == rows &&
      other.modePreambleBase64 == modePreambleBase64;

  @override
  int get hashCode => Object.hash(
    protocolVersion,
    sessionId,
    state,
    outputSize,
    requestedOffset,
    startOffset,
    rebased,
    cols,
    rows,
    modePreambleBase64,
  );
}

/// Server→client text frames: the hello, or non-fatal in-stream errors.
sealed class RemoteTerminalWSServerMessage {
  const RemoteTerminalWSServerMessage._();
}

final class WsHelloMessage extends RemoteTerminalWSServerMessage {
  const WsHelloMessage(this.hello) : super._();
  final RemoteTerminalWSHello hello;

  @override
  bool operator ==(Object other) =>
      other is WsHelloMessage && other.hello == hello;

  @override
  int get hashCode => hello.hashCode;
}

final class WsErrorMessage extends RemoteTerminalWSServerMessage {
  const WsErrorMessage(this.message) : super._();
  final String message;

  @override
  bool operator ==(Object other) =>
      other is WsErrorMessage && other.message == message;

  @override
  int get hashCode => message.hashCode;
}

final class WsUnknownMessage extends RemoteTerminalWSServerMessage {
  const WsUnknownMessage() : super._();

  @override
  bool operator ==(Object other) => other is WsUnknownMessage;

  @override
  int get hashCode => 0;
}

RemoteTerminalWSServerMessage parseWsServerMessage(String text) {
  final Object? decoded;
  try {
    decoded = json.decode(text);
  } on FormatException {
    return const WsUnknownMessage();
  }
  if (decoded is! Map<String, Object?>) return const WsUnknownMessage();
  final type = decoded['type'];
  if (type == 'hello') {
    try {
      return WsHelloMessage(RemoteTerminalWSHello.fromJson(decoded));
    } on Object {
      return const WsUnknownMessage();
    }
  }
  if (type == 'error') {
    final message = decoded['message'];
    return WsErrorMessage(message is String ? message : 'unknown error');
  }
  return const WsUnknownMessage();
}

/// Server→client binary frame: bytes 0-7 are the big-endian u64 output
/// offset of the first payload byte, the rest is raw terminal bytes (no
/// base64). `offset + payload.length` is the resume offset — the same offset
/// space the HTTP long-poll uses, so the two transports are interchangeable.
final class RemoteTerminalWSBinaryFrame {
  const RemoteTerminalWSBinaryFrame({
    required this.offset,
    required this.payload,
  });

  final int offset;
  final List<int> payload;

  static RemoteTerminalWSBinaryFrame? parse(List<int> data) {
    if (data.length < 8) return null;
    var offset = 0;
    for (var i = 0; i < 8; i++) {
      offset = (offset << 8) | data[i];
    }
    return RemoteTerminalWSBinaryFrame(
      offset: offset,
      payload: data.sublist(8),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is RemoteTerminalWSBinaryFrame &&
      other.offset == offset &&
      _bytesEqual(other.payload, payload);

  @override
  int get hashCode => Object.hash(offset, Object.hashAll(payload));
}

bool _bytesEqual(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

/// Client→server JSON text frames.
abstract final class RemoteTerminalWSClientMessage {
  /// The server caps one input message's data at 64KB; chunk well under it
  /// (on character boundaries, so escape sequences and multi-byte UTF-8
  /// never split mid-scalar).
  static const int maxInputBytesPerFrame = 32 * 1024;

  /// One raw-PTY input as one or more `{"type":"input","data":...}` frames,
  /// in order. No ack is expected — the echo arrives via output.
  ///
  /// [writeId] is the idempotency key the caller also sends on the HTTP
  /// fallback for this same logical send. It is attached only when the send
  /// fits in one frame.
  static List<String> inputFrames(
    String text, {
    String? writeId,
    int maxBytes = maxInputBytesPerFrame,
  }) {
    if (text.isEmpty) return const [];
    if (_utf8Length(text) <= maxBytes) {
      return [_encodeInput(text, writeId: writeId)];
    }
    final frames = <String>[];
    final chunk = StringBuffer();
    var chunkBytes = 0;
    for (final rune in text.runes) {
      final char = String.fromCharCode(rune);
      final size = _utf8Length(char);
      if (chunkBytes + size > maxBytes && chunkBytes > 0) {
        frames.add(_encodeInput(chunk.toString()));
        chunk.clear();
        chunkBytes = 0;
      }
      chunk.write(char);
      chunkBytes += size;
    }
    if (chunkBytes > 0) {
      frames.add(_encodeInput(chunk.toString()));
    }
    return frames;
  }

  static String _encodeInput(String data, {String? writeId}) {
    final payload = <String, Object?>{
      'type': 'input',
      'data': data,
      if (writeId != null && writeId.isNotEmpty) 'wid': writeId,
    };
    return json.encode(payload);
  }

  static int _utf8Length(String s) => utf8.encode(s).length;
}

/// Exponential reconnect backoff. A frame that paints resets the delay:
/// [delayAfterFailure] takes the latest healthy stream serial, so a
/// connection that was healthy before failing restarts at the initial
/// delay instead of compounding.
final class RemoteTerminalReconnectBackoff {
  RemoteTerminalReconnectBackoff({required int healthySerial})
    : _lastHealthySerial = healthySerial;

  static const int initialDelayMs = 500;
  static const int maximumDelayMs = 8000;

  int _nextDelayMs = initialDelayMs;
  int _lastHealthySerial;

  int delayAfterFailure(int healthySerial) {
    if (healthySerial != _lastHealthySerial) {
      _lastHealthySerial = healthySerial;
      _nextDelayMs = initialDelayMs;
    }
    final delay = _nextDelayMs;
    _nextDelayMs = (_nextDelayMs * 2).clamp(0, maximumDelayMs);
    return delay;
  }
}
