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
/// - Capability negotiation (`requiredCapabilities`): ported for `MenuSpec`;
///   the Dart renderer assumes the Host only sends supported components for
///   other types. GAP-APPKIT-2 (partial).
/// - `UIDelta` incremental application: ported in `appkit_delta_apply.dart`.
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
    this.header = const PageHeaderUnsupported('none'),
    this.body = const PageBodyList(ListSpec(id: '', items: [])),
    this.footer = const FooterActionsSpec(),
  });

  final String title;
  final List<PageTab> tabs;
  final PageToolbar? toolbar;
  final String? back;
  final PageHeader header;
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
      header: PageHeader.fromJson(json['header'] as Map<String, dynamic>?),
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
        'header': header.toJson(),
        'body': body.toJson(),
        'footer': footer.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is PageSpec &&
      other.title == title &&
      _listEq(other.tabs, tabs) &&
      other.back == back &&
      other.header == header &&
      other.body == body &&
      other.footer == footer;

  @override
  int get hashCode => Object.hash(title, back, header, body, footer);
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
      case 'sparkline':
        return PageBodySparkline(SparklineSpec.fromJson(
            json['sparkline'] as Map<String, dynamic>? ?? json));
      case 'barChart':
        return PageBodyBarChart(BarChartSpec.fromJson(
            json['barChart'] as Map<String, dynamic>? ?? json));
      case 'lineChart':
        return PageBodyLineChart(LineChartSpec.fromJson(
            json['lineChart'] as Map<String, dynamic>? ?? json));
      default:
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

final class PageBodySparkline extends PageBody {
  const PageBodySparkline(this.sparkline);
  final SparklineSpec sparkline;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'sparkline', 'sparkline': sparkline.toJson()};
  @override
  bool operator ==(Object other) =>
      other is PageBodySparkline && other.sparkline == sparkline;
  @override
  int get hashCode => sparkline.hashCode;
}

final class PageBodyBarChart extends PageBody {
  const PageBodyBarChart(this.chart);
  final BarChartSpec chart;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'barChart', 'barChart': chart.toJson()};
  @override
  bool operator ==(Object other) =>
      other is PageBodyBarChart && other.chart == chart;
  @override
  int get hashCode => chart.hashCode;
}

final class PageBodyLineChart extends PageBody {
  const PageBodyLineChart(this.chart);
  final LineChartSpec chart;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'lineChart', 'lineChart': chart.toJson()};
  @override
  bool operator ==(Object other) =>
      other is PageBodyLineChart && other.chart == chart;
  @override
  int get hashCode => chart.hashCode;
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

/// Mirrors Swift `UIInputSpec`: the page-header search/filter field.
///
/// The input behaves like a row for focus (Up from the first row lands on
/// it) and typing goes straight into it. `setValue` is the action fired on
/// every keystroke; `submit` fires on Enter.
final class UIInputSpec {
  const UIInputSpec({
    required this.id,
    required this.label,
    this.value = '',
    this.placeholder = '',
    this.setValue,
    this.submit,
  });

  final String id;
  final String label;
  final String value;
  final String placeholder;
  final String? setValue;
  final String? submit;

  factory UIInputSpec.fromJson(Map<String, dynamic> json) => UIInputSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        value: json['value'] as String? ?? '',
        placeholder: json['placeholder'] as String? ?? '',
        setValue: json['setValue'] as String?,
        submit: json['submit'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'value': value,
        'placeholder': placeholder,
        if (setValue != null) 'setValue': setValue,
        if (submit != null) 'submit': submit,
      };

  UIInputSpec copyWith({String? value}) => UIInputSpec(
        id: id,
        label: label,
        value: value ?? this.value,
        placeholder: placeholder,
        setValue: setValue,
        submit: submit,
      );

  @override
  bool operator ==(Object other) =>
      other is UIInputSpec &&
      other.id == id &&
      other.label == label &&
      other.value == value &&
      other.placeholder == placeholder &&
      other.setValue == setValue &&
      other.submit == submit;

  @override
  int get hashCode => Object.hash(id, label, value, placeholder);
}

/// Page header slot. Mirrors Swift `UIPageHeaderSlot`.
sealed class PageHeader {
  const PageHeader();

  factory PageHeader.fromJson(Map<String, dynamic>? json) {
    if (json == null) return const PageHeaderUnsupported('none');
    final type = json['type'] as String?;
    switch (type) {
      case 'input':
        return PageHeaderInput(
            UIInputSpec.fromJson(json['input'] as Map<String, dynamic>? ?? json));
      default:
        return PageHeaderUnsupported(type ?? 'unknown');
    }
  }

  Map<String, dynamic> toJson();
}

final class PageHeaderInput extends PageHeader {
  const PageHeaderInput(this.input);
  final UIInputSpec input;
  @override
  Map<String, dynamic> toJson() =>
      {'type': 'input', 'input': input.toJson()};
  @override
  bool operator ==(Object other) =>
      other is PageHeaderInput && other.input == input;
  @override
  int get hashCode => input.hashCode;
}

final class PageHeaderUnsupported extends PageHeader {
  const PageHeaderUnsupported(this.kind);
  final String kind;
  @override
  Map<String, dynamic> toJson() => {'type': kind};
  @override
  bool operator ==(Object other) =>
      other is PageHeaderUnsupported && other.kind == kind;
  @override
  int get hashCode => kind.hashCode;
}

/// Mirrors Swift `UIFooterActionsSpec`.
final class FooterActionsSpec {  const FooterActionsSpec({this.actions = const [], this.status});

  final List<FooterActionSpec> actions;
  final String? status;

  bool get isEmpty => actions.isEmpty && status == null;

  /// Mirrors Swift `UIFooterActionsSpec.isValid` (simplified): actions have
  /// unique non-empty IDs and single-line labels.
  bool get isValid {
    final ids = <String>{};
    for (final action in actions) {
      if (action.id.isEmpty ||
          action.label.contains('\n') ||
          action.label.contains('\r') ||
          !ids.add(action.id)) {
        return false;
      }
    }
    return true;
  }

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

  ListSpec copyWith({List<ListItemSpec>? items, String? selectedId, bool clearSelectedId = false}) =>
      ListSpec(
        id: id,
        items: items ?? this.items,
        emptyMessage: emptyMessage,
        selectedId: clearSelectedId ? null : (selectedId ?? this.selectedId),
        rowLayout: rowLayout,
      );
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

  ListItemSpec copyWith({
    bool? done,
    ListItemSlot? leading,
    ListItemSlot? trailing,
    ListItemSlot? accessory,
  }) =>
      ListItemSpec(
        id: id,
        label: label,
        detail: detail,
        value: value,
        done: done ?? this.done,
        busy: busy,
        leading: leading ?? this.leading,
        trailing: trailing ?? this.trailing,
        accessory: accessory ?? this.accessory,
        top: top,
        bottom: bottom,
        media: media,
        divider: divider,
        activate: activate,
      );
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
          on: json['value'] as bool? ?? json['on'] as bool? ?? false,
          label: json['label'] as String? ?? '',
          setValue: json['setValue'] as String?,
        );
      case 'sparkline':
        return SlotSparkline(
            values: ((json['values'] as List?) ?? []).cast<num>());
      case 'disclosure':
        return const SlotDisclosure();
      case 'checkmark':
        return SlotCheckmark(
          id: json['id'] as String? ?? '',
          checked: json['value'] as bool? ?? json['checked'] as bool? ?? false,
          label: json['label'] as String? ?? '',
          setValue: json['setValue'] as String?,
        );
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
  const SlotToggle({required this.id, required this.on, this.label = '', this.setValue});
  final String id;
  final bool on;
  final String label;
  final String? setValue;
  @override
  String get kind => 'toggle';
  @override
  Map<String, dynamic> toJson() => {
        'type': 'toggle',
        'id': id,
        'on': on,
        if (label.isNotEmpty) 'label': label,
        if (setValue != null) 'setValue': setValue,
      };
  SlotToggle copyWith({bool? on}) => SlotToggle(
        id: id,
        on: on ?? this.on,
        label: label,
        setValue: setValue,
      );
  @override
  bool operator ==(Object other) =>
      other is SlotToggle && other.id == id && other.on == on;
  @override
  int get hashCode => Object.hash(id, on);
}

final class SlotSparkline extends ListItemSlot {
  const SlotSparkline({required this.values, this.id = ''});
  final List<num> values;
  final String id;
  @override
  String get kind => 'sparkline';
  @override
  Map<String, dynamic> toJson() => {
        'type': 'sparkline',
        'values': values,
        if (id.isNotEmpty) 'id': id,
      };
  @override
  bool operator ==(Object other) =>
      other is SlotSparkline && _listEq(other.values, values) && other.id == id;
  @override
  int get hashCode => Object.hash(values.length, id);
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
  const SlotCheckmark(
      {required this.id, required this.checked, this.label = '', this.setValue});
  final String id;
  final bool checked;
  final String label;
  final String? setValue;
  @override
  String get kind => 'checkmark';
  @override
  Map<String, dynamic> toJson() => {
        'type': 'checkmark',
        'id': id,
        'checked': checked,
        if (label.isNotEmpty) 'label': label,
        if (setValue != null) 'setValue': setValue,
      };
  SlotCheckmark copyWith({bool? checked}) => SlotCheckmark(
        id: id,
        checked: checked ?? this.checked,
        label: label,
        setValue: setValue,
      );
  @override
  bool operator ==(Object other) =>
      other is SlotCheckmark && other.id == id && other.checked == checked;
  @override
  int get hashCode => Object.hash(id, checked);
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

// ---------------------------------------------------------------------------
// Charts (sparkline / barChart / lineChart page bodies)
// ---------------------------------------------------------------------------

/// Mirrors Swift `UISparklineSpec`.
///
/// Validation mirrors Swift `isValid`: 1…100,000 finite series values,
/// containing bounds, single-line caption/unit, valid identifiers and
/// accessibility text.
final class SparklineSpec {
  const SparklineSpec({
    required this.id,
    required this.series,
    this.min,
    this.max,
    this.caption,
    this.unit,
    required this.accessibilityText,
    this.activate,
  });

  final String id;
  final List<double> series;
  final double? min;
  final double? max;
  final String? caption;
  final String? unit;
  final String accessibilityText;
  final String? activate;

  bool get isValid =>
      id.isNotEmpty &&
      series.length >= 1 &&
      series.length <= 100000 &&
      series.every((v) => v.isFinite) &&
      (min?.isFinite ?? true) &&
      (max?.isFinite ?? true) &&
      (min == null || series.every((v) => v >= min!)) &&
      (max == null || series.every((v) => v <= max!)) &&
      (min == null || max == null || min! < max!) &&
      accessibilityText.trim().isNotEmpty &&
      (caption == null || !_hasNewline(caption!)) &&
      (unit == null || !_hasNewline(unit!));

  /// Mirrors Swift `resolvedBounds`: inferred bounds include zero; an
  /// all-zero series expands to 0...1.
  (double, double) get resolvedBounds {
    final seriesMin = series.reduce((a, b) => a < b ? a : b);
    final seriesMax = series.reduce((a, b) => a > b ? a : b);
    final lower = min ?? (seriesMin < 0 ? seriesMin : 0);
    var upper = max ?? (seriesMax > 0 ? seriesMax : 0);
    if (lower == upper) upper = lower + 1;
    return (lower, upper);
  }

  /// Mirrors Swift `normalizedSeries`.
  List<double> get normalizedSeries {
    final (lower, upper) = resolvedBounds;
    final range = upper - lower;
    return series
        .map((v) => ((v - lower) / range).clamp(0.0, 1.0))
        .toList();
  }

  factory SparklineSpec.fromJson(Map<String, dynamic> json) {
    final spec = SparklineSpec(
      id: json['id'] as String,
      series: ((json['series'] as List?) ?? [])
          .map((v) => (v as num).toDouble())
          .toList(),
      min: (json['min'] as num?)?.toDouble(),
      max: (json['max'] as num?)?.toDouble(),
      caption: json['caption'] as String?,
      unit: json['unit'] as String?,
      accessibilityText: json['accessibilityText'] as String,
      activate: json['activate'] as String?,
    );
    if (!spec.isValid) {
      throw FormatException('Sparkline needs finite data and valid bounds', json);
    }
    return spec;
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'series': series,
        if (min != null) 'min': min,
        if (max != null) 'max': max,
        if (caption != null) 'caption': caption,
        if (unit != null) 'unit': unit,
        'accessibilityText': accessibilityText,
        if (activate != null) 'activate': activate,
      };

  @override
  bool operator ==(Object other) =>
      other is SparklineSpec &&
      other.id == id &&
      _listEq(other.series, series) &&
      other.min == min &&
      other.max == max &&
      other.caption == caption &&
      other.unit == unit &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode => Object.hash(id, series.length, accessibilityText);
}

bool _hasNewline(String s) => s.contains('\n') || s.contains('\r');

/// Mirrors Swift `UIBarChartBar`.
final class BarChartBar {
  const BarChartBar({
    required this.label,
    required this.value,
    this.valueCaption,
    this.emphasis = 'default',
  });

  final String label;
  final double value;
  final String? valueCaption;
  final String emphasis;

  bool get isValid =>
      label.trim().isNotEmpty &&
      value.isFinite &&
      value >= 0 &&
      (emphasis == 'default' || emphasis == 'accent' || emphasis == 'danger');

  factory BarChartBar.fromJson(Map<String, dynamic> json) => BarChartBar(
        label: json['label'] as String,
        value: (json['value'] as num).toDouble(),
        valueCaption: json['valueCaption'] as String?,
        emphasis: json['emphasis'] as String? ?? 'default',
      );

  Map<String, dynamic> toJson() => {
        'label': label,
        'value': value,
        if (valueCaption != null) 'valueCaption': valueCaption,
        'emphasis': emphasis,
      };

  @override
  bool operator ==(Object other) =>
      other is BarChartBar &&
      other.label == label &&
      other.value == value &&
      other.valueCaption == valueCaption &&
      other.emphasis == emphasis;

  @override
  int get hashCode => Object.hash(label, value, emphasis);
}

/// Mirrors Swift `UIBarChartSpec`.
final class BarChartSpec {
  const BarChartSpec({
    required this.id,
    required this.bars,
    required this.accessibilityText,
    this.activate,
  });

  final String id;
  final List<BarChartBar> bars;
  final String accessibilityText;
  final String? activate;

  bool get isValid =>
      id.isNotEmpty &&
      bars.length >= 1 &&
      bars.length <= 1000 &&
      bars.every((b) => b.isValid) &&
      accessibilityText.trim().isNotEmpty;

  /// Mirrors Swift `normalizedValues`.
  List<double> get normalizedValues {
    final maximum = bars.map((b) => b.value).reduce((a, b) => a > b ? a : b);
    final denom = maximum > 0 ? maximum : 1;
    return bars.map((b) => (b.value / denom).clamp(0.0, 1.0)).toList();
  }

  factory BarChartSpec.fromJson(Map<String, dynamic> json) {
    final spec = BarChartSpec(
      id: json['id'] as String,
      bars: ((json['bars'] as List?) ?? [])
          .map((b) => BarChartBar.fromJson(b as Map<String, dynamic>))
          .toList(),
      accessibilityText: json['accessibilityText'] as String,
      activate: json['activate'] as String?,
    );
    if (!spec.isValid) {
      throw FormatException(
          'BarChart needs labeled non-negative bars and accessibility text',
          json);
    }
    return spec;
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'bars': bars.map((b) => b.toJson()).toList(),
        'accessibilityText': accessibilityText,
        if (activate != null) 'activate': activate,
      };

  @override
  bool operator ==(Object other) =>
      other is BarChartSpec &&
      other.id == id &&
      _listEq(other.bars, bars) &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode => Object.hash(id, bars.length, accessibilityText);
}

/// Mirrors Swift `UILineChartPoint`.
final class LineChartPoint {
  const LineChartPoint({required this.x, required this.y});

  final double x;
  final double y;

  factory LineChartPoint.fromJson(Map<String, dynamic> json) => LineChartPoint(
        x: (json['x'] as num).toDouble(),
        y: (json['y'] as num).toDouble(),
      );

  Map<String, dynamic> toJson() => {'x': x, 'y': y};

  @override
  bool operator ==(Object other) =>
      other is LineChartPoint && other.x == x && other.y == y;

  @override
  int get hashCode => Object.hash(x, y);
}

/// Mirrors Swift `UILineChartSeries`.
final class LineChartSeries {
  const LineChartSeries({required this.name, this.points = const []});

  final String name;
  final List<LineChartPoint> points;

  factory LineChartSeries.fromJson(Map<String, dynamic> json) => LineChartSeries(
        name: json['name'] as String,
        points: ((json['points'] as List?) ?? [])
            .map((p) => LineChartPoint.fromJson(p as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'name': name,
        'points': points.map((p) => p.toJson()).toList(),
      };

  @override
  bool operator ==(Object other) =>
      other is LineChartSeries &&
      other.name == name &&
      _listEq(other.points, points);

  @override
  int get hashCode => Object.hash(name, points.length);
}

/// Mirrors Swift `UILineChartAxis`.
final class LineChartAxis {
  const LineChartAxis({this.min, this.max, this.label});

  final double? min;
  final double? max;
  final String? label;

  bool get isValid =>
      (min == null || min!.isFinite) &&
      (max == null || max!.isFinite) &&
      (min == null || max == null || min! < max!);

  factory LineChartAxis.fromJson(Map<String, dynamic> json) => LineChartAxis(
        min: (json['min'] as num?)?.toDouble(),
        max: (json['max'] as num?)?.toDouble(),
        label: json['label'] as String?,
      );

  Map<String, dynamic> toJson() => {
        if (min != null) 'min': min,
        if (max != null) 'max': max,
        if (label != null) 'label': label,
      };

  @override
  bool operator ==(Object other) =>
      other is LineChartAxis &&
      other.min == min &&
      other.max == max &&
      other.label == label;

  @override
  int get hashCode => Object.hash(min, max, label);
}

/// Mirrors Swift `UILineChartSpec`.
final class LineChartSpec {
  const LineChartSpec({
    required this.id,
    required this.series,
    this.xAxis = const LineChartAxis(),
    this.yAxis = const LineChartAxis(),
    required this.accessibilityText,
    this.activate,
  });

  final String id;
  final List<LineChartSeries> series;
  final LineChartAxis xAxis;
  final LineChartAxis yAxis;
  final String accessibilityText;
  final String? activate;

  bool get isValid =>
      id.isNotEmpty &&
      series.isNotEmpty &&
      series.every((s) =>
          s.name.trim().isNotEmpty &&
          s.points.every((p) => p.x.isFinite && p.y.isFinite)) &&
      xAxis.isValid &&
      yAxis.isValid &&
      accessibilityText.trim().isNotEmpty;

  factory LineChartSpec.fromJson(Map<String, dynamic> json) {
    final spec = LineChartSpec(
      id: json['id'] as String,
      series: ((json['series'] as List?) ?? [])
          .map((s) => LineChartSeries.fromJson(s as Map<String, dynamic>))
          .toList(),
      xAxis: json['xAxis'] == null
          ? const LineChartAxis()
          : LineChartAxis.fromJson(json['xAxis'] as Map<String, dynamic>),
      yAxis: json['yAxis'] == null
          ? const LineChartAxis()
          : LineChartAxis.fromJson(json['yAxis'] as Map<String, dynamic>),
      accessibilityText: json['accessibilityText'] as String,
      activate: json['activate'] as String?,
    );
    if (!spec.isValid) {
      throw FormatException(
          'LineChart needs named series with finite points and valid axes',
          json);
    }
    return spec;
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'series': series.map((s) => s.toJson()).toList(),
        'xAxis': xAxis.toJson(),
        'yAxis': yAxis.toJson(),
        'accessibilityText': accessibilityText,
        if (activate != null) 'activate': activate,
      };

  @override
  bool operator ==(Object other) =>
      other is LineChartSpec &&
      other.id == id &&
      _listEq(other.series, series) &&
      other.xAxis == xAxis &&
      other.yAxis == yAxis &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode => Object.hash(id, series.length, accessibilityText);
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
    this.selection,
  });

  final String id;
  final String label;
  final List<ContentLine> lines;
  final String emptyMessage;
  final Map<String, dynamic>? selection;

  factory ContentSpec.fromJson(Map<String, dynamic> json) => ContentSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        lines: ((json['lines'] as List?) ?? [])
            .map((l) => ContentLine.fromJson(l as Map<String, dynamic>))
            .toList(),
        emptyMessage: json['emptyMessage'] as String? ?? '',
        selection: json['selection'] as Map<String, dynamic>?,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'lines': lines.map((l) => l.toJson()).toList(),
        'emptyMessage': emptyMessage,
        if (selection != null) 'selection': selection,
      };

  ContentSpec copyWith({
    List<ContentLine>? lines,
    Map<String, dynamic>? selection,
    bool clearSelection = false,
  }) =>
      ContentSpec(
        id: id,
        label: label,
        lines: lines ?? this.lines,
        emptyMessage: emptyMessage,
        selection: clearSelection ? null : (selection ?? this.selection),
      );

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
    this.selectedId,
    this.filter,
    this.location = '',
    this.footer = const FooterActionsSpec(),
  });

  final String id;
  final List<TreeItemSpec> items;
  final String emptyMessage;
  final String? selectedId;
  final TreeFilterSpec? filter;
  final String location;
  final FooterActionsSpec footer;

  factory TreeSpec.fromJson(Map<String, dynamic> json) => TreeSpec(
        id: json['id'] as String,
        items: ((json['items'] as List?) ?? [])
            .map((i) => TreeItemSpec.fromJson(i as Map<String, dynamic>))
            .toList(),
        emptyMessage: json['emptyMessage'] as String? ?? '',
        selectedId: json['selectedId'] as String?,
        filter: json['filter'] == null
            ? null
            : TreeFilterSpec.fromJson(json['filter'] as Map<String, dynamic>),
        location: json['location'] as String? ?? '',
        footer: json['footer'] == null
            ? const FooterActionsSpec()
            : FooterActionsSpec.fromJson(
                json['footer'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'items': items.map((i) => i.toJson()).toList(),
        'emptyMessage': emptyMessage,
        if (selectedId != null) 'selectedId': selectedId,
        if (filter != null) 'filter': filter!.toJson(),
        'location': location,
        'footer': footer.toJson(),
      };

  TreeSpec copyWith({
    List<TreeItemSpec>? items,
    String? selectedId,
    bool clearSelectedId = false,
    TreeFilterSpec? filter,
    String? location,
    FooterActionsSpec? footer,
  }) =>
      TreeSpec(
        id: id,
        items: items ?? this.items,
        emptyMessage: emptyMessage,
        selectedId: clearSelectedId ? null : (selectedId ?? this.selectedId),
        filter: filter ?? this.filter,
        location: location ?? this.location,
        footer: footer ?? this.footer,
      );

  @override
  bool operator ==(Object other) =>
      other is TreeSpec &&
      other.id == id &&
      _listEq(other.items, items) &&
      other.selectedId == selectedId &&
      other.location == location;

  @override
  int get hashCode => Object.hash(id, items.length, selectedId);
}

/// Mirrors Swift `UITreeFilterSpec`.
final class TreeFilterSpec {
  const TreeFilterSpec({required this.id, this.value = ''});

  final String id;
  final String value;

  factory TreeFilterSpec.fromJson(Map<String, dynamic> json) => TreeFilterSpec(
        id: json['id'] as String,
        value: json['value'] as String? ?? '',
      );

  Map<String, dynamic> toJson() => {'id': id, 'value': value};

  TreeFilterSpec copyWith({String? value}) =>
      TreeFilterSpec(id: id, value: value ?? this.value);

  @override
  bool operator ==(Object other) =>
      other is TreeFilterSpec && other.id == id && other.value == value;

  @override
  int get hashCode => Object.hash(id, value);
}

/// Mirrors Swift `UITreeItem`.
final class TreeItemSpec {
  const TreeItemSpec({
    required this.id,
    required this.label,
    this.kind = 'file',
    this.children = const [],
    this.expanded = false,
    this.childState = 'loaded',
  });

  final String id;
  final String label;
  final String kind;
  final List<TreeItemSpec> children;
  final bool expanded;
  final String childState;

  factory TreeItemSpec.fromJson(Map<String, dynamic> json) => TreeItemSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        kind: json['kind'] as String? ?? 'file',
        children: ((json['children'] as List?) ?? [])
            .map((c) => TreeItemSpec.fromJson(c as Map<String, dynamic>))
            .toList(),
        expanded: json['expanded'] as bool? ?? false,
        childState: json['childState'] as String? ?? 'loaded',
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'kind': kind,
        'children': children.map((c) => c.toJson()).toList(),
        'expanded': expanded,
        'childState': childState,
      };

  TreeItemSpec copyWith({
    List<TreeItemSpec>? children,
    bool? expanded,
    String? childState,
  }) =>
      TreeItemSpec(
        id: id,
        label: label,
        kind: kind,
        children: children ?? this.children,
        expanded: expanded ?? this.expanded,
        childState: childState ?? this.childState,
      );

  @override
  bool operator ==(Object other) =>
      other is TreeItemSpec &&
      other.id == id &&
      other.label == label &&
      other.kind == kind &&
      _listEq(other.children, children) &&
      other.expanded == expanded &&
      other.childState == childState;

  @override
  int get hashCode => Object.hash(id, label, kind, expanded, childState);
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
  const MediaSpec({this.source = '', this.caption, this.intrinsic});

  final String source;
  final String? caption;
  final Map<String, dynamic>? intrinsic;

  factory MediaSpec.fromJson(Map<String, dynamic> json) => MediaSpec(
        source: json['source'] as String? ?? '',
        caption: json['caption'] as String?,
        intrinsic: json['intrinsic'] as Map<String, dynamic>?,
      );

  Map<String, dynamic> toJson() => {
        'source': source,
        if (caption != null) 'caption': caption,
        if (intrinsic != null) 'intrinsic': intrinsic,
      };

  MediaSpec copyWith({String? source, Map<String, dynamic>? intrinsic}) =>
      MediaSpec(
        source: source ?? this.source,
        caption: caption,
        intrinsic: intrinsic ?? this.intrinsic,
      );

  @override
  bool operator ==(Object other) =>
      other is MediaSpec && other.source == source && other.caption == caption;

  @override
  int get hashCode => Object.hash(source, caption);
}

/// Mirrors Swift `UIMenuSpec` (render subset).
/// Mirrors Swift `UIMenuSpec` (full decode).
///
/// The menu presents grouped actions. `presentation` is `popup` or
/// `palette`; `anchor` is `control` or `cursor`. `selectedID` tracks the
/// keyboard-selected item; `dismiss` is the action fired on dismiss.
final class MenuSpec {
  const MenuSpec({
    required this.id,
    this.label = '',
    this.presentation = 'popup',
    this.anchor = 'control',
    this.items = const [],
    this.selectedId,
    this.dismiss,
  });

  final String id;
  final String label;
  final String presentation;
  final String anchor;
  final List<MenuItemSpec> items;
  final String? selectedId;
  final String? dismiss;

  /// Mirrors Swift `UIMenuSpec.requiredCapabilities`: nil when the menu is
  /// invalid (duplicate IDs, >256 items, selectedID not in items, or the
  /// selected item is disabled).
  List<String>? get requiredCapabilities {
    final ids = items.map((i) => i.id).toSet();
    if (items.length > 256 ||
        ids.length != items.length ||
        (selectedId != null && !ids.contains(selectedId)) ||
        items.any((i) => i.id == selectedId && i.disabled)) {
      return null;
    }
    return [AppKitProtocol.menuCapability, AppKitProtocol.menuAnchorCapability];
  }

  factory MenuSpec.fromJson(Map<String, dynamic> json) {
    final spec = MenuSpec(
      id: json['id'] as String? ?? '',
      label: json['label'] as String? ?? '',
      presentation: json['presentation'] as String? ?? 'popup',
      anchor: json['anchor'] as String? ?? 'control',
      items: ((json['items'] as List?) ?? [])
          .map((i) => MenuItemSpec.fromJson(i as Map<String, dynamic>))
          .toList(),
      selectedId: json['selectedId'] as String?,
      dismiss: json['dismiss'] as String?,
    );
    return spec;
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'presentation': presentation,
        'anchor': anchor,
        'items': items.map((i) => i.toJson()).toList(),
        if (selectedId != null) 'selectedId': selectedId,
        if (dismiss != null) 'dismiss': dismiss,
      };

  MenuSpec copyWith({String? selectedId}) => MenuSpec(
        id: id,
        label: label,
        presentation: presentation,
        anchor: anchor,
        items: items,
        selectedId: selectedId,
        dismiss: dismiss,
      );

  @override
  bool operator ==(Object other) =>
      other is MenuSpec &&
      other.id == id &&
      other.label == label &&
      other.presentation == presentation &&
      other.anchor == anchor &&
      _listEq(other.items, items) &&
      other.selectedId == selectedId &&
      other.dismiss == dismiss;

  @override
  int get hashCode => Object.hash(id, label, items.length, selectedId);
}

/// Mirrors Swift `UIMenuItemSpec` (render subset).
final class MenuItemSpec {
  const MenuItemSpec({
    required this.id,
    required this.label,
    this.action,
    this.hint,
    this.disabled = false,
    this.role = 'default',
  });

  final String id;
  final String label;
  final String? action;
  final String? hint;
  final bool disabled;
  final String role;

  factory MenuItemSpec.fromJson(Map<String, dynamic> json) => MenuItemSpec(
        id: json['id'] as String,
        label: json['label'] as String,
        action: json['action'] as String?,
        hint: json['hint'] as String?,
        disabled: json['disabled'] as bool? ?? false,
        role: json['role'] as String? ?? 'default',
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        if (action != null) 'action': action,
        if (hint != null) 'hint': hint,
        'disabled': disabled,
        'role': role,
      };

  @override
  bool operator ==(Object other) =>
      other is MenuItemSpec &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.hint == hint &&
      other.disabled == disabled &&
      other.role == role;

  @override
  int get hashCode => Object.hash(id, label, action, disabled, role);
}

/// Mirrors Swift `MarkdownEditorSpec` (full).
///
/// The editor is the richest component: text with UTF-16 selection, presentation
/// mode, read-only/dirty flags, command hint, title, actions, insert/context
/// menus, and footer. Complex nested payloads (`selection`, `presentation`,
/// `commandHint`, `actions`) are kept as raw wire maps; `insertMenu`,
/// `contextMenu`, and `footer` decode through their spec types.
final class MarkdownEditorSpec {
  const MarkdownEditorSpec({
    this.text = '',
    this.selection,
    this.presentation,
    this.readOnly = false,
    this.dirty = false,
    this.placeholder = '',
    this.commandHint,
    this.title,
    this.actions,
    this.insertMenu,
    this.contextMenu,
    this.footer = const FooterActionsSpec(),
  });

  final String text;
  final Map<String, dynamic>? selection;
  final Map<String, dynamic>? presentation;
  final bool readOnly;
  final bool dirty;
  final String placeholder;
  final Map<String, dynamic>? commandHint;
  final String? title;
  final Map<String, dynamic>? actions;
  final MenuSpec? insertMenu;
  final MenuSpec? contextMenu;
  final FooterActionsSpec footer;

  factory MarkdownEditorSpec.fromJson(Map<String, dynamic> json) =>
      MarkdownEditorSpec(
        text: json['text'] as String? ?? '',
        selection: json['selection'] as Map<String, dynamic>?,
        presentation: json['presentation'] as Map<String, dynamic>?,
        readOnly: json['readOnly'] as bool? ?? false,
        dirty: json['dirty'] as bool? ?? false,
        placeholder: json['placeholder'] as String? ?? '',
        commandHint: json['commandHint'] as Map<String, dynamic>?,
        title: json['title'] as String?,
        actions: json['actions'] as Map<String, dynamic>?,
        insertMenu: json['insertMenu'] == null
            ? null
            : MenuSpec.fromJson(json['insertMenu'] as Map<String, dynamic>),
        contextMenu: json['contextMenu'] == null
            ? null
            : MenuSpec.fromJson(json['contextMenu'] as Map<String, dynamic>),
        footer: json['footer'] == null
            ? const FooterActionsSpec()
            : FooterActionsSpec.fromJson(
                json['footer'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'text': text,
        if (selection != null) 'selection': selection,
        if (presentation != null) 'presentation': presentation,
        'readOnly': readOnly,
        'dirty': dirty,
        'placeholder': placeholder,
        if (commandHint != null) 'commandHint': commandHint,
        if (title != null) 'title': title,
        if (actions != null) 'actions': actions,
        if (insertMenu != null) 'insertMenu': insertMenu!.toJson(),
        if (contextMenu != null) 'contextMenu': contextMenu!.toJson(),
        'footer': footer.toJson(),
      };

  MarkdownEditorSpec copyWith({
    String? text,
    Map<String, dynamic>? selection,
    Map<String, dynamic>? presentation,
    bool? readOnly,
    bool? dirty,
    String? placeholder,
    Map<String, dynamic>? commandHint,
    String? title,
    bool clearTitle = false,
    Map<String, dynamic>? actions,
    MenuSpec? insertMenu,
    bool clearInsertMenu = false,
    MenuSpec? contextMenu,
    bool clearContextMenu = false,
    FooterActionsSpec? footer,
  }) =>
      MarkdownEditorSpec(
        text: text ?? this.text,
        selection: selection ?? this.selection,
        presentation: presentation ?? this.presentation,
        readOnly: readOnly ?? this.readOnly,
        dirty: dirty ?? this.dirty,
        placeholder: placeholder ?? this.placeholder,
        commandHint: commandHint ?? this.commandHint,
        title: clearTitle ? null : (title ?? this.title),
        actions: actions ?? this.actions,
        insertMenu:
            clearInsertMenu ? null : (insertMenu ?? this.insertMenu),
        contextMenu:
            clearContextMenu ? null : (contextMenu ?? this.contextMenu),
        footer: footer ?? this.footer,
      );

  @override
  bool operator ==(Object other) =>
      other is MarkdownEditorSpec &&
      other.text == text &&
      other.readOnly == readOnly &&
      other.dirty == dirty &&
      other.placeholder == placeholder &&
      other.title == title &&
      other.footer == footer;

  @override
  int get hashCode =>
      Object.hash(text, readOnly, dirty, placeholder, title);
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
  const SurfaceSpec({this.child, this.reference});

  final AppKitNode? child;
  final Map<String, dynamic>? reference;

  factory SurfaceSpec.fromJson(Map<String, dynamic> json) => SurfaceSpec(
        child: json['child'] == null
            ? null
            : AppKitNode.fromJson(json['child'] as Map<String, dynamic>),
        reference: json['reference'] as Map<String, dynamic>?,
      );

  Map<String, dynamic> toJson() => {
        if (child != null) 'child': child!.toJson(),
        if (reference != null) 'reference': reference,
      };

  SurfaceSpec copyWith({Map<String, dynamic>? reference}) => SurfaceSpec(
        child: child,
        reference: reference ?? this.reference,
      );

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

/// Mirrors Swift `UISnapshot`.
///
/// The snapshot carries the route envelope (protocol name/version, app
/// instance, client, view) plus the revision. A delta applies only when its
/// route matches and its `baseRevision` equals the snapshot's `revision`.
final class AppKitSnapshot {
  const AppKitSnapshot({
    this.protocolName = 'supercli-ui-v1',
    required this.appInstanceId,
    required this.clientId,
    this.viewId = '',
    this.revision = 0,
    required this.root,
    this.protocolVersion = AppKitProtocol.version,
  });

  final String protocolName;
  final String appInstanceId;
  final String clientId;
  final String viewId;
  final int revision;
  final AppKitNode root;
  final int protocolVersion;

  bool get versionSupported => AppKitProtocol.supports(protocolVersion);

  factory AppKitSnapshot.fromJson(Map<String, dynamic> json) =>
      AppKitSnapshot(
        protocolName: json['protocol'] as String? ?? 'supercli-ui-v1',
        appInstanceId: json['appInstanceId'] as String,
        clientId: json['clientId'] as String,
        viewId: json['viewId'] as String? ?? '',
        revision: json['revision'] as int? ?? 0,
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
