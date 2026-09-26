/// Web event bridge: forwards DOM events back to the host.
///
/// The native app receives `GpuiEvent`s (`{type, id, value, ...}` — see
/// `bin/main.dart` `handleEvent`: `action`, `click`, `input`,
/// `table_selection`). The web renderer has no native host, so this file
/// provides:
///
/// 1. [WebEvent] — the Dart model of a DOM-originated event, with the same
///    shape as `GpuiEvent` (`type`, `id`, `value`). The host can feed these
///    straight into the existing `handleEvent` switch.
/// 2. [webEventBridgeJs] — a dependency-free JS snippet. It listens for
///    clicks on `[data-node-id]` elements and input on `.sc-input`, and
///    POSTs `{type, id, value}` JSON to [endpoint]. Debounces input events.
/// 3. [WebEventHandler] — a tiny Dart dispatcher for tests and for hosts
///    that receive events over another transport (e.g. WebSocket): it maps
///    a decoded event map onto the same cases `handleEvent` handles.
///
/// The default endpoint is `/web/event` (same origin). The host side is
/// expected to accept the POST, convert to `GpuiEvent`, and run it through
/// the normal action/click/input pipeline, then re-render.
///
/// No external URLs are contacted: the bridge only talks to the
/// same-origin endpoint it was served from.
library;

/// A DOM-originated UI event, shaped like `GpuiEvent`.
///
/// - `click`: user clicked the node with [id]. If the node carried
///   `data-action`, [value] holds the action name (`mcp.approve`, ...).
/// - `input`: user typed in the input with [id]; [value] is the field text.
/// - `action`: explicit action dispatch; [value] is the action name.
/// - `table_selection`: user picked a table row; [value] is the row index.
final class WebEvent {
  const WebEvent({required this.type, required this.id, this.value});

  /// Builds from a decoded JSON map (e.g. the POST body the JS bridge
  /// sends). Returns null when the map is not a well-formed event.
  static WebEvent? fromJson(Map<String, Object?> json) {
    final type = json['type']?.toString();
    final id = json['id']?.toString();
    if (type == null || type.isEmpty || id == null || id.isEmpty) {
      return null;
    }
    return WebEvent(
      type: type,
      id: id,
      value: json['value']?.toString(),
    );
  }

  final String type;
  final String id;
  final String? value;

  Map<String, Object?> toJson() => {
        'type': type,
        'id': id,
        if (value != null) 'value': value,
      };
}

/// Dispatches [WebEvent]s to callbacks mirroring `handleEvent` in
/// `bin/main.dart`. Hosts wire the real implementations; tests use fakes.
final class WebEventHandler {
  const WebEventHandler({
    this.onAction,
    this.onClick,
    this.onInput,
    this.onTableSelection,
  });

  final Future<void> Function(String id, String? action)? onAction;
  final Future<void> Function(String id, String? action)? onClick;
  final Future<void> Function(String id, String value)? onInput;
  final Future<void> Function(String id, int row)? onTableSelection;

  /// Returns true when the event was handled, false for unknown types.
  Future<bool> handle(WebEvent event) async {
    switch (event.type) {
      case 'action':
        await onAction?.call(event.id, event.value);
        return true;
      case 'click':
        await onClick?.call(event.id, event.value);
        return true;
      case 'input':
        await onInput?.call(event.id, event.value ?? '');
        return true;
      case 'table_selection':
        final row = int.tryParse(event.value ?? '');
        if (row == null) return false;
        await onTableSelection?.call(event.id, row);
        return true;
      default:
        return false;
    }
  }
}

/// Generates the JS event bridge.
///
/// The snippet:
/// - clicks: any click inside `[data-node-id]` posts
///   `{type:'click', id, value: data-action ?? null}`;
/// - inputs: `input` events on `.sc-input[data-node-id]` are debounced
///   ([inputDebounceMs]) and posted as `{type:'input', id, value}`;
/// - table rows: clicks on `tr[data-row]` inside `table[data-node-id]`
///   post `{type:'table_selection', id, value: row}` — unless the click
///   landed on an interactive element (button, link, input, or a nested
///   node container) inside the row, in which case the element's own
///   click is reported instead.
///
/// Failures to reach [endpoint] are swallowed (the page stays usable and
/// the host's poll loop will refresh state anyway).
String webEventBridgeJs({
  String endpoint = '/web/event',
  int inputDebounceMs = 300,
}) {
  final ep = _jsString(endpoint);
  return '''
(function(){
"use strict";
var ENDPOINT=$ep, DEBOUNCE=$inputDebounceMs, timers={};
function post(ev){
  try{
    fetch(ENDPOINT,{method:"POST",headers:{"Content-Type":"application/json"},
      body:JSON.stringify(ev),keepalive:true}).catch(function(){});
  }catch(e){}
}
document.addEventListener("click",function(e){
  var t=e.target instanceof Element?e.target:null;
  if(!t)return;
  var row=t.closest("tr[data-row]");
  var table=row?row.closest("table[data-node-id]"):null;
  // An interactive element inside a row (button/link/input, or a nested
  // node container that is not the table itself) keeps its own click
  // semantics; only bare row clicks become table_selection.
  var interactive=null;
  if(row){
    var el=t.closest("button,a,input,select,textarea");
    if(el&&row.contains(el))interactive=el;
    if(!interactive){
      var n=t.closest("[data-node-id]");
      if(n&&n!==table&&row.contains(n))interactive=n;
    }
  }
  if(row&&table&&!interactive){
    post({type:"table_selection",id:table.getAttribute("data-node-id"),
      value:row.getAttribute("data-row")});
    return;
  }
  var n=t.closest("[data-node-id]");
  if(!n)return;
  if(n.disabled)return;
  post({type:"click",id:n.getAttribute("data-node-id"),
    value:n.getAttribute("data-action")});
},true);
document.addEventListener("input",function(e){
  var t=e.target;
  if(!(t instanceof HTMLInputElement))return;
  var id=t.getAttribute("data-node-id");
  if(!id)return;
  if(timers[id])clearTimeout(timers[id]);
  timers[id]=setTimeout(function(){
    delete timers[id];
    post({type:"input",id:id,value:t.value});
  },DEBOUNCE);
},true);
})();
''';
}

/// Escapes a Dart string for embedding as a JS double-quoted string.
String _jsString(String s) => '"${s
    .replaceAll(r'\', r'\\')
    .replaceAll('"', r'\"')
    .replaceAll('\n', r'\n')
    .replaceAll('\r', r'\r')
    .replaceAll('<', r'\x3c')}"';
