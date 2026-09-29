/// App-kit UI protocol: message envelope, attach handshake, and text types.
/// Faithful wire port of the corresponding sections of
/// `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`
/// (4216 lines).
///
/// This file covers the protocol identity/capability constants (the protocol
/// declaration at the top of `UIProtocol.swift`), the attach handshake
/// `UIRendererMetadata`, `UIRendererState`, `UIAttach`, `UIAttached`), the
/// text primitives (`UITextPosition`, `UITextRange`, `UITextSelection`,
/// `UITextEdit`), and the top-level message envelope (`UISnapshot`, `UIEvent`,
/// `UIAction`, `UIEventValue`, `UIAck`, `UILifecycle`, `UIRequestSnapshot`,
/// `UIPresence`, `UIErrorMessage`, `UIMessage`).
///
/// Wire compatibility: JSON keys match the Swift `CodingKeys` exactly
/// (`protocol`, `appInstanceId`, `clientId`, `viewId`, `rendererId`,
/// `participantId`, `eventId`, `nodeId`, `sourceSessionId`, `selectedId`,
/// `anchorId`, `headId`, ...). Decode-time validation mirrors the Swift
/// `init(from:)` guards; violations throw [FormatException].
///
/// NOTE on protocol name: the wire protocol name is `"supercli.ui"`, matching
/// the pre-existing `AppKitProtocol.name` in `appkit_protocol.dart` and the
/// canonical `protocol/supercli-ui-v1.schema.json` (`"protocol": {"const":
/// "supercli.ui"}`). The frozen Swift source declared the legacy name; the
/// renamed protocol is the wire identity used by the Rust host.
///
/// The `delta` message case decodes through [UIDelta] from
/// `appkit_protocol_delta.dart` (the port of `UIDelta.swift`).
library;

import 'dart:convert';

import 'appkit_protocol_delta.dart';
import 'appkit_protocol_lists.dart';
import 'appkit_protocol_widgets.dart';

bool _listEq<T>(List<T> a, List<T> b) {
  if (identical(a, b)) return true;
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

int _utf8Length(String s) => utf8.encode(s).length;

// ---------------------------------------------------------------------------
// Attach handshake
// ---------------------------------------------------------------------------

/// Mirrors Swift `AppMetadata`.
final class AppMetadata {
  const AppMetadata({
    required this.id,
    required this.name,
    required this.version,
    this.description,
  });

  final String id;
  final String name;
  final String version;
  final String? description;

  factory AppMetadata.fromJson(Map<String, dynamic> json) => AppMetadata(
        id: json['id'] as String,
        name: json['name'] as String,
        version: json['version'] as String,
        description: json['description'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'version': version,
        if (description != null) 'description': description,
      };

  @override
  bool operator ==(Object other) =>
      other is AppMetadata &&
      other.id == id &&
      other.name == name &&
      other.version == version &&
      other.description == description;

  @override
  int get hashCode => Object.hash(id, name, version, description);
}

/// Mirrors Swift `UIParticipantKind`.
enum UIParticipantKind {
  human,
  agent,
  service;

  static UIParticipantKind fromJson(String value) =>
      UIParticipantKind.values.byName(value);
  String toJson() => name;
}

/// Mirrors Swift `UIParticipant`. Opaque Host identity and signed access
/// grants.
final class UIParticipant {
  const UIParticipant({
    required this.id,
    this.kind = UIParticipantKind.human,
    this.sourceSessionID,
    this.displayName,
    this.color,
    this.grants = const [],
  });

  final String id;
  final UIParticipantKind kind;
  final String? sourceSessionID;
  final String? displayName;
  final String? color;
  final List<String> grants;

  factory UIParticipant.fromJson(Map<String, dynamic> json) => UIParticipant(
        id: json['id'] as String,
        kind: json['kind'] == null
            ? UIParticipantKind.human
            : UIParticipantKind.fromJson(json['kind'] as String),
        sourceSessionID: json['sourceSessionId'] as String?,
        displayName: json['displayName'] as String?,
        color: json['color'] as String?,
        grants: ((json['grants'] as List?) ?? [])
            .map((g) => g as String)
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'kind': kind.toJson(),
        if (sourceSessionID != null) 'sourceSessionId': sourceSessionID,
        if (displayName != null) 'displayName': displayName,
        if (color != null) 'color': color,
        'grants': grants,
      };

  @override
  bool operator ==(Object other) =>
      other is UIParticipant &&
      other.id == id &&
      other.kind == kind &&
      other.sourceSessionID == sourceSessionID &&
      other.displayName == displayName &&
      other.color == color &&
      _listEq(other.grants, grants);

  @override
  int get hashCode =>
      Object.hash(id, kind, sourceSessionID, displayName, color, grants.length);
}

/// Mirrors Swift `UIRendererMetadata`.
final class UIRendererMetadata {
  const UIRendererMetadata({
    required this.id,
    required this.kind,
    this.capabilities = const [],
  });

  final String id;
  final String kind;
  final List<String> capabilities;

  factory UIRendererMetadata.fromJson(Map<String, dynamic> json) =>
      UIRendererMetadata(
        id: json['id'] as String,
        kind: json['kind'] as String,
        capabilities: ((json['capabilities'] as List?) ?? [])
            .map((c) => c as String)
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'kind': kind,
        'capabilities': capabilities,
      };

  @override
  bool operator ==(Object other) =>
      other is UIRendererMetadata &&
      other.id == id &&
      other.kind == kind &&
      _listEq(other.capabilities, capabilities);

  @override
  int get hashCode => Object.hash(id, kind, capabilities.length);
}

/// Mirrors Swift `UIRendererState`.
final class UIRendererState {
  const UIRendererState({
    required this.rendererVisible,
    required this.terminalVisible,
  });

  final bool rendererVisible;
  final bool terminalVisible;

  static const terminal =
      UIRendererState(rendererVisible: false, terminalVisible: true);
  static const component =
      UIRendererState(rendererVisible: true, terminalVisible: false);
  static const hidden =
      UIRendererState(rendererVisible: false, terminalVisible: false);

  factory UIRendererState.fromJson(Map<String, dynamic> json) =>
      UIRendererState(
        rendererVisible: json['rendererVisible'] as bool,
        terminalVisible: json['terminalVisible'] as bool,
      );

  Map<String, dynamic> toJson() => {
        'rendererVisible': rendererVisible,
        'terminalVisible': terminalVisible,
      };

  @override
  bool operator ==(Object other) =>
      other is UIRendererState &&
      other.rendererVisible == rendererVisible &&
      other.terminalVisible == terminalVisible;

  @override
  int get hashCode => Object.hash(rendererVisible, terminalVisible);
}

/// Mirrors Swift `UIAttach`. Scoped local attachment. Never expose
/// [participantToken] to web code; [toString] redacts it, mirroring Swift's
/// `CustomDebugStringConvertible` conformance.
final class UIAttach {
  const UIAttach({
    required this.participantToken,
    required this.clientID,
    required this.renderer,
    required this.viewID,
    this.minProtocolVersion = UIProtocol.minimumVersion,
    this.maxProtocolVersion = UIProtocol.maximumVersion,
    this.expectedAppInstanceID,
    this.lastSeenRevision,
    this.state = UIRendererState.terminal,
  });

  final String protocolName = UIProtocol.name;
  final int minProtocolVersion;
  final int maxProtocolVersion;
  final String participantToken;
  final String clientID;
  final UIRendererMetadata renderer;
  final String viewID;
  final String? expectedAppInstanceID;
  final int? lastSeenRevision;
  final UIRendererState state;

  factory UIAttach.fromJson(Map<String, dynamic> json) {
    final minProtocolVersion = json['minProtocolVersion'] as int;
    final maxProtocolVersion = json['maxProtocolVersion'] as int;
    if (!(minProtocolVersion > 0 &&
        minProtocolVersion <= maxProtocolVersion &&
        maxProtocolVersion <= 4294967295)) {
      throw FormatException('Invalid UI protocol version range', json);
    }
    final participantToken = json['participantToken'] as String;
    if (participantToken.isEmpty || _utf8Length(participantToken) > 16384) {
      throw FormatException(
          'participantToken must contain 1...16384 bytes', json);
    }
    return UIAttach(
      participantToken: participantToken,
      clientID: json['clientId'] as String,
      renderer:
          UIRendererMetadata.fromJson(json['renderer'] as Map<String, dynamic>),
      viewID: json['viewId'] as String,
      minProtocolVersion: minProtocolVersion,
      maxProtocolVersion: maxProtocolVersion,
      expectedAppInstanceID: json['expectedAppInstanceId'] as String?,
      lastSeenRevision: json['lastSeenRevision'] as int?,
      state: json['state'] == null
          ? UIRendererState.terminal
          : UIRendererState.fromJson(json['state'] as Map<String, dynamic>),
    );
  }

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'minProtocolVersion': minProtocolVersion,
        'maxProtocolVersion': maxProtocolVersion,
        'participantToken': participantToken,
        'clientId': clientID,
        'renderer': renderer.toJson(),
        'viewId': viewID,
        if (expectedAppInstanceID != null)
          'expectedAppInstanceId': expectedAppInstanceID,
        if (lastSeenRevision != null) 'lastSeenRevision': lastSeenRevision,
        'state': state.toJson(),
      };

  /// Mirrors Swift `debugDescription`: the token is never printed.
  @override
  String toString() =>
      'UIAttach(client: $clientID, participantToken: [REDACTED])';

  @override
  bool operator ==(Object other) =>
      other is UIAttach &&
      other.minProtocolVersion == minProtocolVersion &&
      other.maxProtocolVersion == maxProtocolVersion &&
      other.participantToken == participantToken &&
      other.clientID == clientID &&
      other.renderer == renderer &&
      other.viewID == viewID &&
      other.expectedAppInstanceID == expectedAppInstanceID &&
      other.lastSeenRevision == lastSeenRevision &&
      other.state == state;

  @override
  int get hashCode => Object.hash(
      minProtocolVersion,
      maxProtocolVersion,
      participantToken,
      clientID,
      renderer,
      viewID,
      expectedAppInstanceID,
      lastSeenRevision,
      state);
}

/// Mirrors Swift `UIAttached`.
final class UIAttached {
  const UIAttached({
    required this.protocolVersion,
    required this.minProtocolVersion,
    required this.maxProtocolVersion,
    required this.app,
    required this.appInstanceID,
    required this.participantID,
    required this.clientID,
    required this.rendererID,
    required this.viewID,
    required this.resumed,
    this.currentRevision,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final int minProtocolVersion;
  final int maxProtocolVersion;
  final AppMetadata app;
  final String appInstanceID;
  final String participantID;
  final String clientID;
  final String rendererID;
  final String viewID;
  final bool resumed;
  final int? currentRevision;

  factory UIAttached.fromJson(Map<String, dynamic> json) {
    final protocolVersion = json['protocolVersion'] as int;
    final minProtocolVersion = json['minProtocolVersion'] as int;
    final maxProtocolVersion = json['maxProtocolVersion'] as int;
    if (!(minProtocolVersion > 0 &&
        minProtocolVersion <= maxProtocolVersion &&
        maxProtocolVersion <= 4294967295 &&
        protocolVersion >= minProtocolVersion &&
        protocolVersion <= maxProtocolVersion)) {
      throw FormatException(
          'Selected UI protocol version is outside the server range', json);
    }
    return UIAttached(
      protocolVersion: protocolVersion,
      minProtocolVersion: minProtocolVersion,
      maxProtocolVersion: maxProtocolVersion,
      app: AppMetadata.fromJson(json['app'] as Map<String, dynamic>),
      appInstanceID: json['appInstanceId'] as String,
      participantID: json['participantId'] as String,
      clientID: json['clientId'] as String,
      rendererID: json['rendererId'] as String,
      viewID: json['viewId'] as String,
      resumed: json['resumed'] as bool,
      currentRevision: json['currentRevision'] as int?,
    );
  }

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'minProtocolVersion': minProtocolVersion,
        'maxProtocolVersion': maxProtocolVersion,
        'app': app.toJson(),
        'appInstanceId': appInstanceID,
        'participantId': participantID,
        'clientId': clientID,
        'rendererId': rendererID,
        'viewId': viewID,
        'resumed': resumed,
        if (currentRevision != null) 'currentRevision': currentRevision,
      };

  @override
  bool operator ==(Object other) =>
      other is UIAttached &&
      other.protocolVersion == protocolVersion &&
      other.minProtocolVersion == minProtocolVersion &&
      other.maxProtocolVersion == maxProtocolVersion &&
      other.app == app &&
      other.appInstanceID == appInstanceID &&
      other.participantID == participantID &&
      other.clientID == clientID &&
      other.rendererID == rendererID &&
      other.viewID == viewID &&
      other.resumed == resumed &&
      other.currentRevision == currentRevision;

  @override
  int get hashCode => Object.hash(
      protocolVersion,
      minProtocolVersion,
      maxProtocolVersion,
      app,
      appInstanceID,
      participantID,
      clientID,
      rendererID,
      viewID,
      resumed,
      currentRevision);
}

// ---------------------------------------------------------------------------
// Text primitives (UITextPosition/Range/Selection/Edit) live in
// appkit_protocol_widgets.dart: the delta engine needs them without
// importing this file.

// Events (renderer -> host)
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIEventKind`.
enum UIEventKind {
  activate,
  select,
  change,
  submit,
  cancel,
  command;

  static UIEventKind fromJson(String value) =>
      UIEventKind.values.byName(value);
  String toJson() => name;
}

/// Mirrors Swift `UIEventValue` with its custom type/value JSON dispatch.
sealed class UIEventValue {
  const UIEventValue();

  factory UIEventValue.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'none':
        return const UIEventValueNone();
      case 'bool':
        return UIEventValueBool(json['value'] as bool);
      case 'index':
        return UIEventValueIndex(json['value'] as int);
      case 'integer':
        return UIEventValueInteger(json['value'] as int);
      case 'number':
        return UIEventValueNumber((json['value'] as num).toDouble());
      case 'text':
        return UIEventValueText(json['value'] as String);
      case 'textList':
        return UIEventValueTextList(
            (json['value'] as List).map((e) => e as String).toList());
      case 'textEdit':
        return UIEventValueTextEdit(
            UITextEdit.fromJson(json['value'] as Map<String, dynamic>));
      case 'textSelection':
        return UIEventValueTextSelection(
            UITextSelection.fromJson(json['value'] as Map<String, dynamic>));
      default:
        throw FormatException(
            'Unknown UIEventValue type ${json['type']}', json);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIEventValueNone extends UIEventValue {
  const UIEventValueNone();
  @override
  Map<String, dynamic> toJson() => {'type': 'none'};
  @override
  bool operator ==(Object other) => other is UIEventValueNone;
  @override
  int get hashCode => 0;
}

final class UIEventValueBool extends UIEventValue {
  const UIEventValueBool(this.value);
  final bool value;
  @override
  Map<String, dynamic> toJson() => {'type': 'bool', 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueBool && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIEventValueIndex extends UIEventValue {
  const UIEventValueIndex(this.value);
  final int value;
  @override
  Map<String, dynamic> toJson() => {'type': 'index', 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueIndex && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIEventValueInteger extends UIEventValue {
  const UIEventValueInteger(this.value);
  final int value;
  @override
  Map<String, dynamic> toJson() => {'type': 'integer', 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueInteger && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIEventValueNumber extends UIEventValue {
  const UIEventValueNumber(this.value);
  final double value;
  @override
  Map<String, dynamic> toJson() => {'type': 'number', 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueNumber && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIEventValueText extends UIEventValue {
  const UIEventValueText(this.value);
  final String value;
  @override
  Map<String, dynamic> toJson() => {'type': 'text', 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueText && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIEventValueTextList extends UIEventValue {
  const UIEventValueTextList(this.value);
  final List<String> value;
  @override
  Map<String, dynamic> toJson() => {'type': 'textList', 'value': value};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueTextList && _listEq(other.value, value);
  @override
  int get hashCode => value.length;
}

final class UIEventValueTextEdit extends UIEventValue {
  const UIEventValueTextEdit(this.value);
  final UITextEdit value;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'textEdit', 'value': value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueTextEdit && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIEventValueTextSelection extends UIEventValue {
  const UIEventValueTextSelection(this.value);
  final UITextSelection value;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'textSelection', 'value': value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIEventValueTextSelection && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

/// Mirrors Swift `UIAction`. Renderer-local action; the session transport
/// applies identity and revision.
final class UIAction {
  const UIAction({
    required this.nodeID,
    required this.action,
    required this.kind,
    this.value = const UIEventValueNone(),
  });

  final String nodeID;
  final String action;
  final UIEventKind kind;
  final UIEventValue value;

  factory UIAction.fromJson(Map<String, dynamic> json) => UIAction(
        nodeID: json['nodeId'] as String,
        action: json['action'] as String,
        kind: UIEventKind.fromJson(json['kind'] as String),
        value: UIEventValue.fromJson(json['value'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'nodeId': nodeID,
        'action': action,
        'kind': kind.toJson(),
        'value': value.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIAction &&
      other.nodeID == nodeID &&
      other.action == action &&
      other.kind == kind &&
      other.value == value;

  @override
  int get hashCode => Object.hash(nodeID, action, kind, value);
}

/// Mirrors Swift `UISnapshot`. The root node uses the full [UINode] from
/// `appkit_protocol_lists.dart`.
final class UISnapshot {
  const UISnapshot({
    required this.appInstanceID,
    required this.clientID,
    required this.viewID,
    required this.revision,
    required this.root,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String appInstanceID;
  final String clientID;
  final String viewID;
  final int revision;
  final UINode root;

  factory UISnapshot.fromJson(Map<String, dynamic> json) => UISnapshot(
        protocolVersion: json['protocolVersion'] as int,
        appInstanceID: json['appInstanceId'] as String,
        clientID: json['clientId'] as String,
        viewID: json['viewId'] as String,
        revision: json['revision'] as int,
        root: UINode.fromJson(json['root'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'appInstanceId': appInstanceID,
        'clientId': clientID,
        'viewId': viewID,
        'revision': revision,
        'root': root.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UISnapshot &&
      other.protocolVersion == protocolVersion &&
      other.appInstanceID == appInstanceID &&
      other.clientID == clientID &&
      other.viewID == viewID &&
      other.revision == revision &&
      other.root == root;

  @override
  int get hashCode => Object.hash(
      protocolVersion, appInstanceID, clientID, viewID, revision, root);
}

/// Applies route-matched contiguous deltas, mirroring Swift's
/// `UISnapshot.applying(_:)` from `UIDelta.swift`. Returns complete new
/// state; the input snapshot is never mutated.
extension UIDeltaApplication on UISnapshot {
  UISnapshot applying(UIDelta delta) {
    if (!(protocolName == delta.protocolName &&
        protocolVersion == delta.protocolVersion &&
        appInstanceID == delta.appInstanceID &&
        clientID == delta.clientID &&
        viewID == delta.viewID)) {
      throw UIDeltaApplicationError(
          'Delta route does not match the current snapshot');
    }
    if (!(revision == delta.baseRevision &&
        delta.revision > delta.baseRevision)) {
      throw UIDeltaApplicationError(
          'Delta is not contiguous with the current snapshot');
    }
    if (!(delta.operations.length >= 1 &&
        delta.operations.length <= 4096)) {
      throw UIDeltaApplicationError(
          'Delta must contain 1...4096 operations');
    }
    var node = root;
    for (final operation in delta.operations) {
      node = node.applying(operation);
    }
    final component = node.component;
    if (component is UIComponentMarkdownEditor) {
      final editor = component.editor;
      // Mirrors Swift's post-apply selection sanity check.
      utf16OffsetForPosition(
          UITextPosition(
              line: editor.anchorLine, utf16Column: editor.anchorColumn),
          editor.text);
      utf16OffsetForPosition(
          UITextPosition(line: editor.headLine, utf16Column: editor.headColumn),
          editor.text);
    }
    return UISnapshot(
      protocolVersion: delta.protocolVersion,
      appInstanceID: delta.appInstanceID,
      clientID: delta.clientID,
      viewID: delta.viewID,
      revision: delta.revision,
      root: node,
    );
  }
}

/// Mirrors Swift `UIEvent`.
final class UIEvent {
  const UIEvent({
    required this.appInstanceID,
    required this.participantID,
    required this.clientID,
    required this.rendererID,
    required this.viewID,
    required this.eventID,
    required this.baseRevision,
    required this.action,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String appInstanceID;
  final String participantID;
  final String clientID;
  final String rendererID;
  final String viewID;
  final String eventID;
  final int baseRevision;
  final UIAction action;

  /// Mirrors Swift `UIEvent.init(snapshot:participantID:rendererID:action:)`.
  factory UIEvent.fromSnapshot({
    required UISnapshot snapshot,
    required String participantID,
    required String rendererID,
    required String eventID,
    required UIAction action,
  }) =>
      UIEvent(
        protocolVersion: snapshot.protocolVersion,
        appInstanceID: snapshot.appInstanceID,
        participantID: participantID,
        clientID: snapshot.clientID,
        rendererID: rendererID,
        viewID: snapshot.viewID,
        eventID: eventID,
        baseRevision: snapshot.revision,
        action: action,
      );

  factory UIEvent.fromJson(Map<String, dynamic> json) => UIEvent(
        protocolVersion: json['protocolVersion'] as int,
        appInstanceID: json['appInstanceId'] as String,
        participantID: json['participantId'] as String,
        clientID: json['clientId'] as String,
        rendererID: json['rendererId'] as String,
        viewID: json['viewId'] as String,
        eventID: json['eventId'] as String,
        baseRevision: json['baseRevision'] as int,
        action: UIAction.fromJson(json),
      );

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'appInstanceId': appInstanceID,
        'participantId': participantID,
        'clientId': clientID,
        'rendererId': rendererID,
        'viewId': viewID,
        'eventId': eventID,
        'baseRevision': baseRevision,
        ...action.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIEvent &&
      other.protocolVersion == protocolVersion &&
      other.appInstanceID == appInstanceID &&
      other.participantID == participantID &&
      other.clientID == clientID &&
      other.rendererID == rendererID &&
      other.viewID == viewID &&
      other.eventID == eventID &&
      other.baseRevision == baseRevision &&
      other.action == action;

  @override
  int get hashCode => Object.hash(protocolVersion, appInstanceID,
      participantID, clientID, rendererID, viewID, eventID, baseRevision, action);
}

// ---------------------------------------------------------------------------
// Host -> renderer messages
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIAckStatus`.
enum UIAckStatus {
  pending,
  applied,
  rejected,
  stale;

  static UIAckStatus fromJson(String value) =>
      UIAckStatus.values.byName(value);
  String toJson() => name;
}

/// Mirrors Swift `UIAck`.
final class UIAck {
  const UIAck({
    required this.appInstanceID,
    required this.clientID,
    required this.rendererID,
    required this.viewID,
    required this.eventID,
    required this.status,
    required this.revision,
    this.message,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String appInstanceID;
  final String clientID;
  final String rendererID;
  final String viewID;
  final String eventID;
  final UIAckStatus status;
  final int revision;
  final String? message;

  factory UIAck.fromJson(Map<String, dynamic> json) => UIAck(
        protocolVersion: json['protocolVersion'] as int,
        appInstanceID: json['appInstanceId'] as String,
        clientID: json['clientId'] as String,
        rendererID: json['rendererId'] as String,
        viewID: json['viewId'] as String,
        eventID: json['eventId'] as String,
        status: UIAckStatus.fromJson(json['status'] as String),
        revision: json['revision'] as int,
        message: json['message'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'appInstanceId': appInstanceID,
        'clientId': clientID,
        'rendererId': rendererID,
        'viewId': viewID,
        'eventId': eventID,
        'status': status.toJson(),
        'revision': revision,
        if (message != null) 'message': message,
      };

  @override
  bool operator ==(Object other) =>
      other is UIAck &&
      other.protocolVersion == protocolVersion &&
      other.appInstanceID == appInstanceID &&
      other.clientID == clientID &&
      other.rendererID == rendererID &&
      other.viewID == viewID &&
      other.eventID == eventID &&
      other.status == status &&
      other.revision == revision &&
      other.message == message;

  @override
  int get hashCode => Object.hash(protocolVersion, appInstanceID, clientID,
      rendererID, viewID, eventID, status, revision, message);
}

/// Mirrors Swift `UILifecycle`.
final class UILifecycle {
  const UILifecycle({
    required this.appInstanceID,
    required this.clientID,
    required this.rendererID,
    required this.viewID,
    required this.state,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String appInstanceID;
  final String clientID;
  final String rendererID;
  final String viewID;
  final UIRendererState state;

  /// Mirrors Swift `UILifecycle.init(snapshot:rendererID:state:)`.
  factory UILifecycle.fromSnapshot({
    required UISnapshot snapshot,
    required String rendererID,
    required UIRendererState state,
  }) =>
      UILifecycle(
        protocolVersion: snapshot.protocolVersion,
        appInstanceID: snapshot.appInstanceID,
        clientID: snapshot.clientID,
        rendererID: rendererID,
        viewID: snapshot.viewID,
        state: state,
      );

  factory UILifecycle.fromJson(Map<String, dynamic> json) => UILifecycle(
        protocolVersion: json['protocolVersion'] as int,
        appInstanceID: json['appInstanceId'] as String,
        clientID: json['clientId'] as String,
        rendererID: json['rendererId'] as String,
        viewID: json['viewId'] as String,
        state: UIRendererState.fromJson(json['state'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'appInstanceId': appInstanceID,
        'clientId': clientID,
        'rendererId': rendererID,
        'viewId': viewID,
        'state': state.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UILifecycle &&
      other.protocolVersion == protocolVersion &&
      other.appInstanceID == appInstanceID &&
      other.clientID == clientID &&
      other.rendererID == rendererID &&
      other.viewID == viewID &&
      other.state == state;

  @override
  int get hashCode => Object.hash(protocolVersion, appInstanceID, clientID,
      rendererID, viewID, state);
}

/// Mirrors Swift `UIRequestSnapshot`.
final class UIRequestSnapshot {
  const UIRequestSnapshot({
    required this.appInstanceID,
    required this.clientID,
    required this.rendererID,
    required this.viewID,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String appInstanceID;
  final String clientID;
  final String rendererID;
  final String viewID;

  factory UIRequestSnapshot.fromJson(Map<String, dynamic> json) =>
      UIRequestSnapshot(
        protocolVersion: json['protocolVersion'] as int,
        appInstanceID: json['appInstanceId'] as String,
        clientID: json['clientId'] as String,
        rendererID: json['rendererId'] as String,
        viewID: json['viewId'] as String,
      );

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'appInstanceId': appInstanceID,
        'clientId': clientID,
        'rendererId': rendererID,
        'viewId': viewID,
      };

  @override
  bool operator ==(Object other) =>
      other is UIRequestSnapshot &&
      other.protocolVersion == protocolVersion &&
      other.appInstanceID == appInstanceID &&
      other.clientID == clientID &&
      other.rendererID == rendererID &&
      other.viewID == viewID;

  @override
  int get hashCode => Object.hash(
      protocolVersion, appInstanceID, clientID, rendererID, viewID);
}

/// Mirrors Swift `UIPresenceMember`.
final class UIPresenceMember {
  const UIPresenceMember({
    required this.participant,
    required this.clientID,
    required this.renderer,
    required this.state,
  });

  final UIParticipant participant;
  final String clientID;
  final UIRendererMetadata renderer;
  final UIRendererState state;

  factory UIPresenceMember.fromJson(Map<String, dynamic> json) =>
      UIPresenceMember(
        participant:
            UIParticipant.fromJson(json['participant'] as Map<String, dynamic>),
        clientID: json['clientId'] as String,
        renderer: UIRendererMetadata.fromJson(
            json['renderer'] as Map<String, dynamic>),
        state:
            UIRendererState.fromJson(json['state'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'participant': participant.toJson(),
        'clientId': clientID,
        'renderer': renderer.toJson(),
        'state': state.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIPresenceMember &&
      other.participant == participant &&
      other.clientID == clientID &&
      other.renderer == renderer &&
      other.state == state;

  @override
  int get hashCode =>
      Object.hash(participant, clientID, renderer, state);
}

/// Mirrors Swift `UIPresence`.
final class UIPresence {
  const UIPresence({
    required this.appInstanceID,
    required this.viewID,
    required this.members,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String appInstanceID;
  final String viewID;
  final List<UIPresenceMember> members;

  factory UIPresence.fromJson(Map<String, dynamic> json) => UIPresence(
        protocolVersion: json['protocolVersion'] as int,
        appInstanceID: json['appInstanceId'] as String,
        viewID: json['viewId'] as String,
        members: (json['members'] as List)
            .map((m) =>
                UIPresenceMember.fromJson(m as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'appInstanceId': appInstanceID,
        'viewId': viewID,
        'members': members.map((m) => m.toJson()).toList(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIPresence &&
      other.protocolVersion == protocolVersion &&
      other.appInstanceID == appInstanceID &&
      other.viewID == viewID &&
      _listEq(other.members, members);

  @override
  int get hashCode =>
      Object.hash(protocolVersion, appInstanceID, viewID, members.length);
}

/// Mirrors Swift `UIErrorMessage`.
final class UIErrorMessage {
  const UIErrorMessage({
    required this.code,
    required this.message,
    this.protocolVersion = UIProtocol.version,
  });

  final String protocolName = UIProtocol.name;
  final int protocolVersion;
  final String code;
  final String message;

  factory UIErrorMessage.fromJson(Map<String, dynamic> json) =>
      UIErrorMessage(
        protocolVersion: json['protocolVersion'] as int,
        code: json['code'] as String,
        message: json['message'] as String,
      );

  Map<String, dynamic> toJson() => {
        'protocol': UIProtocol.name,
        'protocolVersion': protocolVersion,
        'code': code,
        'message': message,
      };

  @override
  bool operator ==(Object other) =>
      other is UIErrorMessage &&
      other.protocolVersion == protocolVersion &&
      other.code == code &&
      other.message == message;

  @override
  int get hashCode => Object.hash(protocolVersion, code, message);
}

// ---------------------------------------------------------------------------
// Top-level message envelope
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIMessage`: the type-discriminated top-level wire envelope.
sealed class UIMessage {
  const UIMessage();

  /// The selected connection version, or `nil` for the range-bearing attach.
  /// Mirrors Swift `UIMessage.protocolVersion`.
  int? get protocolVersion;

  factory UIMessage.fromJson(Map<String, dynamic> json) {
    final protocolName = json['protocol'] as String?;
    if (protocolName != UIProtocol.name) {
      throw FormatException('Unsupported UI protocol $protocolName', json);
    }
    final type = json['type'] as String;
    if (type != 'attach') {
      final version = json['protocolVersion'] as int?;
      if (version == null || !UIProtocol.supports(version)) {
        throw FormatException(
            'Unsupported UI protocol version $version', json);
      }
    }
    switch (type) {
      case 'attach':
        return UIMessageAttach(
            UIAttach.fromJson(json));
      case 'attached':
        return UIMessageAttached(
            UIAttached.fromJson(json));
      case 'snapshot':
        return UIMessageSnapshot(
            UISnapshot.fromJson(json));
      case 'delta':
        return UIMessageDelta.fromJson(json);
      case 'event':
        return UIMessageEvent(UIEvent.fromJson(json));
      case 'ack':
        return UIMessageAck(UIAck.fromJson(json));
      case 'lifecycle':
        return UIMessageLifecycle(
            UILifecycle.fromJson(json));
      case 'requestSnapshot':
        return UIMessageRequestSnapshot(
            UIRequestSnapshot.fromJson(json));
      case 'presence':
        return UIMessagePresence(
            UIPresence.fromJson(json));
      case 'error':
        return UIMessageError(
            UIErrorMessage.fromJson(json));
      default:
        throw FormatException('Unknown UI message type $type', json);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIMessageAttach extends UIMessage {
  const UIMessageAttach(this.attach);
  final UIAttach attach;
  @override
  int? get protocolVersion => null;
  @override
  Map<String, dynamic> toJson() => {'type': 'attach', ...attach.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageAttach && other.attach == attach;
  @override
  int get hashCode => attach.hashCode;
}

final class UIMessageAttached extends UIMessage {
  const UIMessageAttached(this.attached);
  final UIAttached attached;
  @override
  int? get protocolVersion => attached.protocolVersion;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'attached', ...attached.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageAttached && other.attached == attached;
  @override
  int get hashCode => attached.hashCode;
}

final class UIMessageSnapshot extends UIMessage {
  const UIMessageSnapshot(this.snapshot);
  final UISnapshot snapshot;
  @override
  int? get protocolVersion => snapshot.protocolVersion;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'snapshot', ...snapshot.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageSnapshot && other.snapshot == snapshot;
  @override
  int get hashCode => snapshot.hashCode;
}

final class UIMessageDelta extends UIMessage {
  const UIMessageDelta(this.delta);

  /// Mirrors Swift `UIMessage.delta(UIDelta)`: the contiguous
  /// server-to-renderer change from `UIDelta.swift`.
  final UIDelta delta;

  factory UIMessageDelta.fromJson(Map<String, dynamic> json) =>
      UIMessageDelta(UIDelta.fromJson(json));

  @override
  int? get protocolVersion => delta.protocolVersion;
  @override
  Map<String, dynamic> toJson() => {'type': 'delta', ...delta.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageDelta && other.delta == delta;
  @override
  int get hashCode => delta.hashCode;
}

final class UIMessageEvent extends UIMessage {
  const UIMessageEvent(this.event);
  final UIEvent event;
  @override
  int? get protocolVersion => event.protocolVersion;
  @override
  Map<String, dynamic> toJson() => {'type': 'event', ...event.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageEvent && other.event == event;
  @override
  int get hashCode => event.hashCode;
}

final class UIMessageAck extends UIMessage {
  const UIMessageAck(this.ack);
  final UIAck ack;
  @override
  int? get protocolVersion => ack.protocolVersion;
  @override
  Map<String, dynamic> toJson() => {'type': 'ack', ...ack.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageAck && other.ack == ack;
  @override
  int get hashCode => ack.hashCode;
}

final class UIMessageLifecycle extends UIMessage {
  const UIMessageLifecycle(this.lifecycle);
  final UILifecycle lifecycle;
  @override
  int? get protocolVersion => lifecycle.protocolVersion;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'lifecycle', ...lifecycle.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageLifecycle && other.lifecycle == lifecycle;
  @override
  int get hashCode => lifecycle.hashCode;
}

final class UIMessageRequestSnapshot extends UIMessage {
  const UIMessageRequestSnapshot(this.request);
  final UIRequestSnapshot request;
  @override
  int? get protocolVersion => request.protocolVersion;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'requestSnapshot', ...request.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageRequestSnapshot && other.request == request;
  @override
  int get hashCode => request.hashCode;
}

final class UIMessagePresence extends UIMessage {
  const UIMessagePresence(this.presence);
  final UIPresence presence;
  @override
  int? get protocolVersion => presence.protocolVersion;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'presence', ...presence.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessagePresence && other.presence == presence;
  @override
  int get hashCode => presence.hashCode;
}

final class UIMessageError extends UIMessage {
  const UIMessageError(this.error);
  final UIErrorMessage error;
  @override
  int? get protocolVersion => error.protocolVersion;
  @override
  Map<String, dynamic> toJson() => {'type': 'error', ...error.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIMessageError && other.error == error;
  @override
  int get hashCode => error.hashCode;
}
