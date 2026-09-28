/// App-kit UI protocol ported from Swift to Dart.
///
/// Port of `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`
/// (4216 lines), `UIDelta.swift` (1336 lines), `UIParticipantToken.swift`
/// (163 lines), and the wire types of `UIUnixSessionClient.swift` (513 lines).
///
/// The Swift original is a server-driven UI protocol: the Host sends
/// [AppKitNode] JSON (a `type`-discriminated component), the client decodes it
/// and renders native widgets. This file provides the Dart decode side;
/// [AppKitRenderer] (in `appkit_renderer.dart`) maps components to gpuidart
/// [UiNode] trees using the widgets in `appkit_widgets.dart`.
///
/// Wire compatibility: JSON keys match the Swift `CodingKeys` exactly
/// (`selectedId`, `sourceSessionId`, …). Unknown component `type` values decode
/// to [AppKitComponent.unsupported] instead of throwing, mirroring the Swift
/// `unknownComponentDecodesForTerminalFallbackWithoutRejectingAttachment`
/// behavior.
///
/// NOT ported (documented gaps):
/// - `UIUnixSessionClient` transport: Dart uses [HostClient] (HTTP) instead of
///   the Unix-domain socket. See `docs/gpuidart-gaps-appkit.md` GAP-APPKIT-1.
/// - Capability negotiation (`requiredCapabilities`): the Dart renderer assumes
///   the Host only sends supported components. GAP-APPKIT-2.
/// - `UIDelta` incremental application: the Dart renderer re-renders full
///   snapshots. GAP-APPKIT-3.
library;

import 'dart:convert';

/// Protocol identity. Mirrors `SupercliUIProtocol`.
abstract final class AppKitProtocol {
  static const name = 'supercli.ui';
  static const version = 1;
  static const minimumVersion = 1;
  static const maximumVersion = 1;

  static const pageCapability = 'page';
  static const listCapability = 'list';
  static const listItemCapability = 'listItem';
  static const gaugeCapability = 'gauge';
  static const badgeCapability = 'badge';
  static const statusSymbolCapability = 'statusSymbol';
  static const toggleCapability = 'toggle';
  static const inputCapability = 'input';
  static const buttonCapability = 'button';
  static const contentCapability = 'content';
  static const treeCapability = 'tree';
  static const textBoxCapability = 'textBox';
  static const mediaCapability = 'media';
  static const menuCapability = 'menu';
  static const footerActionsCapability = 'footerActions';
  static const footerStatusCapability = 'footerStatus';

  static bool supports(int version) =>
      version >= minimumVersion && version <= maximumVersion;

  /// Mirrors `SupercliUIProtocol.negotiate(minimum:maximum:)`.
  static int? negotiate({required int minimum, required int maximum}) {
    if (minimum <= 0 || minimum > maximum) return null;
    final lo = minimum > minimumVersion ? minimum : minimumVersion;
    final hi = maximum < maximumVersion ? maximum : maximumVersion;
    return lo <= hi ? hi : null;
  }
}

/// A decoded UI node: an id plus a type-discriminated component.
///
/// Mirrors Swift `UINode`. JSON shape: `{"id": "...", "type": "page", ...}`.
final class AppKitNode {
  const AppKitNode({required this.id, required this.component});

  final String id;
  final AppKitComponent component;

  factory AppKitNode.fromJson(Map<String, dynamic> json) {
    final id = json['id'] as String;
    final type = json['type'] as String;
    return AppKitNode(id: id, component: AppKitComponent.fromJson(type, json));
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'type': component.kind,
        ...component.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is AppKitNode && other.id == id && other.component == component;

  @override
  int get hashCode => Object.hash(id, component);

  @override
  String toString() => 'AppKitNode(id: $id, component: ${component.kind})';
}

/// The component union. Mirrors Swift `UIComponent`.
sealed class AppKitComponent {
  const AppKitComponent();

  String get kind;

  factory AppKitComponent.fromJson(String type, Map<String, dynamic> json) {
    switch (type) {
      case 'page':
        return AppKitPage(PageSpec.fromJson(json));
      case 'list':
        return AppKitList(ListSpec.fromJson(json));
      case 'content':
        return AppKitContent(ContentSpec.fromJson(json));
      case 'tree':
        return AppKitTree(TreeSpec.fromJson(json));
      case 'textBox':
        return AppKitTextBox(TextBoxSpec.fromJson(json));
      case 'media':
        return AppKitMedia(MediaSpec.fromJson(json));
      case 'menu':
        return AppKitMenu(MenuSpec.fromJson(json));
      case 'markdownEditor':
        return AppKitMarkdownEditor(MarkdownEditorSpec.fromJson(json));
      case 'canvasPage':
        return AppKitCanvasPage(CanvasPageSpec.fromJson(json));
      case 'surface':
        return AppKitSurface(SurfaceSpec.fromJson(json));
      default:
        return AppKitUnsupported(type);
    }
  }

  Map<String, dynamic> toJson();
}

final class AppKitPage extends AppKitComponent {
  const AppKitPage(this.spec);
  final PageSpec spec;
  @override
  String get kind => 'page';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitPage && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitList extends AppKitComponent {
  const AppKitList(this.spec);
  final ListSpec spec;
  @override
  String get kind => 'list';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitList && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitContent extends AppKitComponent {
  const AppKitContent(this.spec);
  final ContentSpec spec;
  @override
  String get kind => 'content';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitContent && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitTree extends AppKitComponent {
  const AppKitTree(this.spec);
  final TreeSpec spec;
  @override
  String get kind => 'tree';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitTree && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitTextBox extends AppKitComponent {
  const AppKitTextBox(this.spec);
  final TextBoxSpec spec;
  @override
  String get kind => 'textBox';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitTextBox && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitMedia extends AppKitComponent {
  const AppKitMedia(this.spec);
  final MediaSpec spec;
  @override
  String get kind => 'media';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitMedia && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitMenu extends AppKitComponent {
  const AppKitMenu(this.spec);
  final MenuSpec spec;
  @override
  String get kind => 'menu';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitMenu && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitMarkdownEditor extends AppKitComponent {
  const AppKitMarkdownEditor(this.spec);
  final MarkdownEditorSpec spec;
  @override
  String get kind => 'markdownEditor';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitMarkdownEditor && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitCanvasPage extends AppKitComponent {
  const AppKitCanvasPage(this.spec);
  final CanvasPageSpec spec;
  @override
  String get kind => 'canvasPage';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitCanvasPage && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

final class AppKitSurface extends AppKitComponent {
  const AppKitSurface(this.spec);
  final SurfaceSpec spec;
  @override
  String get kind => 'surface';
  @override
  Map<String, dynamic> toJson() => spec.toJson();
  @override
  bool operator ==(Object other) =>
      other is AppKitSurface && other.spec == spec;
  @override
  int get hashCode => spec.hashCode;
}

/// Unknown component kinds decode here instead of throwing, so the client can
/// fall back to terminal rendering. Mirrors Swift `.unsupported(kind:)`.
final class AppKitUnsupported extends AppKitComponent {
  const AppKitUnsupported(this.unsupportedKind);
  final String unsupportedKind;
  @override
  String get kind => unsupportedKind;
  @override
  Map<String, dynamic> toJson() => {};
  @override
  bool operator ==(Object other) =>
      other is AppKitUnsupported && other.unsupportedKind == unsupportedKind;
  @override
  int get hashCode => unsupportedKind.hashCode;
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

/// Mirrors Swift `PageSpec`.
final class PageSpec {
  const PageSpec({
    required this.title,
    this.tabs = const [],
    this.toolbar,
    this.back,
    this.body = const PageBodyList(ListSpec(id: '', items: [])),
    this.footer = const FooterActionsSpec(),
  });

  final String title;
  final List<PageTab> tabs;
  final PageToolbar? toolbar;
  final String? back;
  final PageBody body;
  final FooterActionsSpec footer;

  factory PageSpec.fromJson(Map<String, dynamic> json) {
    return PageSpec(
      title: json['title'] as String,
      tabs: ((json['tabs'] as List?) ?? [])
          .map((t) => PageTab.fromJson(t as Map<String, dynamic>))
          .toList(),
      toolbar: json['toolbar'] == null
          ? null
          : PageToolbar.fromJson(json['toolbar'] as Map<String, dynamic>),
      back: json['back'] as String?,
      body: PageBody.fromJson(json['body'] as Map<String, dynamic>? ?? {}),
      footer: json['footer'] == null
          ? const FooterActionsSpec()
          : FooterActionsSpec.fromJson(
              json['footer'] as Map<String, dynamic>),
    );
  }

  Map<String, dynamic> toJson() => {
        'title': title,
        'tabs': tabs.map((t) => t.toJson()).toList(),
        if (toolbar != null) 'toolbar': toolbar!.toJson(),
        if (back != null) 'back': back,
        'body': body.toJson(),
        'footer': footer.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is PageSpec &&
      other.title == title &&
      _listEq(other.tabs, tabs) &&
      other.back == back &&
      other.body == body &&
      other.footer == footer;

  @override
  int get hashCode => Object.hash(title, back, body, footer);
}

/// Page body slot. Mirrors Swift `UIPageBodySlot`.
sealed class PageBody {
  const PageBody();

  factory PageBody.fromJson(Map<String, dynamic> json) {
    final type = json['type'] as String?;
    switch (type) {
      case 'list':
        return PageBodyList(
            ListSpec.fromJson(json['list'] as Map<String, dynamic>? ?? json));
      case 'content':
        return PageBodyContent(ContentSpec.fromJson(
            json['content'] as Map<String, dynamic>? ?? json));
      case 'gauge':
        return PageBodyGauge(GaugeSpec.fromJson(json));
      default:
        // The Swift body also supports sparkline/barChart/lineChart; the Dart
        // renderer falls back to unsupported for chart bodies (GAP-APPKIT-4).
        if (type == null && json.containsKey('items')) {
          return PageBodyList(ListSpec.fromJson(json));
        }
        return PageBodyUnsupported(type ?? 'unknown');
    }
  }

  Map<String, dynamic> toJson();
}

final class PageBodyList extends PageBody {
  const PageBodyList(this.list);
  final ListSpec list;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'list', 'list': list.toJson()};
  @override
  bool operator ==(Object other) =>
      other is PageBodyList && other.list == list;
  @override
  int get hashCode => list.hashCode;
}

final class PageBodyContent extends PageBody {
  const PageBodyContent(this.content);
  final ContentSpec content;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'content', 'content': content.toJson()};
  @override
  bool operator ==(Object other) =>
      other is PageBodyContent && other.content == content;
  @override
  int get hashCode => content.hashCode;
}

final class PageBodyGauge extends PageBody {
  const PageBodyGauge(this.gauge);
  final GaugeSpec gauge;
  @override
  Map<String, dynamic> toJson() => {'type': 'gauge', ...gauge.toJson()};
  @override
  bool operator ==(Object other) =>
      other is PageBodyGauge && other.gauge == gauge;
  @override
  int get hashCode => gauge.hashCode;
}

final class PageBodyUnsupported extends PageBody {
  const PageBodyUnsupported(this.bodyKind);
  final String bodyKind;
  @override
  Map<String, dynamic> toJson() => {'type': bodyKind};
  @override
  bool operator ==(Object other) =>
      other is PageBodyUnsupported && other.bodyKind == bodyKind;
  @override
  int get hashCode => bodyKind.hashCode;
}

/// Mirrors Swift `UIPageTab`.
final class PageTab {
  const PageTab({
    required this.id,
    required this.label,
    required this.action,
    this.selected = false,
  });

  final String id;
  final String label;
  final String action;
  final bool selected;

  factory PageTab.fromJson(Map<String, dynamic> json) => PageTab(
        id: json['id'] as String,
        label: json['label'] as String,
        action: json['action'] as String,
        selected: json['selected'] as bool? ?? false,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'action': action,
        'selected': selected,
      };

  @override
  bool operator ==(Object other) =>
      other is PageTab &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.selected == selected;

  @override
  int get hashCode => Object.hash(id, label, action, selected);
}

/// Mirrors Swift `UIPageToolbar` (subset: title + actions).
final class PageToolbar {
  const PageToolbar({this.title = '', this.actions = const []});

  final String title;
  final List<String> actions;

  factory PageToolbar.fromJson(Map<String, dynamic> json) => PageToolbar(
        title: json['title'] as String? ?? '',
        actions: ((json['actions'] as List?) ?? []).cast<String>(),
      );

  Map<String, dynamic> toJson() => {'title': title, 'actions': actions};

  @override
  bool operator ==(Object other) =>
      other is PageToolbar &&
      other.title == title &&
      _listEq(other.actions, actions);

  @override
  int get hashCode => Object.hash(title, actions.length);
}

/// Mirrors Swift `UIFooterActionsSpec`.
final class FooterActionsSpec {
  const FooterActionsSpec({this.actions = const [], this.status});

  final List<FooterActionSpec> actions;
  final String? status;

  bool get isEmpty => actions.isEmpty && status == null;

  factory FooterActionsSpec.fromJson(Map<String, dynamic> json) =>
      FooterActionsSpec(
        actions: ((json['actions'] as List?) ?? [])
            .map((a) => FooterActionSpec.fromJson(a as Map<String, dynamic>))
            .toList(),
        status: json['status'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'actions': actions.map((a) => a.toJson()).toList(),
        if (status != null) 'status': status,
      };

  @override
  bool operator ==(Object other) =>
      other is FooterActionsSpec &&
      _listEq(other.actions, actions) &&
      other.status == status;

  @override
  int get hashCode => Object.hash(actions.length, status);
}

/// Mirrors Swift `UIFooterActionSpec`.
final class FooterActionSpec {
  const FooterActionSpec({
    required this.id,
    required this.label,
    required this.action,
    this.busy = false,
    this.disabled = false,
  });

  final String id;
  final String label;
  final String action;
  final bool busy;
  final bool disabled;

  factory FooterActionSpec.fromJson(Map<String, dynamic> json) =>
      FooterActionSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        action: json['action'] as String,
        busy: json['busy'] as bool? ?? false,
        disabled: json['disabled'] as bool? ?? false,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'action': action,
        'busy': busy,
        'disabled': disabled,
      };

  @override
  bool operator ==(Object other) =>
      other is FooterActionSpec &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.busy == busy &&
      other.disabled == disabled;

  @override
  int get hashCode => Object.hash(id, label, action, busy, disabled);
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIListSpec`.
final class ListSpec {
  const ListSpec({
    required this.id,
    required this.items,
    this.emptyMessage = '',
    this.selectedId,
    this.rowLayout = const ListRowLayout.auto(),
  });

  final String id;
  final List<ListItemSpec> items;
  final String emptyMessage;
  final String? selectedId;
  final ListRowLayout rowLayout;

  factory ListSpec.fromJson(Map<String, dynamic> json) => ListSpec(
        id: json['id'] as String,
        items: ((json['items'] as List?) ?? [])
            .map((i) => ListItemSpec.fromJson(i as Map<String, dynamic>))
            .toList(),
        emptyMessage: json['emptyMessage'] as String? ?? '',
        selectedId: json['selectedId'] as String?,
        rowLayout: ListRowLayout.fromJson(json['rowLayout']),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'items': items.map((i) => i.toJson()).toList(),
        'emptyMessage': emptyMessage,
        if (selectedId != null) 'selectedId': selectedId,
        'rowLayout': rowLayout.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is ListSpec &&
      other.id == id &&
      _listEq(other.items, items) &&
      other.emptyMessage == emptyMessage &&
      other.selectedId == selectedId &&
      other.rowLayout == rowLayout;

  @override
  int get hashCode => Object.hash(id, items.length, selectedId, rowLayout);
}

/// How a List lays out each row. Mirrors Swift `UIListRowLayout`.
sealed class ListRowLayout {
  const ListRowLayout();

  factory ListRowLayout.fromJson(dynamic json) {
    if (json == null) return const ListRowLayout.auto();
    final map = json as Map<String, dynamic>;
    final type = map['type'] as String?;
    switch (type) {
      case 'inline':
        return const ListRowLayout.inline();
      case 'stacked':
        return const ListRowLayout.stacked();
      case 'auto':
        return ListRowLayout.auto(
            stackBelowWidth: map['stackBelowWidth'] as int? ?? 60);
      default:
        return const ListRowLayout.auto();
    }
  }

  Map<String, dynamic> toJson();

  const factory ListRowLayout.inline() = _RowLayoutInline;
  const factory ListRowLayout.stacked() = _RowLayoutStacked;
  const factory ListRowLayout.auto({int stackBelowWidth}) = _RowLayoutAuto;
}

final class _RowLayoutInline extends ListRowLayout {
  const _RowLayoutInline();
  @override
  Map<String, dynamic> toJson() => {'type': 'inline'};
  @override
  bool operator ==(Object other) => other is _RowLayoutInline;
  @override
  int get hashCode => 'inline'.hashCode;
}

final class _RowLayoutStacked extends ListRowLayout {
  const _RowLayoutStacked();
  @override
  Map<String, dynamic> toJson() => {'type': 'stacked'};
  @override
  bool operator ==(Object other) => other is _RowLayoutStacked;
  @override
  int get hashCode => 'stacked'.hashCode;
}

final class _RowLayoutAuto extends ListRowLayout {
  const _RowLayoutAuto({this.stackBelowWidth = 60});
  final int stackBelowWidth;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'auto', 'stackBelowWidth': stackBelowWidth};
  @override
  bool operator ==(Object other) =>
      other is _RowLayoutAuto && other.stackBelowWidth == stackBelowWidth;
  @override
  int get hashCode => stackBelowWidth.hashCode;
}

/// Mirrors Swift `UIListItemSpec` (render-relevant subset).
final class ListItemSpec {
  const ListItemSpec({
    required this.id,
    required this.label,
    this.detail,
    this.value,
    this.done = false,
    this.busy = false,
    this.leading,
    this.trailing,
    this.accessory,
    this.top,
    this.bottom,
    this.media,
    this.divider = false,
    this.activate,
  });

  final String id;
  final String label;
  final String? detail;
  final String? value;
  final bool done;
  final bool busy;
  final ListItemSlot? leading;
  final ListItemSlot? trailing;
  final ListItemSlot? accessory;
  final ListItemBand? top;
  final ListItemBand? bottom;
  final ListItemMedia? media;
  final bool divider;
  final String? activate;

  factory ListItemSpec.fromJson(Map<String, dynamic> json) {
    // Swift validates single-line label/detail/value; mirror it.
    for (final key in ['label', 'detail', 'value']) {
      final v = json[key] as String?;
      if (v != null && (v.contains('\n') || v.contains('\r'))) {
        throw FormatException(
            'ListItem $key must be single-line', json);
      }
    }
    return ListItemSpec(
      id: json['id'] as String,
      label: json['label'] as String,
      detail: json['detail'] as String?,
      value: json['value'] as String?,
      done: json['done'] as bool? ?? false,
      busy: json['busy'] as bool? ?? false,
      leading: _slotFromJson(json['leading']),
      trailing: _slotFromJson(json['trailing']),
      accessory: _slotFromJson(json['accessory']),
      top: _bandFromJson(json['top']),
      bottom: _bandFromJson(json['bottom']),
      media: json['media'] == null
          ? null
          : ListItemMedia.fromJson(json['media'] as Map<String, dynamic>),
      divider: json['divider'] as bool? ?? false,
      activate: json['activate'] as String?,
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        if (detail != null) 'detail': detail,
        if (value != null) 'value': value,
        'done': done,
        'busy': busy,
        if (leading != null) 'leading': leading!.toJson(),
        if (trailing != null) 'trailing': trailing!.toJson(),
        if (accessory != null) 'accessory': accessory!.toJson(),
        if (top != null) 'top': top!.toJson(),
        if (bottom != null) 'bottom': bottom!.toJson(),
        if (media != null) 'media': media!.toJson(),
        'divider': divider,
        if (activate != null) 'activate': activate,
      };

  @override
  bool operator ==(Object other) =>
      other is ListItemSpec &&
      other.id == id &&
      other.label == label &&
      other.detail == detail &&
      other.value == value &&
      other.done == done &&
      other.busy == busy &&
      other.leading == leading &&
      other.trailing == trailing &&
      other.top == top &&
      other.bottom == bottom &&
      other.divider == divider &&
      other.activate == activate;

  @override
  int get hashCode => Object.hash(id, label, detail, value, divider);
}

ListItemSlot? _slotFromJson(dynamic json) {
  if (json == null) return null;
  final map = json as Map<String, dynamic>;
  return ListItemSlot.fromJson(map);
}

ListItemBand? _bandFromJson(dynamic json) {
  if (json == null) return null;
  final map = json as Map<String, dynamic>;
  return ListItemBand.fromJson(map);
}

/// Mirrors Swift `UIListItemSlot`.
sealed class ListItemSlot {
  const ListItemSlot();

  String get kind;

  factory ListItemSlot.fromJson(Map<String, dynamic> json) {
    final type = json['type'] as String;
    switch (type) {
      case 'gauge':
        return SlotGauge(GaugeSpec.fromJson(json));
      case 'badge':
        return SlotBadge(BadgeSpec.fromJson(json));
      case 'status':
        return SlotStatus(StatusSymbolSpec.fromJson(json));
      case 'toggle':
        return SlotToggle(
            id: json['id'] as String? ?? '',
            on: json['on'] as bool? ?? false);
      case 'sparkline':
        return SlotSparkline(
            values: ((json['values'] as List?) ?? []).cast<num>());
      case 'disclosure':
        return const SlotDisclosure();
      case 'checkmark':
        return SlotCheckmark(checked: json['checked'] as bool? ?? false);
      default:
        return SlotUnsupported(type);
    }
  }

  Map<String, dynamic> toJson();
}

final class SlotGauge extends ListItemSlot {
  const SlotGauge(this.gauge);
  final GaugeSpec gauge;
  @override
  String get kind => 'gauge';
  @override
  Map<String, dynamic> toJson() => {'type': 'gauge', ...gauge.toJson()};
  @override
  bool operator ==(Object other) =>
      other is SlotGauge && other.gauge == gauge;
  @override
  int get hashCode => gauge.hashCode;
}

final class SlotBadge extends ListItemSlot {
  const SlotBadge(this.badge);
  final BadgeSpec badge;
  @override
  String get kind => 'badge';
  @override
  Map<String, dynamic> toJson() => {'type': 'badge', ...badge.toJson()};
  @override
  bool operator ==(Object other) =>
      other is SlotBadge && other.badge == badge;
  @override
  int get hashCode => badge.hashCode;
}

final class SlotStatus extends ListItemSlot {
  const SlotStatus(this.status);
  final StatusSymbolSpec status;
  @override
  String get kind => 'status';
  @override
  Map<String, dynamic> toJson() => {'type': 'status', ...status.toJson()};
  @override
  bool operator ==(Object other) =>
      other is SlotStatus && other.status == status;
  @override
  int get hashCode => status.hashCode;
}

final class SlotToggle extends ListItemSlot {
  const SlotToggle({required this.id, required this.on});
  final String id;
  final bool on;
  @override
  String get kind => 'toggle';
  @override
  Map<String, dynamic> toJson() => {'type': 'toggle', 'id': id, 'on': on};
  @override
  bool operator ==(Object other) =>
      other is SlotToggle && other.id == id && other.on == on;
  @override
  int get hashCode => Object.hash(id, on);
}

final class SlotSparkline extends ListItemSlot {
  const SlotSparkline({required this.values});
  final List<num> values;
  @override
  String get kind => 'sparkline';
  @override
  Map<String, dynamic> toJson() => {'type': 'sparkline', 'values': values};
  @override
  bool operator ==(Object other) =>
      other is SlotSparkline && _listEq(other.values, values);
  @override
  int get hashCode => values.length;
}

final class SlotDisclosure extends ListItemSlot {
  const SlotDisclosure();
  @override
  String get kind => 'disclosure';
  @override
  Map<String, dynamic> toJson() => {'type': 'disclosure'};
  @override
  bool operator ==(Object other) => other is SlotDisclosure;
  @override
  int get hashCode => 'disclosure'.hashCode;
}

final class SlotCheckmark extends ListItemSlot {
  const SlotCheckmark({required this.checked});
  final bool checked;
  @override
  String get kind => 'checkmark';
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'checkmark', 'checked': checked};
  @override
  bool operator ==(Object other) =>
      other is SlotCheckmark && other.checked == checked;
  @override
  int get hashCode => checked.hashCode;
}

final class SlotUnsupported extends ListItemSlot {
  const SlotUnsupported(this.slotKind);
  final String slotKind;
  @override
  String get kind => slotKind;
  @override
  Map<String, dynamic> toJson() => {'type': slotKind};
  @override
  bool operator ==(Object other) =>
      other is SlotUnsupported && other.slotKind == slotKind;
  @override
  int get hashCode => slotKind.hashCode;
}

/// Full-width band above/below a list item's text rows.
/// Mirrors Swift `UIListItemBand` (gauge/text subset).
final class ListItemBand {
  const ListItemBand._(
      {required this.id, required this.kind, this.gauge, this.text});

  factory ListItemBand.gauge({required String id, required GaugeSpec gauge}) =>
      ListItemBand._(id: id, kind: 'gauge', gauge: gauge);
  factory ListItemBand.text(
          {required String id, required String text, String tone = 'default'}) =>
      ListItemBand._(id: id, kind: 'text', text: text);

  final String id;
  final String kind;
  final GaugeSpec? gauge;
  final String? text;

  factory ListItemBand.fromJson(Map<String, dynamic> json) {
    final type = json['type'] as String;
    final id = json['id'] as String? ?? '';
    switch (type) {
      case 'gauge':
        return ListItemBand.gauge(id: id, gauge: GaugeSpec.fromJson(json));
      default:
        return ListItemBand.text(
            id: id, text: json['text'] as String? ?? '');
    }
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'type': kind,
        if (gauge != null) ...gauge!.toJson(),
        if (text != null) 'text': text,
      };

  @override
  bool operator ==(Object other) =>
      other is ListItemBand &&
      other.id == id &&
      other.kind == kind &&
      other.gauge == gauge &&
      other.text == text;

  @override
  int get hashCode => Object.hash(id, kind, gauge, text);
}

/// Media column spanning a list item. Mirrors Swift `UIListItemMedia`.
final class ListItemMedia {
  const ListItemMedia({
    this.side = 'trailing',
    this.width = 4,
    this.glyph = '',
    this.tone = 'default',
  });

  final String side;
  final int width;
  final String glyph;
  final String tone;

  factory ListItemMedia.fromJson(Map<String, dynamic> json) => ListItemMedia(
        side: json['side'] as String? ?? 'trailing',
        width: json['width'] as int? ?? 4,
        glyph: json['glyph'] as String? ?? '',
        tone: json['tone'] as String? ?? 'default',
      );

  Map<String, dynamic> toJson() =>
      {'side': side, 'width': width, 'glyph': glyph, 'tone': tone};

  @override
  bool operator ==(Object other) =>
      other is ListItemMedia &&
      other.side == side &&
      other.width == width &&
      other.glyph == glyph &&
      other.tone == tone;

  @override
  int get hashCode => Object.hash(side, width, glyph, tone);
}

/// Mirrors Swift `UIGaugeSpec`, including `percentageLabel`/`valueLabel`.
final class GaugeSpec {
  const GaugeSpec({
    required this.id,
    required this.ratio,
    required this.label,
    this.caption,
    required this.accessibilityText,
    this.activate,
  });

  final String id;
  final double ratio;
  final String label;
  final String? caption;
  final String accessibilityText;
  final String? activate;

  /// Mirrors Swift `percentageValueLabel`: `Int((ratio * 100).rounded())%`.
  String get percentageValueLabel => '${(ratio * 100).round()}%';

  /// Mirrors Swift `valueLabel`: caption ?? percentage.
  String get valueLabel => caption ?? percentageValueLabel;

  /// Mirrors Swift `percentageLabel`: `"\(label)  \(valueLabel)"`.
  String get percentageLabel => '$label  $valueLabel';

  bool get isValid =>
      id.isNotEmpty &&
      ratio.isFinite &&
      ratio >= 0 &&
      ratio <= 1 &&
      label.trim().isNotEmpty &&
      accessibilityText.trim().isNotEmpty;

  factory GaugeSpec.fromJson(Map<String, dynamic> json) {
    final spec = GaugeSpec(
      id: json['id'] as String,
      ratio: (json['ratio'] as num).toDouble(),
      label: json['label'] as String,
      caption: json['caption'] as String?,
      accessibilityText: json['accessibilityText'] as String,
      activate: json['activate'] as String?,
    );
    if (!spec.isValid) {
      throw FormatException(
          'Gauge ratio must be 0...1 with label and accessibility text',
          json);
    }
    return spec;
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'ratio': ratio,
        'label': label,
        if (caption != null) 'caption': caption,
        'accessibilityText': accessibilityText,
        if (activate != null) 'activate': activate,
      };

  @override
  bool operator ==(Object other) =>
      other is GaugeSpec &&
      other.id == id &&
      other.ratio == ratio &&
      other.label == label &&
      other.caption == caption &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode => Object.hash(id, ratio, label, accessibilityText);
}

/// Mirrors Swift `UIBadgeSpec`.
final class BadgeSpec {
  const BadgeSpec({required this.text, this.tone = 'default'});

  final String text;
  final String tone;

  factory BadgeSpec.fromJson(Map<String, dynamic> json) => BadgeSpec(
        text: json['text'] as String? ?? '',
        tone: json['tone'] as String? ?? 'default',
      );

  Map<String, dynamic> toJson() => {'text': text, 'tone': tone};

  @override
  bool operator ==(Object other) =>
      other is BadgeSpec && other.text == text && other.tone == tone;

  @override
  int get hashCode => Object.hash(text, tone);
}

/// Mirrors Swift `UIStatusSymbolSpec`.
final class StatusSymbolSpec {
  const StatusSymbolSpec({required this.symbol, this.tone = 'default'});

  final String symbol;
  final String tone;

  factory StatusSymbolSpec.fromJson(Map<String, dynamic> json) =>
      StatusSymbolSpec(
        symbol: json['symbol'] as String? ?? '',
        tone: json['tone'] as String? ?? 'default',
      );

  Map<String, dynamic> toJson() => {'symbol': symbol, 'tone': tone};

  @override
  bool operator ==(Object other) =>
      other is StatusSymbolSpec &&
      other.symbol == symbol &&
      other.tone == tone;

  @override
  int get hashCode => Object.hash(symbol, tone);
}

// ---------------------------------------------------------------------------
// Content (markdown)
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIContentSpec`.
final class ContentSpec {
  const ContentSpec({
    required this.id,
    required this.label,
    this.lines = const [],
    this.emptyMessage = '',
  });

  final String id;
  final String label;
  final List<ContentLine> lines;
  final String emptyMessage;

  factory ContentSpec.fromJson(Map<String, dynamic> json) => ContentSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        lines: ((json['lines'] as List?) ?? [])
            .map((l) => ContentLine.fromJson(l as Map<String, dynamic>))
            .toList(),
        emptyMessage: json['emptyMessage'] as String? ?? '',
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'lines': lines.map((l) => l.toJson()).toList(),
        'emptyMessage': emptyMessage,
      };

  @override
  bool operator ==(Object other) =>
      other is ContentSpec &&
      other.id == id &&
      other.label == label &&
      _listEq(other.lines, lines);

  @override
  int get hashCode => Object.hash(id, label, lines.length);
}

/// Mirrors Swift `UIContentLine`.
final class ContentLine {
  const ContentLine({
    required this.id,
    required this.text,
    this.tone = 'default',
  });

  final String id;
  final String text;
  final String tone;

  factory ContentLine.fromJson(Map<String, dynamic> json) => ContentLine(
        id: json['id'] as String,
        text: json['text'] as String? ?? '',
        tone: json['tone'] as String? ?? 'default',
      );

  Map<String, dynamic> toJson() => {'id': id, 'text': text, 'tone': tone};

  @override
  bool operator ==(Object other) =>
      other is ContentLine &&
      other.id == id &&
      other.text == text &&
      other.tone == tone;

  @override
  int get hashCode => Object.hash(id, text, tone);
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

/// Mirrors Swift `UITreeSpec` (render subset).
final class TreeSpec {
  const TreeSpec({
    required this.id,
    required this.items,
    this.emptyMessage = '',
  });

  final String id;
  final List<TreeItemSpec> items;
  final String emptyMessage;

  factory TreeSpec.fromJson(Map<String, dynamic> json) => TreeSpec(
        id: json['id'] as String,
        items: ((json['items'] as List?) ?? [])
            .map((i) => TreeItemSpec.fromJson(i as Map<String, dynamic>))
            .toList(),
        emptyMessage: json['emptyMessage'] as String? ?? '',
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'items': items.map((i) => i.toJson()).toList(),
        'emptyMessage': emptyMessage,
      };

  @override
  bool operator ==(Object other) =>
      other is TreeSpec &&
      other.id == id &&
      _listEq(other.items, items);

  @override
  int get hashCode => Object.hash(id, items.length);
}

/// Mirrors Swift `UITreeItem`.
final class TreeItemSpec {
  const TreeItemSpec({
    required this.id,
    required this.label,
    this.kind = 'file',
    this.children = const [],
    this.expanded = false,
  });

  final String id;
  final String label;
  final String kind;
  final List<TreeItemSpec> children;
  final bool expanded;

  factory TreeItemSpec.fromJson(Map<String, dynamic> json) => TreeItemSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        kind: json['kind'] as String? ?? 'file',
        children: ((json['children'] as List?) ?? [])
            .map((c) => TreeItemSpec.fromJson(c as Map<String, dynamic>))
            .toList(),
        expanded: json['expanded'] as bool? ?? false,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'kind': kind,
        'children': children.map((c) => c.toJson()).toList(),
        'expanded': expanded,
      };

  @override
  bool operator ==(Object other) =>
      other is TreeItemSpec &&
      other.id == id &&
      other.label == label &&
      other.kind == kind &&
      _listEq(other.children, children) &&
      other.expanded == expanded;

  @override
  int get hashCode => Object.hash(id, label, kind, expanded);
}

// ---------------------------------------------------------------------------
// TextBox / Media / Menu / MarkdownEditor / Canvas / Surface (light specs)
// ---------------------------------------------------------------------------

/// Mirrors Swift `TextBoxSpec` (render subset).
final class TextBoxSpec {
  const TextBoxSpec({
    this.text = '',
    this.placeholder = '',
    this.prompt = '',
    this.submitLabel = 'Submit',
  });

  final String text;
  final String placeholder;
  final String prompt;
  final String submitLabel;

  factory TextBoxSpec.fromJson(Map<String, dynamic> json) => TextBoxSpec(
        text: json['text'] as String? ?? '',
        placeholder: json['placeholder'] as String? ?? '',
        prompt: json['prompt'] as String? ?? '',
        submitLabel: json['submitLabel'] as String? ?? 'Submit',
      );

  Map<String, dynamic> toJson() => {
        'text': text,
        'placeholder': placeholder,
        'prompt': prompt,
        'submitLabel': submitLabel,
      };

  @override
  bool operator ==(Object other) =>
      other is TextBoxSpec &&
      other.text == text &&
      other.placeholder == placeholder &&
      other.prompt == prompt;

  @override
  int get hashCode => Object.hash(text, placeholder, prompt);
}

/// Mirrors Swift `MediaSpec` (render subset).
final class MediaSpec {
  const MediaSpec({this.source = '', this.caption});

  final String source;
  final String? caption;

  factory MediaSpec.fromJson(Map<String, dynamic> json) => MediaSpec(
        source: json['source'] as String? ?? '',
        caption: json['caption'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'source': source,
        if (caption != null) 'caption': caption,
      };

  @override
  bool operator ==(Object other) =>
      other is MediaSpec && other.source == source && other.caption == caption;

  @override
  int get hashCode => Object.hash(source, caption);
}

/// Mirrors Swift `UIMenuSpec` (render subset).
final class MenuSpec {
  const MenuSpec({required this.id, this.items = const []});

  final String id;
  final List<MenuItemSpec> items;

  factory MenuSpec.fromJson(Map<String, dynamic> json) => MenuSpec(
        id: json['id'] as String,
        items: ((json['items'] as List?) ?? [])
            .map((i) => MenuItemSpec.fromJson(i as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'items': items.map((i) => i.toJson()).toList(),
      };

  @override
  bool operator ==(Object other) =>
      other is MenuSpec && other.id == id && _listEq(other.items, items);

  @override
  int get hashCode => Object.hash(id, items.length);
}

/// Mirrors Swift `UIMenuItemSpec` (render subset).
final class MenuItemSpec {
  const MenuItemSpec({
    required this.id,
    required this.label,
    this.action,
    this.role = 'default',
  });

  final String id;
  final String label;
  final String? action;
  final String role;

  factory MenuItemSpec.fromJson(Map<String, dynamic> json) => MenuItemSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        action: json['action'] as String?,
        role: json['role'] as String? ?? 'default',
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        if (action != null) 'action': action,
        'role': role,
      };

  @override
  bool operator ==(Object other) =>
      other is MenuItemSpec &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.role == role;

  @override
  int get hashCode => Object.hash(id, label, action, role);
}

/// Mirrors Swift `MarkdownEditorSpec` (render subset).
final class MarkdownEditorSpec {
  const MarkdownEditorSpec({this.text = '', this.placeholder = ''});

  final String text;
  final String placeholder;

  factory MarkdownEditorSpec.fromJson(Map<String, dynamic> json) =>
      MarkdownEditorSpec(
        text: json['text'] as String? ?? '',
        placeholder: json['placeholder'] as String? ?? '',
      );

  Map<String, dynamic> toJson() => {'text': text, 'placeholder': placeholder};

  @override
  bool operator ==(Object other) =>
      other is MarkdownEditorSpec &&
      other.text == text &&
      other.placeholder == placeholder;

  @override
  int get hashCode => Object.hash(text, placeholder);
}

/// Mirrors Swift `CanvasPageSpec` (render subset).
final class CanvasPageSpec {
  const CanvasPageSpec({this.children = const []});

  final List<AppKitNode> children;

  factory CanvasPageSpec.fromJson(Map<String, dynamic> json) =>
      CanvasPageSpec(
        children: ((json['children'] as List?) ?? [])
            .map((c) => AppKitNode.fromJson(c as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'children': children.map((c) => c.toJson()).toList(),
      };

  @override
  bool operator ==(Object other) =>
      other is CanvasPageSpec && _listEq(other.children, children);

  @override
  int get hashCode => children.length;
}

/// Mirrors Swift `SurfaceSpec` (render subset).
final class SurfaceSpec {
  const SurfaceSpec({this.child});

  final AppKitNode? child;

  factory SurfaceSpec.fromJson(Map<String, dynamic> json) => SurfaceSpec(
        child: json['child'] == null
            ? null
            : AppKitNode.fromJson(json['child'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        if (child != null) 'child': child!.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is SurfaceSpec && other.child == child;

  @override
  int get hashCode => child.hashCode;
}

// ---------------------------------------------------------------------------
// Events (client -> host)
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIAction`.
final class AppKitAction {
  const AppKitAction({
    required this.id,
    required this.action,
    this.value,
  });

  final String id;
  final String action;
  final String? value;

  Map<String, dynamic> toJson() => {
        'id': id,
        'action': action,
        if (value != null) 'value': value,
      };

  @override
  bool operator ==(Object other) =>
      other is AppKitAction &&
      other.id == id &&
      other.action == action &&
      other.value == value;

  @override
  int get hashCode => Object.hash(id, action, value);
}

/// Mirrors Swift `UIEventKind`.
enum AppKitEventKind {
  activate('activate'),
  select('select'),
  input('input'),
  toggle('toggle'),
  submit('submit'),
  dismiss('dismiss');

  const AppKitEventKind(this.wireValue);
  final String wireValue;

  static AppKitEventKind? fromWire(String value) {
    for (final k in AppKitEventKind.values) {
      if (k.wireValue == value) return k;
    }
    return null;
  }
}

/// Mirrors Swift `UIEvent` (kind + target + value envelope).
final class AppKitEvent {
  const AppKitEvent({
    required this.kind,
    required this.targetId,
    this.value,
  });

  final AppKitEventKind kind;
  final String targetId;
  final String? value;

  Map<String, dynamic> toJson() => {
        'kind': kind.wireValue,
        'targetId': targetId,
        if (value != null) 'value': value,
      };
}

// ---------------------------------------------------------------------------
// Participant token (UIParticipantToken.swift)
// ---------------------------------------------------------------------------

/// Opaque participant identity for app-kit sessions.
/// Mirrors Swift `UIParticipantToken` (163 lines).
final class AppKitParticipantToken {
  const AppKitParticipantToken({
    required this.token,
    required this.participantId,
    this.expiresAt,
  });

  final String token;
  final String participantId;
  final DateTime? expiresAt;

  bool get isExpired =>
      expiresAt != null && DateTime.now().isAfter(expiresAt!);

  factory AppKitParticipantToken.fromJson(Map<String, dynamic> json) =>
      AppKitParticipantToken(
        token: json['token'] as String,
        participantId: json['participantId'] as String,
        expiresAt: json['expiresAt'] == null
            ? null
            : DateTime.parse(json['expiresAt'] as String),
      );

  Map<String, dynamic> toJson() => {
        'token': token,
        'participantId': participantId,
        if (expiresAt != null)
          'expiresAt': expiresAt!.toUtc().toIso8601String(),
      };

  @override
  bool operator ==(Object other) =>
      other is AppKitParticipantToken &&
      other.token == token &&
      other.participantId == participantId;

  @override
  int get hashCode => Object.hash(token, participantId);
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// Mirrors Swift `UISnapshot` (header subset).
final class AppKitSnapshot {
  const AppKitSnapshot({
    required this.appInstanceId,
    required this.clientId,
    required this.root,
    this.protocolVersion = AppKitProtocol.version,
  });

  final String appInstanceId;
  final String clientId;
  final AppKitNode root;
  final int protocolVersion;

  bool get versionSupported => AppKitProtocol.supports(protocolVersion);

  factory AppKitSnapshot.fromJson(Map<String, dynamic> json) =>
      AppKitSnapshot(
        appInstanceId: json['appInstanceId'] as String,
        clientId: json['clientId'] as String,
        root: AppKitNode.fromJson(json['root'] as Map<String, dynamic>),
        protocolVersion: json['protocolVersion'] as int? ?? 1,
      );
}

/// Decode a JSON string into an [AppKitNode]. Throws [FormatException] on
/// malformed JSON, mirroring Swift's `DecodingError`.
AppKitNode appKitNodeFromJsonString(String source) =>
    AppKitNode.fromJson(jsonDecode(source) as Map<String, dynamic>);

bool _listEq<T>(List<T> a, List<T> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
