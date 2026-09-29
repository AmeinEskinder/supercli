/// App-kit UI protocol: list, chart, content, input, page, tree, textbox
/// spec types.
/// Faithful wire port of the corresponding sections of
/// `clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift`
/// (4216 lines).
///
/// Covers `UIListItemTone`, `UIListItemEmphasis`, `UIListItemTextRun`,
/// `UIListItemActionRole`, `UIListItemPrimaryRole`, `UIListPageBehavior`,
/// `UISparklineSpec`, `UIBarChartEmphasis`, `UIBarChartBar`,
/// `UIBarChartSpec`, `UILineChartPoint`, `UILineChartSeries`,
/// `UILineChartBounds`, `UILineChartAxis`, `UILineChartSpec`, `UIGaugeSpec`,
/// `UIListItemSlot`, `UIListRowLayout`, `UIListItemBand`,
/// `UIListItemMediaSide`, `UIListItemMedia`, `UIListItemSpec`, `UIListSpec`,
/// `UIContentFont`, `UIContentTone`, `UIContentEmphasis`,
/// `UIContentLineTone`, `UIContentRun`, `UIContentLine`,
/// `UIContentSelection`, `UIContentSpec`, `UIInputSpec`, `UIPageHeaderSlot`,
/// `UIPageBodySlot`, `UIPageTab`, `UIPageToolbar`, `PageSpec`,
/// `UITreePresentation`, `UITreeItemKind`, `UITreeChildState`, `UITreeItem`,
/// `UITreeFilter`, `UITreeActions`, `UITreeSpec`, `TextBoxTitlePosition`,
/// `TextBoxSubmitMode`, `TextBoxTitle`, `TextBoxKeyHint`, `TextBoxBusy`,
/// `TextBoxActions`, `TextBoxSpec`, and `UIComponent`, plus the
/// `listItemRunsAreValid`, `chartIdentifierIsValid`,
/// `chartSingleLineIsValid`, `chartAccessibilityIsValid`, and
/// `validateTreeItems` helpers.
///
/// Wire compatibility: JSON keys match the Swift `CodingKeys` exactly.
/// Decode-time validation mirrors the Swift `init(from:)` guards; violations
/// throw [FormatException].
///
/// NOTE: `default` and `static` are reserved words in Dart, so the Swift
/// `.default` cases are named `standard` (wire `"default"`) and
/// `UIListItemPrimaryRole.static` is named `staticRow`.
library;

import 'dart:convert';

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

const int _uInt16Max = 65535;

bool _isAlnum(int c) =>
    (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122);

// ---------------------------------------------------------------------------
// List item text runs
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIListItemTone` (`.default` encodes as `"default"`).
enum UIListItemTone {
  standard('default'),
  muted('muted'),
  accent('accent'),
  info('info'),
  success('success'),
  warning('warning'),
  danger('danger');

  const UIListItemTone(this.wire);
  final String wire;

  static UIListItemTone fromJson(String v) => UIListItemTone.values
      .firstWhere((e) => e.wire == v,
          orElse: () => throw FormatException('Unknown UIListItemTone $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIListItemEmphasis`.
enum UIListItemEmphasis {
  regular,
  strong;

  static UIListItemEmphasis fromJson(String v) =>
      UIListItemEmphasis.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIListItemTextRun`.
final class UIListItemTextRun {
  const UIListItemTextRun({required this.text, this.tone, this.emphasis});

  final String text;
  final UIListItemTone? tone;
  final UIListItemEmphasis? emphasis;

  factory UIListItemTextRun.fromJson(Map<String, dynamic> json) =>
      UIListItemTextRun(
        text: json['text'] as String,
        tone: json['tone'] == null
            ? null
            : UIListItemTone.fromJson(json['tone'] as String),
        emphasis: json['emphasis'] == null
            ? null
            : UIListItemEmphasis.fromJson(json['emphasis'] as String),
      );

  Map<String, dynamic> toJson() => {
        'text': text,
        if (tone != null) 'tone': tone!.toJson(),
        if (emphasis != null) 'emphasis': emphasis!.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIListItemTextRun &&
      other.text == text &&
      other.tone == tone &&
      other.emphasis == emphasis;

  @override
  int get hashCode => Object.hash(text, tone, emphasis);
}

/// Mirrors Swift `listItemRunsAreValid(_:fallback:)`.
bool listItemRunsAreValid(List<UIListItemTextRun> runs, String? fallback) {
  if (runs.isEmpty) return true;
  if (runs.length > 256 || fallback == null) return false;
  if (runs.map((r) => r.text).join('') != fallback) return false;
  return runs.every((r) =>
      r.text.isNotEmpty &&
      _utf8Length(r.text) <= 16384 &&
      !r.text.contains('\n') &&
      !r.text.contains('\r') &&
      !r.text.contains('\x00'));
}

/// Mirrors Swift `UIListItemActionRole` (`.default` encodes as `"default"`).
enum UIListItemActionRole {
  standard('default'),
  destructive('destructive');

  const UIListItemActionRole(this.wire);
  final String wire;

  static UIListItemActionRole fromJson(String v) =>
      UIListItemActionRole.values.firstWhere((e) => e.wire == v,
          orElse: () =>
              throw FormatException('Unknown UIListItemActionRole $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIListItemPrimaryRole` (not Codable in Swift).
enum UIListItemPrimaryRole {
  /// Mirrors Swift `.static` (`static` is a Dart keyword).
  staticRow,
  toggle,
  checkmark,
  disclosure,
  command,
  destructive,
}

/// Mirrors Swift `UIListPageBehavior`.
enum UIListPageBehavior {
  selection,
  scroll;

  static UIListPageBehavior fromJson(String v) =>
      UIListPageBehavior.values.byName(v);
  String toJson() => name;
}

// ---------------------------------------------------------------------------
// Chart validators
// ---------------------------------------------------------------------------

/// Mirrors Swift `chartIdentifierIsValid(_:)`.
bool chartIdentifierIsValid(String value) {
  if (value.isEmpty || _utf8Length(value) > 256) return false;
  const extra = {46, 95, 58, 47, 45}; // . _ : / -
  return value.codeUnits.every((c) => _isAlnum(c) || extra.contains(c));
}

/// Mirrors Swift `chartSingleLineIsValid(_:allowEmpty:)`.
bool chartSingleLineIsValid(String value, {required bool allowEmpty}) {
  return _utf8Length(value) <= 4096 &&
      !value.contains('\n') &&
      !value.contains('\r') &&
      !value.contains('\x00') &&
      (allowEmpty || value.trim().isNotEmpty);
}

/// Mirrors Swift `chartAccessibilityIsValid(_:)`.
bool chartAccessibilityIsValid(String value) {
  return _utf8Length(value) <= 16384 &&
      value.trim().isNotEmpty &&
      !value.contains('\r') &&
      !value.contains('\x00');
}

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

/// Mirrors Swift `UISparklineSpec`, including `resolvedBounds` and
/// `normalizedSeries`.
final class UISparklineSpec {
  const UISparklineSpec({
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

  bool get isValid {
    if (series.length < 1 || series.length > 100000) return false;
    if (!series.every((v) => v.isFinite)) return false;
    final lo = min;
    final hi = max;
    if (lo != null && !lo.isFinite) return false;
    if (hi != null && !hi.isFinite) return false;
    if (lo != null && !series.every((v) => v >= lo)) return false;
    if (hi != null && !series.every((v) => v <= hi)) return false;
    if (lo != null && hi != null && !(lo < hi)) return false;
    if (!chartAccessibilityIsValid(accessibilityText)) return false;
    for (final s in [caption, unit]) {
      if (s != null && !chartSingleLineIsValid(s, allowEmpty: true)) {
        return false;
      }
    }
    if (!chartIdentifierIsValid(id)) return false;
    if (activate != null && !chartIdentifierIsValid(activate!)) return false;
    return true;
  }

  factory UISparklineSpec.fromJson(Map<String, dynamic> json) {
    final spec = UISparklineSpec(
      id: json['id'] as String,
      series:
          (json['series'] as List).map((v) => (v as num).toDouble()).toList(),
      min: (json['min'] as num?)?.toDouble(),
      max: (json['max'] as num?)?.toDouble(),
      caption: json['caption'] as String?,
      unit: json['unit'] as String?,
      accessibilityText: json['accessibilityText'] as String,
      activate: json['activate'] as String?,
    );
    if (!spec.isValid) {
      throw FormatException(
          'Sparkline needs finite data, containing bounds, and accessibility text',
          json);
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

  /// Mirrors Swift `UISparklineSpec.resolvedBounds`: inferred bounds include
  /// zero and an all-zero series expands to 0...1.
  ({double lower, double upper}) get resolvedBounds {
    final seriesMinimum =
        series.fold<double>(double.infinity, (a, b) => a < b ? a : b);
    final seriesMaximum =
        series.fold<double>(double.negativeInfinity, (a, b) => a > b ? a : b);
    final lower = min ?? (seriesMinimum < 0 ? seriesMinimum : 0);
    var upper = max ?? (seriesMaximum > 0 ? seriesMaximum : 0);
    if (lower == upper) upper = lower + 1;
    return (lower: lower, upper: upper);
  }

  /// Mirrors Swift `UISparklineSpec.normalizedSeries`.
  List<double> get normalizedSeries {
    final bounds = resolvedBounds;
    final range = bounds.upper - bounds.lower;
    return series
        .map((v) => ((v - bounds.lower) / range).clamp(0.0, 1.0))
        .toList();
  }

  @override
  bool operator ==(Object other) =>
      other is UISparklineSpec &&
      other.id == id &&
      _listEq(other.series, series) &&
      other.min == min &&
      other.max == max &&
      other.caption == caption &&
      other.unit == unit &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode => Object.hash(
      id, series.length, min, max, caption, unit, accessibilityText, activate);
}

/// Mirrors Swift `UIBarChartEmphasis` (`.standard` encodes as `"default"`).
enum UIBarChartEmphasis {
  standard('default'),
  accent('accent'),
  danger('danger');

  const UIBarChartEmphasis(this.wire);
  final String wire;

  static UIBarChartEmphasis fromJson(String v) =>
      UIBarChartEmphasis.values.firstWhere((e) => e.wire == v,
          orElse: () =>
              throw FormatException('Unknown UIBarChartEmphasis $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIBarChartBar`.
final class UIBarChartBar {
  const UIBarChartBar({
    required this.label,
    required this.value,
    this.valueCaption,
    this.emphasis = UIBarChartEmphasis.standard,
  });

  final String label;
  final double value;
  final String? valueCaption;
  final UIBarChartEmphasis emphasis;

  bool get isValid =>
      chartSingleLineIsValid(label, allowEmpty: false) &&
      value.isFinite &&
      value >= 0 &&
      (valueCaption == null ||
          chartSingleLineIsValid(valueCaption!, allowEmpty: true));

  factory UIBarChartBar.fromJson(Map<String, dynamic> json) =>
      UIBarChartBar(
        label: json['label'] as String,
        value: (json['value'] as num).toDouble(),
        valueCaption: json['valueCaption'] as String?,
        emphasis: json['emphasis'] == null
            ? UIBarChartEmphasis.standard
            : UIBarChartEmphasis.fromJson(json['emphasis'] as String),
      );

  Map<String, dynamic> toJson() => {
        'label': label,
        'value': value,
        if (valueCaption != null) 'valueCaption': valueCaption,
        'emphasis': emphasis.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIBarChartBar &&
      other.label == label &&
      other.value == value &&
      other.valueCaption == valueCaption &&
      other.emphasis == emphasis;

  @override
  int get hashCode => Object.hash(label, value, valueCaption, emphasis);
}

/// Mirrors Swift `UIBarChartSpec`, including `normalizedValues`.
final class UIBarChartSpec {
  const UIBarChartSpec({
    required this.id,
    required this.bars,
    required this.accessibilityText,
    this.activate,
  });

  final String id;
  final List<UIBarChartBar> bars;
  final String accessibilityText;
  final String? activate;

  bool get isValid =>
      chartIdentifierIsValid(id) &&
      bars.length >= 1 &&
      bars.length <= 1000 &&
      bars.every((b) => b.isValid) &&
      chartAccessibilityIsValid(accessibilityText) &&
      (activate == null || chartIdentifierIsValid(activate!));

  factory UIBarChartSpec.fromJson(Map<String, dynamic> json) {
    final spec = UIBarChartSpec(
      id: json['id'] as String,
      bars: (json['bars'] as List)
          .map((b) => UIBarChartBar.fromJson(b as Map<String, dynamic>))
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

  /// Mirrors Swift `UIBarChartSpec.normalizedValues`.
  List<double> get normalizedValues {
    final maximum =
        bars.map((b) => b.value).fold<double>(0, (a, b) => a > b ? a : b);
    final divisor = maximum > 1 ? maximum : 1;
    return bars.map((b) => (b.value / divisor).clamp(0.0, 1.0)).toList();
  }

  @override
  bool operator ==(Object other) =>
      other is UIBarChartSpec &&
      other.id == id &&
      _listEq(other.bars, bars) &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode =>
      Object.hash(id, bars.length, accessibilityText, activate);
}

/// Mirrors Swift `UILineChartPoint`.
final class UILineChartPoint {
  const UILineChartPoint({required this.x, required this.y});

  final double x;
  final double y;

  factory UILineChartPoint.fromJson(Map<String, dynamic> json) =>
      UILineChartPoint(
        x: (json['x'] as num).toDouble(),
        y: (json['y'] as num).toDouble(),
      );

  Map<String, dynamic> toJson() => {'x': x, 'y': y};

  @override
  bool operator ==(Object other) =>
      other is UILineChartPoint && other.x == x && other.y == y;

  @override
  int get hashCode => Object.hash(x, y);
}

/// Mirrors Swift `UILineChartSeries`.
final class UILineChartSeries {
  const UILineChartSeries({required this.name, required this.points});

  final String name;
  final List<UILineChartPoint> points;

  factory UILineChartSeries.fromJson(Map<String, dynamic> json) =>
      UILineChartSeries(
        name: json['name'] as String,
        points: (json['points'] as List)
            .map((p) => UILineChartPoint.fromJson(p as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'name': name,
        'points': points.map((p) => p.toJson()).toList(),
      };

  @override
  bool operator ==(Object other) =>
      other is UILineChartSeries &&
      other.name == name &&
      _listEq(other.points, points);

  @override
  int get hashCode => Object.hash(name, points.length);
}

/// Mirrors Swift `UILineChartBounds`.
final class UILineChartBounds {
  const UILineChartBounds({required this.min, required this.max});

  final double min;
  final double max;

  bool get isValid => min.isFinite && max.isFinite && min < max;

  factory UILineChartBounds.fromJson(Map<String, dynamic> json) =>
      UILineChartBounds(
        min: (json['min'] as num).toDouble(),
        max: (json['max'] as num).toDouble(),
      );

  Map<String, dynamic> toJson() => {'min': min, 'max': max};

  @override
  bool operator ==(Object other) =>
      other is UILineChartBounds && other.min == min && other.max == max;

  @override
  int get hashCode => Object.hash(min, max);
}

/// Mirrors Swift `UILineChartAxis`.
final class UILineChartAxis {
  const UILineChartAxis({this.bounds, this.label});

  final UILineChartBounds? bounds;
  final String? label;

  factory UILineChartAxis.fromJson(Map<String, dynamic> json) =>
      UILineChartAxis(
        bounds: json['bounds'] == null
            ? null
            : UILineChartBounds.fromJson(
                json['bounds'] as Map<String, dynamic>),
        label: json['label'] as String?,
      );

  Map<String, dynamic> toJson() => {
        if (bounds != null) 'bounds': bounds!.toJson(),
        if (label != null) 'label': label,
      };

  @override
  bool operator ==(Object other) =>
      other is UILineChartAxis &&
      other.bounds == bounds &&
      other.label == label;

  @override
  int get hashCode => Object.hash(bounds, label);
}

/// Mirrors Swift `UILineChartSpec`, including `resolvedXBounds` /
/// `resolvedYBounds`.
final class UILineChartSpec {
  const UILineChartSpec({
    required this.id,
    required this.series,
    this.xAxis = const UILineChartAxis(),
    this.yAxis = const UILineChartAxis(),
    required this.accessibilityText,
    this.activate,
  });

  final String id;
  final List<UILineChartSeries> series;
  final UILineChartAxis xAxis;
  final UILineChartAxis yAxis;
  final String accessibilityText;
  final String? activate;

  static bool _axisIsValid(UILineChartAxis axis, List<double> values) {
    if (axis.label != null &&
        !chartSingleLineIsValid(axis.label!, allowEmpty: true)) {
      return false;
    }
    if (axis.bounds != null && !axis.bounds!.isValid) return false;
    final bounds = axis.bounds;
    if (bounds == null) return true;
    return values.every((v) => v >= bounds.min && v <= bounds.max);
  }

  bool get isValid {
    if (!chartIdentifierIsValid(id)) return false;
    if (series.length < 1 || series.length > 16) return false;
    if (series.map((s) => s.name).toSet().length != series.length) {
      return false;
    }
    for (final s in series) {
      if (!chartSingleLineIsValid(s.name, allowEmpty: false)) return false;
      if (s.points.isEmpty) return false;
      if (!s.points.every((p) => p.x.isFinite && p.y.isFinite)) return false;
    }
    final points = series.expand((s) => s.points).toList();
    if (points.length > 100000) return false;
    if (!_axisIsValid(xAxis, points.map((p) => p.x).toList())) return false;
    if (!_axisIsValid(yAxis, points.map((p) => p.y).toList())) return false;
    if (!chartAccessibilityIsValid(accessibilityText)) return false;
    if (activate != null && !chartIdentifierIsValid(activate!)) return false;
    return true;
  }

  factory UILineChartSpec.fromJson(Map<String, dynamic> json) {
    final spec = UILineChartSpec(
      id: json['id'] as String,
      series: (json['series'] as List)
          .map((s) => UILineChartSeries.fromJson(s as Map<String, dynamic>))
          .toList(),
      xAxis: json['xAxis'] == null
          ? const UILineChartAxis()
          : UILineChartAxis.fromJson(json['xAxis'] as Map<String, dynamic>),
      yAxis: json['yAxis'] == null
          ? const UILineChartAxis()
          : UILineChartAxis.fromJson(json['yAxis'] as Map<String, dynamic>),
      accessibilityText: json['accessibilityText'] as String,
      activate: json['activate'] as String?,
    );
    if (!spec.isValid) {
      throw FormatException(
          'LineChart needs finite named series, containing axes, and accessibility text',
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

  static ({double lower, double upper}) _resolvedBounds(
      UILineChartAxis axis, List<double> values) {
    final bounds = axis.bounds;
    if (bounds != null) return (lower: bounds.min, upper: bounds.max);
    final lower = values.fold<double>(
        double.infinity, (a, b) => a < b ? a : b);
    var upper = values.fold<double>(
        double.negativeInfinity, (a, b) => a > b ? a : b);
    if (lower == upper) upper = lower + 1;
    return (lower: lower, upper: upper);
  }

  /// Mirrors Swift `UILineChartSpec.resolvedXBounds`.
  ({double lower, double upper}) get resolvedXBounds =>
      _resolvedBounds(xAxis, series.expand((s) => s.points).map((p) => p.x).toList());

  /// Mirrors Swift `UILineChartSpec.resolvedYBounds`.
  ({double lower, double upper}) get resolvedYBounds =>
      _resolvedBounds(yAxis, series.expand((s) => s.points).map((p) => p.y).toList());

  @override
  bool operator ==(Object other) =>
      other is UILineChartSpec &&
      other.id == id &&
      _listEq(other.series, series) &&
      other.xAxis == xAxis &&
      other.yAxis == yAxis &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode => Object.hash(
      id, series.length, xAxis, yAxis, accessibilityText, activate);
}

/// Mirrors Swift `UIGaugeSpec`, including the label math.
final class UIGaugeSpec {
  const UIGaugeSpec({
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

  bool get isValid =>
      chartIdentifierIsValid(id) &&
      ratio.isFinite &&
      ratio >= 0 &&
      ratio <= 1 &&
      chartSingleLineIsValid(label, allowEmpty: false) &&
      (caption == null ||
          chartSingleLineIsValid(caption!, allowEmpty: false)) &&
      chartAccessibilityIsValid(accessibilityText) &&
      (activate == null || chartIdentifierIsValid(activate!));

  factory UIGaugeSpec.fromJson(Map<String, dynamic> json) {
    final spec = UIGaugeSpec(
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

  /// Mirrors Swift `UIGaugeSpec.percentageLabel`.
  String get percentageLabel => '$label  $valueLabel';

  /// Mirrors Swift `UIGaugeSpec.percentageValueLabel`.
  String get percentageValueLabel => '${(ratio * 100).round()}%';

  /// Mirrors Swift `UIGaugeSpec.valueLabel`.
  String get valueLabel => caption ?? percentageValueLabel;

  @override
  bool operator ==(Object other) =>
      other is UIGaugeSpec &&
      other.id == id &&
      other.ratio == ratio &&
      other.label == label &&
      other.caption == caption &&
      other.accessibilityText == accessibilityText &&
      other.activate == activate;

  @override
  int get hashCode =>
      Object.hash(id, ratio, label, caption, accessibilityText, activate);
}

// ---------------------------------------------------------------------------
// List item slots, bands, rows, media
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIStatusSymbolSpec`.
final class UIStatusSymbolSpec {
  const UIStatusSymbolSpec({
    required this.symbol,
    required this.label,
    this.tone = UIListItemTone.standard,
    this.emphasis = UIListItemEmphasis.regular,
    this.preserveToneWhenSelected = false,
  });

  final String symbol;
  final String label;
  final UIListItemTone tone;
  final UIListItemEmphasis emphasis;
  final bool preserveToneWhenSelected;

  factory UIStatusSymbolSpec.fromJson(Map<String, dynamic> json) {
    final symbol = json['symbol'] as String;
    if (symbol.isEmpty ||
        symbol.contains('\n') ||
        symbol.contains('\r')) {
      throw FormatException(
          'Status symbol must be a non-empty single line', json);
    }
    return UIStatusSymbolSpec(
      symbol: symbol,
      label: json['label'] as String,
      tone: json['tone'] == null
          ? UIListItemTone.standard
          : UIListItemTone.fromJson(json['tone'] as String),
      emphasis: json['emphasis'] == null
          ? UIListItemEmphasis.regular
          : UIListItemEmphasis.fromJson(json['emphasis'] as String),
      preserveToneWhenSelected:
          json['preserveToneWhenSelected'] as bool? ?? false,
    );
  }

  Map<String, dynamic> toJson() => {
        'symbol': symbol,
        'label': label,
        'tone': tone.toJson(),
        'emphasis': emphasis.toJson(),
        'preserveToneWhenSelected': preserveToneWhenSelected,
      };

  @override
  bool operator ==(Object other) =>
      other is UIStatusSymbolSpec &&
      other.symbol == symbol &&
      other.label == label &&
      other.tone == tone &&
      other.emphasis == emphasis &&
      other.preserveToneWhenSelected == preserveToneWhenSelected;

  @override
  int get hashCode =>
      Object.hash(symbol, label, tone, emphasis, preserveToneWhenSelected);
}

/// Mirrors Swift `UIBadgeSpec`.
final class UIBadgeSpec {
  const UIBadgeSpec({required this.text, this.tone = UIListItemTone.muted});

  final String text;
  final UIListItemTone tone;

  factory UIBadgeSpec.fromJson(Map<String, dynamic> json) {
    final text = json['text'] as String;
    if (text.contains('\n') || text.contains('\r')) {
      throw FormatException('Badge text must be a single line', json);
    }
    return UIBadgeSpec(
      text: text,
      tone: json['tone'] == null
          ? UIListItemTone.standard
          : UIListItemTone.fromJson(json['tone'] as String),
    );
  }

  Map<String, dynamic> toJson() => {
        'text': text,
        'tone': tone.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIBadgeSpec && other.text == text && other.tone == tone;

  @override
  int get hashCode => Object.hash(text, tone);
}

/// Mirrors Swift `UIListItemSlot` with its custom `type`-discriminated
/// Codable. Payloads are flat: the spec fields sit alongside `type`.
sealed class UIListItemSlot {
  const UIListItemSlot();

  String get kind;

  factory UIListItemSlot.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'toggle':
        return UIListItemSlotToggle(UIToggleSpec.fromJson(json));
      case 'status':
        return UIListItemSlotStatus(UIStatusSymbolSpec.fromJson(json));
      case 'badge':
        return UIListItemSlotBadge(UIBadgeSpec.fromJson(json));
      case 'sparkline':
        return UIListItemSlotSparkline(UISparklineSpec.fromJson(json));
      case 'gauge':
        return UIListItemSlotGauge(UIGaugeSpec.fromJson(json));
      case 'disclosure':
        return const UIListItemSlotDisclosure();
      case 'checkmark':
        return UIListItemSlotCheckmark(UICheckmarkSpec.fromJson(json));
      default:
        return UIListItemSlotUnsupported(json['type'] as String);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIListItemSlotToggle extends UIListItemSlot {
  const UIListItemSlotToggle(this.value);
  final UIToggleSpec value;
  @override
  String get kind => 'toggle';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemSlotToggle && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemSlotStatus extends UIListItemSlot {
  const UIListItemSlotStatus(this.value);
  final UIStatusSymbolSpec value;
  @override
  String get kind => 'status';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemSlotStatus && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemSlotBadge extends UIListItemSlot {
  const UIListItemSlotBadge(this.value);
  final UIBadgeSpec value;
  @override
  String get kind => 'badge';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemSlotBadge && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemSlotSparkline extends UIListItemSlot {
  const UIListItemSlotSparkline(this.value);
  final UISparklineSpec value;
  @override
  String get kind => 'sparkline';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemSlotSparkline && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemSlotGauge extends UIListItemSlot {
  const UIListItemSlotGauge(this.value);
  final UIGaugeSpec value;
  @override
  String get kind => 'gauge';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemSlotGauge && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemSlotDisclosure extends UIListItemSlot {
  const UIListItemSlotDisclosure();
  @override
  String get kind => 'disclosure';
  @override
  Map<String, dynamic> toJson() => {'type': kind};
  @override
  bool operator ==(Object other) => other is UIListItemSlotDisclosure;
  @override
  int get hashCode => 0;
}

final class UIListItemSlotCheckmark extends UIListItemSlot {
  const UIListItemSlotCheckmark(this.value);
  final UICheckmarkSpec value;
  @override
  String get kind => 'checkmark';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemSlotCheckmark && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemSlotUnsupported extends UIListItemSlot {
  const UIListItemSlotUnsupported(this.kind);
  @override
  final String kind;
  @override
  Map<String, dynamic> toJson() => {'type': kind};
  @override
  bool operator ==(Object other) =>
      other is UIListItemSlotUnsupported && other.kind == kind;
  @override
  int get hashCode => kind.hashCode;
}

/// Mirrors Swift `UIListRowLayout` with its custom type-discriminated Codable.
sealed class UIListRowLayout {
  const UIListRowLayout();

  String get type;

  factory UIListRowLayout.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'inline':
        return const UIListRowLayoutInline();
      case 'stacked':
        return const UIListRowLayoutStacked();
      case 'auto':
        return UIListRowLayoutAuto(
            stackBelowWidth: json['stackBelowWidth'] as int);
      default:
        throw FormatException('Unknown UIListRowLayout', json);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIListRowLayoutInline extends UIListRowLayout {
  const UIListRowLayoutInline();
  @override
  String get type => 'inline';
  @override
  Map<String, dynamic> toJson() => {'type': type};
  @override
  bool operator ==(Object other) => other is UIListRowLayoutInline;
  @override
  int get hashCode => 0;
}

final class UIListRowLayoutStacked extends UIListRowLayout {
  const UIListRowLayoutStacked();
  @override
  String get type => 'stacked';
  @override
  Map<String, dynamic> toJson() => {'type': type};
  @override
  bool operator ==(Object other) => other is UIListRowLayoutStacked;
  @override
  int get hashCode => 1;
}

final class UIListRowLayoutAuto extends UIListRowLayout {
  const UIListRowLayoutAuto({required this.stackBelowWidth});
  final int stackBelowWidth;
  @override
  String get type => 'auto';
  @override
  Map<String, dynamic> toJson() =>
      {'type': type, 'stackBelowWidth': stackBelowWidth};
  @override
  bool operator ==(Object other) =>
      other is UIListRowLayoutAuto &&
      other.stackBelowWidth == stackBelowWidth;
  @override
  int get hashCode => stackBelowWidth.hashCode;
}

/// Mirrors Swift `UIListItemBand` with its custom `type`-discriminated
/// Codable. Payloads are flat: the spec fields sit alongside `type`.
sealed class UIListItemBand {
  const UIListItemBand();

  String get kind;

  /// Mirrors Swift `UIListItemBand.id`: the chart id, or nil for
  /// text/divider/unsupported bands.
  String? get id {
    final self = this;
    if (self is UIListItemBandGauge) return self.value.id;
    if (self is UIListItemBandSparkline) return self.value.id;
    return null;
  }

  factory UIListItemBand.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'gauge':
        return UIListItemBandGauge(UIGaugeSpec.fromJson(json));
      case 'sparkline':
        return UIListItemBandSparkline(UISparklineSpec.fromJson(json));
      case 'text':
        return UIListItemBandText(
          json['text'] as String,
          json['tone'] == null
              ? UIListItemTone.standard
              : UIListItemTone.fromJson(json['tone'] as String),
        );
      case 'divider':
        return const UIListItemBandDivider();
      default:
        return UIListItemBandUnsupported(json['type'] as String);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIListItemBandGauge extends UIListItemBand {
  const UIListItemBandGauge(this.value);
  final UIGaugeSpec value;
  @override
  String get kind => 'gauge';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemBandGauge && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemBandSparkline extends UIListItemBand {
  const UIListItemBandSparkline(this.value);
  final UISparklineSpec value;
  @override
  String get kind => 'sparkline';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...value.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIListItemBandSparkline && other.value == value;
  @override
  int get hashCode => value.hashCode;
}

final class UIListItemBandText extends UIListItemBand {
  const UIListItemBandText(this.text, [this.tone = UIListItemTone.standard]);
  final String text;
  final UIListItemTone tone;
  @override
  String get kind => 'text';
  @override
  Map<String, dynamic> toJson() => {
        'type': kind,
        'text': text,
        if (tone != UIListItemTone.standard) 'tone': tone.toJson(),
      };
  @override
  bool operator ==(Object other) =>
      other is UIListItemBandText &&
      other.text == text &&
      other.tone == tone;
  @override
  int get hashCode => Object.hash(text, tone);
}

final class UIListItemBandDivider extends UIListItemBand {
  const UIListItemBandDivider();
  @override
  String get kind => 'divider';
  @override
  Map<String, dynamic> toJson() => {'type': kind};
  @override
  bool operator ==(Object other) => other is UIListItemBandDivider;
  @override
  int get hashCode => 0;
}

final class UIListItemBandUnsupported extends UIListItemBand {
  const UIListItemBandUnsupported(this.kind);
  @override
  final String kind;
  @override
  Map<String, dynamic> toJson() => {'type': kind};
  @override
  bool operator ==(Object other) =>
      other is UIListItemBandUnsupported && other.kind == kind;
  @override
  int get hashCode => kind.hashCode;
}

/// Mirrors Swift `UIListItemMediaSide`.
enum UIListItemMediaSide {
  leading,
  trailing;

  static UIListItemMediaSide fromJson(String v) =>
      UIListItemMediaSide.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIListItemMedia`.
final class UIListItemMedia {
  const UIListItemMedia({
    required this.media,
    this.side = UIListItemMediaSide.leading,
    this.tone = UIListItemTone.standard,
    this.width = 2,
  });

  final MediaSpec media;
  final UIListItemMediaSide side;
  final UIListItemTone tone;
  final int width;

  factory UIListItemMedia.fromJson(Map<String, dynamic> json) {
    final media = MediaSpec.fromJson(json);
    final width = json['width'] as int? ?? 2;
    if (width < 1 || width > 12) {
      throw FormatException('Media width must be 1...12', json);
    }
    return UIListItemMedia(
      media: media,
      side: json['side'] == null
          ? UIListItemMediaSide.leading
          : UIListItemMediaSide.fromJson(json['side'] as String),
      tone: json['tone'] == null
          ? UIListItemTone.standard
          : UIListItemTone.fromJson(json['tone'] as String),
      width: width,
    );
  }

  Map<String, dynamic> toJson() => {
        ...media.toJson(),
        if (side != UIListItemMediaSide.leading) 'side': side.toJson(),
        if (tone != UIListItemTone.standard) 'tone': tone.toJson(),
        'width': width,
      };

  @override
  bool operator ==(Object other) =>
      other is UIListItemMedia &&
      other.media == media &&
      other.side == side &&
      other.tone == tone &&
      other.width == width;

  @override
  int get hashCode => Object.hash(media, side, tone, width);
}

/// Mirrors Swift `UIListItemSpec`. All slot/band invariants are enforced in
/// [UIListItemSpec.fromJson], mirroring Swift's `init(from:)` guards;
/// violations throw [FormatException].
final class UIListItemSpec {
  const UIListItemSpec({
    required this.id,
    required this.label,
    this.labelRuns = const [],
    this.labelTone = UIListItemTone.standard,
    this.emphasis = UIListItemEmphasis.regular,
    this.detail,
    this.detailRuns = const [],
    this.value,
    this.valueRuns = const [],
    this.valueTone = UIListItemTone.muted,
    this.valueMinWidth,
    this.done = false,
    this.busy = false,
    this.leading,
    this.trailing,
    this.accessory,
    this.top,
    this.bottom,
    this.media,
    this.divider = false,
    this.delete,
    this.activate,
    this.actionRole = UIListItemActionRole.standard,
  });

  final String id;
  final String label;
  final List<UIListItemTextRun> labelRuns;
  final UIListItemTone labelTone;
  final UIListItemEmphasis emphasis;
  final String? detail;
  final List<UIListItemTextRun> detailRuns;
  final String? value;
  final List<UIListItemTextRun> valueRuns;
  final UIListItemTone valueTone;
  final int? valueMinWidth;
  final bool done;
  final bool busy;
  final UIListItemSlot? leading;
  final UIListItemSlot? trailing;
  final UIListItemSlot? accessory;

  /// Full-width band above the text rows.
  final UIListItemBand? top;

  /// Full-width band below the text rows.
  final UIListItemBand? bottom;

  /// Media column spanning the item.
  final UIListItemMedia? media;

  /// A passive separator row; `label` is an optional caption.
  final bool divider;
  final String? delete;
  final String? activate;
  final UIListItemActionRole actionRole;

  Iterable<UIListItemSlot> get _slots sync* {
    if (leading != null) yield leading!;
    if (trailing != null) yield trailing!;
    if (accessory != null) yield accessory!;
  }

  static List<UIToggleSpec> _togglesOf(UIListItemSpec item) =>
      item._slots.whereType<UIListItemSlotToggle>().map((s) => s.value).toList();

  static List<UICheckmarkSpec> _checkmarksOf(UIListItemSpec item) => item._slots
      .whereType<UIListItemSlotCheckmark>()
      .map((s) => s.value)
      .toList();

  static List<UISparklineSpec> _sparklinesOf(UIListItemSpec item) => item._slots
      .whereType<UIListItemSlotSparkline>()
      .map((s) => s.value)
      .toList();

  static List<UIGaugeSpec> _gaugesOf(UIListItemSpec item) => item._slots
      .whereType<UIListItemSlotGauge>()
      .map((s) => s.value)
      .toList();

  static void _validate(UIListItemSpec item, Map<String, dynamic> json) {
    Never fail(String message) =>
        throw FormatException(message, json);
    final strings = [item.label, item.detail, item.value]
        .whereType<String>()
        .toList();
    if (strings.any((s) => s.contains('\n') || s.contains('\r'))) {
      fail('ListItem label, detail, and value must be single-line');
    }
    if (!listItemRunsAreValid(item.labelRuns, item.label)) {
      fail('ListItem label, detail, and value must be single-line');
    }
    if (!listItemRunsAreValid(item.detailRuns, item.detail)) {
      fail('ListItem label, detail, and value must be single-line');
    }
    if (!listItemRunsAreValid(item.valueRuns, item.value)) {
      fail('ListItem label, detail, and value must be single-line');
    }
    final minWidth = item.valueMinWidth;
    if (minWidth != null && (minWidth < 0 || minWidth > _uInt16Max)) {
      fail('ListItem valueMinWidth must fit in UInt16');
    }
    final toggles = _togglesOf(item);
    if (toggles.length > 1) {
      fail('ListItem accepts one completion Toggle whose value matches done');
    }
    UIToggleSpec? completion;
    for (final t in toggles) {
      if (t.role == UIToggleRole.completion) {
        completion = t;
        break;
      }
    }
    if ((completion?.value ?? item.done) != item.done) {
      fail('ListItem accepts one completion Toggle whose value matches done');
    }
    final checkmarks = _checkmarksOf(item);
    final disclosures =
        item._slots.whereType<UIListItemSlotDisclosure>().toList();
    final sparklines = _sparklinesOf(item);
    final gauges = _gaugesOf(item);
    if (checkmarks.length > 1 || disclosures.length > 1) {
      fail('ListItem accepts one checkmark or disclosure accessory');
    }
    if (checkmarks.isNotEmpty && item.accessory is! UIListItemSlotCheckmark) {
      fail('Checkmark is accepted only in the accessory slot');
    }
    if (disclosures.isNotEmpty &&
        (item.accessory is! UIListItemSlotDisclosure ||
            item.activate == null)) {
      fail('Disclosure accessory requires activate');
    }
    if (sparklines.isNotEmpty &&
        (sparklines.length != 1 ||
            item.trailing is! UIListItemSlotSparkline)) {
      fail('Sparkline is accepted only once in the trailing slot');
    }
    if (gauges.isNotEmpty &&
        (gauges.length != 1 || item.trailing is! UIListItemSlotGauge)) {
      fail('Gauge is accepted only once in the trailing slot');
    }
    final independentRoles = (toggles.isEmpty ? 0 : 1) +
        (checkmarks.isEmpty ? 0 : 1) +
        (disclosures.isEmpty ? 0 : 1) +
        (sparklines.any((s) => s.activate != null) ? 1 : 0) +
        (gauges.any((g) => g.activate != null) ? 1 : 0) +
        (item.activate != null && disclosures.isEmpty ? 1 : 0);
    if (independentRoles > 1) {
      fail('ListItem primary role is ambiguous');
    }
    if (item.actionRole == UIListItemActionRole.destructive &&
        !(item.activate != null && disclosures.isEmpty)) {
      fail('destructive is accepted only for a plain command row');
    }
  }

  /// Mirrors Swift `UIListItemSpec.primaryRole`.
  UIListItemPrimaryRole get primaryRole {
    if (divider) return UIListItemPrimaryRole.staticRow;
    if (_slots.any((s) => s is UIListItemSlotToggle)) {
      return UIListItemPrimaryRole.toggle;
    }
    if (_slots.any((s) => s is UIListItemSlotCheckmark)) {
      return UIListItemPrimaryRole.checkmark;
    }
    if (_slots.any((s) => s is UIListItemSlotDisclosure)) {
      return UIListItemPrimaryRole.disclosure;
    }
    final sparkline = primarySparkline;
    if (sparkline != null && sparkline.activate != null) {
      return UIListItemPrimaryRole.command;
    }
    final gauge = primaryGauge;
    if (gauge != null && gauge.activate != null) {
      return UIListItemPrimaryRole.command;
    }
    if (activate != null) {
      return actionRole == UIListItemActionRole.destructive
          ? UIListItemPrimaryRole.destructive
          : UIListItemPrimaryRole.command;
    }
    return UIListItemPrimaryRole.staticRow;
  }

  /// Mirrors Swift `UIListItemSpec.primaryToggle`.
  UIToggleSpec? get primaryToggle {
    for (final slot in _slots) {
      if (slot is UIListItemSlotToggle) return slot.value;
    }
    return null;
  }

  /// Mirrors Swift `UIListItemSpec.primaryCheckmark`.
  UICheckmarkSpec? get primaryCheckmark {
    for (final slot in _slots) {
      if (slot is UIListItemSlotCheckmark) return slot.value;
    }
    return null;
  }

  /// Mirrors Swift `UIListItemSpec.primarySparkline`.
  UISparklineSpec? get primarySparkline {
    for (final slot in _slots) {
      if (slot is UIListItemSlotSparkline) return slot.value;
    }
    return null;
  }

  /// Mirrors Swift `UIListItemSpec.primaryGauge`.
  UIGaugeSpec? get primaryGauge {
    for (final slot in _slots) {
      if (slot is UIListItemSlotGauge) return slot.value;
    }
    return null;
  }

  factory UIListItemSpec.fromJson(Map<String, dynamic> json) {
    final item = UIListItemSpec(
      id: json['id'] as String,
      label: json['label'] as String,
      labelRuns: ((json['labelRuns'] as List?) ?? [])
          .map((r) => UIListItemTextRun.fromJson(r as Map<String, dynamic>))
          .toList(),
      labelTone: json['labelTone'] == null
          ? UIListItemTone.standard
          : UIListItemTone.fromJson(json['labelTone'] as String),
      emphasis: json['emphasis'] == null
          ? UIListItemEmphasis.regular
          : UIListItemEmphasis.fromJson(json['emphasis'] as String),
      detail: json['detail'] as String?,
      detailRuns: ((json['detailRuns'] as List?) ?? [])
          .map((r) => UIListItemTextRun.fromJson(r as Map<String, dynamic>))
          .toList(),
      value: json['value'] as String?,
      valueRuns: ((json['valueRuns'] as List?) ?? [])
          .map((r) => UIListItemTextRun.fromJson(r as Map<String, dynamic>))
          .toList(),
      valueTone: json['valueTone'] == null
          ? UIListItemTone.muted
          : UIListItemTone.fromJson(json['valueTone'] as String),
      valueMinWidth: json['valueMinWidth'] as int?,
      done: json['done'] as bool? ?? false,
      busy: json['busy'] as bool? ?? false,
      leading: json['leading'] == null
          ? null
          : UIListItemSlot.fromJson(json['leading'] as Map<String, dynamic>),
      trailing: json['trailing'] == null
          ? null
          : UIListItemSlot.fromJson(json['trailing'] as Map<String, dynamic>),
      accessory: json['accessory'] == null
          ? null
          : UIListItemSlot.fromJson(json['accessory'] as Map<String, dynamic>),
      top: json['top'] == null
          ? null
          : UIListItemBand.fromJson(json['top'] as Map<String, dynamic>),
      bottom: json['bottom'] == null
          ? null
          : UIListItemBand.fromJson(json['bottom'] as Map<String, dynamic>),
      media: json['media'] == null
          ? null
          : UIListItemMedia.fromJson(json['media'] as Map<String, dynamic>),
      divider: json['divider'] as bool? ?? false,
      delete: json['delete'] as String?,
      activate: json['activate'] as String?,
      actionRole: json['actionRole'] == null
          ? UIListItemActionRole.standard
          : UIListItemActionRole.fromJson(json['actionRole'] as String),
    );
    _validate(item, json);
    return item;
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        if (labelRuns.isNotEmpty)
          'labelRuns': labelRuns.map((r) => r.toJson()).toList(),
        if (labelTone != UIListItemTone.standard)
          'labelTone': labelTone.toJson(),
        if (emphasis != UIListItemEmphasis.regular)
          'emphasis': emphasis.toJson(),
        if (detail != null) 'detail': detail,
        if (detailRuns.isNotEmpty)
          'detailRuns': detailRuns.map((r) => r.toJson()).toList(),
        if (value != null) 'value': value,
        if (valueRuns.isNotEmpty)
          'valueRuns': valueRuns.map((r) => r.toJson()).toList(),
        if (valueTone != UIListItemTone.muted) 'valueTone': valueTone.toJson(),
        if (valueMinWidth != null) 'valueMinWidth': valueMinWidth,
        'done': done,
        'busy': busy,
        if (leading != null) 'leading': leading!.toJson(),
        if (trailing != null) 'trailing': trailing!.toJson(),
        if (accessory != null) 'accessory': accessory!.toJson(),
        if (top != null) 'top': top!.toJson(),
        if (bottom != null) 'bottom': bottom!.toJson(),
        if (media != null) 'media': media!.toJson(),
        'divider': divider,
        if (delete != null) 'delete': delete,
        if (activate != null) 'activate': activate,
        if (actionRole != UIListItemActionRole.standard)
          'actionRole': actionRole.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIListItemSpec &&
      other.id == id &&
      other.label == label &&
      _listEq(other.labelRuns, labelRuns) &&
      other.labelTone == labelTone &&
      other.emphasis == emphasis &&
      other.detail == detail &&
      _listEq(other.detailRuns, detailRuns) &&
      other.value == value &&
      _listEq(other.valueRuns, valueRuns) &&
      other.valueTone == valueTone &&
      other.valueMinWidth == valueMinWidth &&
      other.done == done &&
      other.busy == busy &&
      other.leading == leading &&
      other.trailing == trailing &&
      other.accessory == accessory &&
      other.top == top &&
      other.bottom == bottom &&
      other.media == media &&
      other.divider == divider &&
      other.delete == delete &&
      other.activate == activate &&
      other.actionRole == actionRole;

  @override
  int get hashCode => Object.hash(id, label, done, divider);
}

/// Mirrors Swift `UIListSpec`.
final class UIListSpec {
  const UIListSpec({
    required this.id,
    required this.items,
    this.emptyMessage = '',
    this.selectedID,
    this.select,
    this.scrollPadding = 0,
    this.pageOverlap = 1,
    this.pageBehavior = UIListPageBehavior.selection,
    this.spacePagesDown = false,
    this.rowLayout = const UIListRowLayoutInline(),
    this.contextMenu,
  });

  final String id;
  final List<UIListItemSpec> items;
  final String emptyMessage;
  final String? selectedID;
  final String? select;
  final int scrollPadding;
  final int pageOverlap;
  final UIListPageBehavior pageBehavior;
  final bool spacePagesDown;
  final UIListRowLayout rowLayout;
  final UIMenuSpec? contextMenu;

  factory UIListSpec.fromJson(Map<String, dynamic> json) {
    final scrollPadding = json['scrollPadding'] as int? ?? 0;
    final pageOverlap = json['pageOverlap'] as int? ?? 1;
    if (scrollPadding < 0 ||
        scrollPadding > _uInt16Max ||
        pageOverlap < 0 ||
        pageOverlap > _uInt16Max) {
      throw FormatException(
          'List scrollPadding and pageOverlap must fit in UInt16', json);
    }
    final items = (json['items'] as List)
        .map((i) => UIListItemSpec.fromJson(i as Map<String, dynamic>))
        .toList();
    final selectedID = json['selectedId'] as String?;
    if (selectedID != null && !items.any((item) => item.id == selectedID)) {
      throw FormatException(
          'List selectedId must identify one of its items', json);
    }
    return UIListSpec(
      id: json['id'] as String,
      items: items,
      emptyMessage: json['emptyMessage'] as String? ?? '',
      selectedID: selectedID,
      select: json['select'] as String?,
      scrollPadding: scrollPadding,
      pageOverlap: pageOverlap,
      pageBehavior: json['pageBehavior'] == null
          ? UIListPageBehavior.selection
          : UIListPageBehavior.fromJson(json['pageBehavior'] as String),
      spacePagesDown: json['spacePagesDown'] as bool? ?? false,
      rowLayout: json['rowLayout'] == null
          ? const UIListRowLayoutInline()
          : UIListRowLayout.fromJson(
              json['rowLayout'] as Map<String, dynamic>),
      contextMenu: json['contextMenu'] == null
          ? null
          : UIMenuSpec.fromJson(json['contextMenu'] as Map<String, dynamic>),
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'items': items.map((i) => i.toJson()).toList(),
        'emptyMessage': emptyMessage,
        if (selectedID != null) 'selectedId': selectedID,
        if (select != null) 'select': select,
        'scrollPadding': scrollPadding,
        'pageOverlap': pageOverlap,
        'pageBehavior': pageBehavior.toJson(),
        'spacePagesDown': spacePagesDown,
        'rowLayout': rowLayout.toJson(),
        if (contextMenu != null) 'contextMenu': contextMenu!.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIListSpec &&
      other.id == id &&
      _listEq(other.items, items) &&
      other.emptyMessage == emptyMessage &&
      other.selectedID == selectedID &&
      other.select == select &&
      other.scrollPadding == scrollPadding &&
      other.pageOverlap == pageOverlap &&
      other.pageBehavior == pageBehavior &&
      other.spacePagesDown == spacePagesDown &&
      other.rowLayout == rowLayout &&
      other.contextMenu == contextMenu;

  @override
  int get hashCode => Object.hash(id, items.length, scrollPadding);
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIContentFont`.
enum UIContentFont {
  body,
  monospace;

  static UIContentFont fromJson(String v) =>
      UIContentFont.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIContentTone` (`.default` encodes as `"default"`).
enum UIContentTone {
  standard('default'),
  muted('muted'),
  accent('accent'),
  info('info'),
  success('success'),
  warning('warning'),
  danger('danger');

  const UIContentTone(this.wire);
  final String wire;

  static UIContentTone fromJson(String v) => UIContentTone.values
      .firstWhere((e) => e.wire == v,
          orElse: () => throw FormatException('Unknown UIContentTone $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIContentEmphasis`.
enum UIContentEmphasis {
  regular,
  strong,
  italic;

  static UIContentEmphasis fromJson(String v) =>
      UIContentEmphasis.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UIContentLineTone` (`.default` encodes as `"default"`).
enum UIContentLineTone {
  standard('default'),
  muted('muted'),
  header('header'),
  added('added'),
  removed('removed');

  const UIContentLineTone(this.wire);
  final String wire;

  static UIContentLineTone fromJson(String v) =>
      UIContentLineTone.values.firstWhere((e) => e.wire == v,
          orElse: () =>
              throw FormatException('Unknown UIContentLineTone $v'));
  String toJson() => wire;
}

/// Mirrors Swift `UIContentRun`.
final class UIContentRun {
  const UIContentRun({
    required this.text,
    this.tone = UIContentTone.standard,
    this.emphasis = UIContentEmphasis.regular,
  });

  final String text;
  final UIContentTone tone;
  final UIContentEmphasis emphasis;

  factory UIContentRun.fromJson(Map<String, dynamic> json) => UIContentRun(
        text: json['text'] as String,
        tone: json['tone'] == null
            ? UIContentTone.standard
            : UIContentTone.fromJson(json['tone'] as String),
        emphasis: json['emphasis'] == null
            ? UIContentEmphasis.regular
            : UIContentEmphasis.fromJson(json['emphasis'] as String),
      );

  Map<String, dynamic> toJson() => {
        'text': text,
        'tone': tone.toJson(),
        'emphasis': emphasis.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIContentRun &&
      other.text == text &&
      other.tone == tone &&
      other.emphasis == emphasis;

  @override
  int get hashCode => Object.hash(text, tone, emphasis);
}

/// Mirrors Swift `UIContentLine`.
final class UIContentLine {
  const UIContentLine({
    required this.id,
    this.runs = const [],
    this.tone = UIContentLineTone.standard,
  });

  final String id;
  final List<UIContentRun> runs;
  final UIContentLineTone tone;

  factory UIContentLine.fromJson(Map<String, dynamic> json) => UIContentLine(
        id: json['id'] as String,
        runs: ((json['runs'] as List?) ?? [])
            .map((r) => UIContentRun.fromJson(r as Map<String, dynamic>))
            .toList(),
        tone: json['tone'] == null
            ? UIContentLineTone.standard
            : UIContentLineTone.fromJson(json['tone'] as String),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        if (runs.isNotEmpty) 'runs': runs.map((r) => r.toJson()).toList(),
        if (tone != UIContentLineTone.standard) 'tone': tone.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIContentLine &&
      other.id == id &&
      _listEq(other.runs, runs) &&
      other.tone == tone;

  @override
  int get hashCode => Object.hash(id, runs.length, tone);
}

/// Mirrors Swift `UIContentSelection`.
final class UIContentSelection {
  const UIContentSelection({required this.anchorID, required this.headID});

  final String anchorID;
  final String headID;

  factory UIContentSelection.fromJson(Map<String, dynamic> json) =>
      UIContentSelection(
        anchorID: json['anchorId'] as String,
        headID: json['headId'] as String,
      );

  Map<String, dynamic> toJson() =>
      {'anchorId': anchorID, 'headId': headID};

  @override
  bool operator ==(Object other) =>
      other is UIContentSelection &&
      other.anchorID == anchorID &&
      other.headID == headID;

  @override
  int get hashCode => Object.hash(anchorID, headID);
}

/// Mirrors Swift `UIContentSpec`, including the line-id/selection/select
/// invariants enforced in `init(from:)`.
final class UIContentSpec {
  const UIContentSpec({
    required this.id,
    required this.label,
    this.lines = const [],
    this.wrap = true,
    this.font = UIContentFont.body,
    this.emptyMessage = '',
    this.selection,
    this.select,
    this.contextMenu,
  });

  final String id;
  final String label;
  final List<UIContentLine> lines;
  final bool wrap;
  final UIContentFont font;
  final String emptyMessage;
  final UIContentSelection? selection;
  final String? select;
  final UIMenuSpec? contextMenu;

  factory UIContentSpec.fromJson(Map<String, dynamic> json) {
    final lines = ((json['lines'] as List?) ?? [])
        .map((l) => UIContentLine.fromJson(l as Map<String, dynamic>))
        .toList();
    final selection = json['selection'] == null
        ? null
        : UIContentSelection.fromJson(
            json['selection'] as Map<String, dynamic>);
    final select = json['select'] as String?;
    final ids = lines.map((l) => l.id).toSet();
    final selectionValid = selection == null ||
        (ids.contains(selection.anchorID) && ids.contains(selection.headID));
    if (ids.length != lines.length ||
        !selectionValid ||
        (selection != null && select == null)) {
      throw FormatException(
          'Content line ids and selection must be valid', json);
    }
    return UIContentSpec(
      id: json['id'] as String,
      label: json['label'] as String,
      lines: lines,
      wrap: json['wrap'] as bool? ?? true,
      font: json['font'] == null
          ? UIContentFont.body
          : UIContentFont.fromJson(json['font'] as String),
      emptyMessage: json['emptyMessage'] as String? ?? '',
      selection: selection,
      select: select,
      contextMenu: json['contextMenu'] == null
          ? null
          : UIMenuSpec.fromJson(json['contextMenu'] as Map<String, dynamic>),
    );
  }

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        if (lines.isNotEmpty) 'lines': lines.map((l) => l.toJson()).toList(),
        'wrap': wrap,
        'font': font.toJson(),
        'emptyMessage': emptyMessage,
        if (selection != null) 'selection': selection!.toJson(),
        if (select != null) 'select': select,
        if (contextMenu != null) 'contextMenu': contextMenu!.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIContentSpec &&
      other.id == id &&
      other.label == label &&
      _listEq(other.lines, lines) &&
      other.wrap == wrap &&
      other.font == font &&
      other.emptyMessage == emptyMessage &&
      other.selection == selection &&
      other.select == select &&
      other.contextMenu == contextMenu;

  @override
  int get hashCode => Object.hash(id, label, lines.length);
}

/// Mirrors Swift `UIInputSpec`.
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
  int get hashCode => Object.hash(id, label, value);
}

// ---------------------------------------------------------------------------
// Page slots, tabs, toolbar, page
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIPageHeaderSlot` with its custom type-discriminated Codable.
sealed class UIPageHeaderSlot {
  const UIPageHeaderSlot();

  String get type;

  factory UIPageHeaderSlot.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'input':
        return UIPageHeaderSlotInput(UIInputSpec.fromJson(json));
      default:
        return UIPageHeaderSlotUnsupported(json['type'] as String);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIPageHeaderSlotInput extends UIPageHeaderSlot {
  const UIPageHeaderSlotInput(this.input);
  final UIInputSpec input;
  @override
  String get type => 'input';
  @override
  Map<String, dynamic> toJson() => {'type': type, ...input.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIPageHeaderSlotInput && other.input == input;
  @override
  int get hashCode => input.hashCode;
}

final class UIPageHeaderSlotUnsupported extends UIPageHeaderSlot {
  const UIPageHeaderSlotUnsupported(this.type);
  @override
  final String type;
  @override
  Map<String, dynamic> toJson() => {'type': type};
  @override
  bool operator ==(Object other) =>
      other is UIPageHeaderSlotUnsupported && other.type == type;
  @override
  int get hashCode => type.hashCode;
}

/// Mirrors Swift `UIPageBodySlot` with its custom type-discriminated Codable.
sealed class UIPageBodySlot {
  const UIPageBodySlot();

  String get type;

  factory UIPageBodySlot.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'list':
        return UIPageBodySlotList(UIListSpec.fromJson(json));
      case 'content':
        return UIPageBodySlotContent(UIContentSpec.fromJson(json));
      case 'sparkline':
        return UIPageBodySlotSparkline(UISparklineSpec.fromJson(json));
      case 'barChart':
        return UIPageBodySlotBarChart(UIBarChartSpec.fromJson(json));
      case 'lineChart':
        return UIPageBodySlotLineChart(UILineChartSpec.fromJson(json));
      case 'gauge':
        return UIPageBodySlotGauge(UIGaugeSpec.fromJson(json));
      default:
        return UIPageBodySlotUnsupported(json['type'] as String);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIPageBodySlotList extends UIPageBodySlot {
  const UIPageBodySlotList(this.list);
  final UIListSpec list;
  @override
  String get type => 'list';
  @override
  Map<String, dynamic> toJson() => {'type': type, ...list.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIPageBodySlotList && other.list == list;
  @override
  int get hashCode => list.hashCode;
}

final class UIPageBodySlotContent extends UIPageBodySlot {
  const UIPageBodySlotContent(this.content);
  final UIContentSpec content;
  @override
  String get type => 'content';
  @override
  Map<String, dynamic> toJson() => {'type': type, ...content.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIPageBodySlotContent && other.content == content;
  @override
  int get hashCode => content.hashCode;
}

final class UIPageBodySlotSparkline extends UIPageBodySlot {
  const UIPageBodySlotSparkline(this.sparkline);
  final UISparklineSpec sparkline;
  @override
  String get type => 'sparkline';
  @override
  Map<String, dynamic> toJson() => {'type': type, ...sparkline.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIPageBodySlotSparkline && other.sparkline == sparkline;
  @override
  int get hashCode => sparkline.hashCode;
}

final class UIPageBodySlotBarChart extends UIPageBodySlot {
  const UIPageBodySlotBarChart(this.chart);
  final UIBarChartSpec chart;
  @override
  String get type => 'barChart';
  @override
  Map<String, dynamic> toJson() => {'type': type, ...chart.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIPageBodySlotBarChart && other.chart == chart;
  @override
  int get hashCode => chart.hashCode;
}

final class UIPageBodySlotLineChart extends UIPageBodySlot {
  const UIPageBodySlotLineChart(this.chart);
  final UILineChartSpec chart;
  @override
  String get type => 'lineChart';
  @override
  Map<String, dynamic> toJson() => {'type': type, ...chart.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIPageBodySlotLineChart && other.chart == chart;
  @override
  int get hashCode => chart.hashCode;
}

final class UIPageBodySlotGauge extends UIPageBodySlot {
  const UIPageBodySlotGauge(this.gauge);
  final UIGaugeSpec gauge;
  @override
  String get type => 'gauge';
  @override
  Map<String, dynamic> toJson() => {'type': type, ...gauge.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIPageBodySlotGauge && other.gauge == gauge;
  @override
  int get hashCode => gauge.hashCode;
}

final class UIPageBodySlotUnsupported extends UIPageBodySlot {
  const UIPageBodySlotUnsupported(this.type);
  @override
  final String type;
  @override
  Map<String, dynamic> toJson() => {'type': type};
  @override
  bool operator ==(Object other) =>
      other is UIPageBodySlotUnsupported && other.type == type;
  @override
  int get hashCode => type.hashCode;
}

/// Mirrors Swift `UIPageTab`.
final class UIPageTab {
  const UIPageTab({
    required this.id,
    required this.label,
    required this.action,
    this.selected = false,
  });

  final String id;
  final String label;
  final String action;
  final bool selected;

  factory UIPageTab.fromJson(Map<String, dynamic> json) => UIPageTab(
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
      other is UIPageTab &&
      other.id == id &&
      other.label == label &&
      other.action == action &&
      other.selected == selected;

  @override
  int get hashCode => Object.hash(id, label, action, selected);
}

/// Mirrors Swift `UIPageToolbar`, including `isValid`.
final class UIPageToolbar {
  const UIPageToolbar({required this.primary, this.menu});

  final UIFooterActionSpec primary;
  final UIMenuSpec? menu;

  bool get isValid =>
      UIFooterActionsSpec(actions: [primary]).isValid &&
      (menu == null || menu!.requiredCapabilities != null) &&
      !(menu?.items.any((item) => item.id == primary.id) ?? false);

  factory UIPageToolbar.fromJson(Map<String, dynamic> json) =>
      UIPageToolbar(
        primary: UIFooterActionSpec.fromJson(
            json['primary'] as Map<String, dynamic>),
        menu: json['menu'] == null
            ? null
            : UIMenuSpec.fromJson(json['menu'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'primary': primary.toJson(),
        if (menu != null) 'menu': menu!.toJson(),
      };

  @override
  bool operator ==(Object other) =>
      other is UIPageToolbar &&
      other.primary == primary &&
      other.menu == menu;

  @override
  int get hashCode => Object.hash(primary, menu);
}

/// Mirrors Swift `PageSpec`, including `requiredCapabilities`.
final class PageSpec {
  const PageSpec({
    required this.title,
    this.tabs = const [],
    this.toolbar,
    this.back,
    this.header,
    required this.body,
    this.footer = const UIFooterActionsSpec(),
  });

  final String title;
  final List<UIPageTab> tabs;
  final UIPageToolbar? toolbar;
  final String? back;
  final UIPageHeaderSlot? header;
  final UIPageBodySlot body;
  final UIFooterActionsSpec footer;

  factory PageSpec.fromJson(Map<String, dynamic> json) => PageSpec(
        title: json['title'] as String,
        tabs: ((json['tabs'] as List?) ?? [])
            .map((t) => UIPageTab.fromJson(t as Map<String, dynamic>))
            .toList(),
        toolbar: json['toolbar'] == null
            ? null
            : UIPageToolbar.fromJson(
                json['toolbar'] as Map<String, dynamic>),
        back: json['back'] as String?,
        header: json['header'] == null
            ? null
            : UIPageHeaderSlot.fromJson(
                json['header'] as Map<String, dynamic>),
        body:
            UIPageBodySlot.fromJson(json['body'] as Map<String, dynamic>),
        footer: json['footer'] == null
            ? const UIFooterActionsSpec()
            : UIFooterActionsSpec.fromJson(
                json['footer'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'title': title,
        if (tabs.isNotEmpty) 'tabs': tabs.map((t) => t.toJson()).toList(),
        if (toolbar != null) 'toolbar': toolbar!.toJson(),
        if (back != null) 'back': back,
        if (header != null) 'header': header!.toJson(),
        'body': body.toJson(),
        'footer': footer.toJson(),
      };

  /// Mirrors Swift `PageSpec.requiredCapabilities`.
  List<String>? get requiredCapabilities {
    if (!footer.isValid) return null;
    final capabilities = <String>[UIProtocol.pageCapability];
    final toolbar = this.toolbar;
    if (toolbar != null) {
      if (!toolbar.isValid) return null;
      capabilities.add(UIProtocol.pageToolbarCapability);
      if (toolbar.menu != null) {
        capabilities.addAll(
            [UIProtocol.menuCapability, UIProtocol.menuAnchorCapability]);
      }
    }
    if (tabs.isNotEmpty) {
      if (!(tabs.length <= 12 &&
          tabs.where((t) => t.selected).length == 1 &&
          tabs.map((t) => t.id).toSet().length == tabs.length &&
          tabs.every((t) => t.id.isNotEmpty && t.action.isNotEmpty))) {
        return null;
      }
      capabilities.add(UIProtocol.pageTabsCapability);
    }
    if (footer.status != null) {
      capabilities.add(UIProtocol.footerStatusCapability);
    }
    if (!footer.isEmpty) {
      capabilities.add(UIProtocol.footerActionsCapability);
    }
    final body = this.body;
    final String? chartCapability;
    if (body is UIPageBodySlotSparkline) {
      chartCapability = UIProtocol.sparklineCapability;
    } else if (body is UIPageBodySlotBarChart) {
      chartCapability = UIProtocol.barChartCapability;
    } else if (body is UIPageBodySlotLineChart) {
      chartCapability = UIProtocol.lineChartCapability;
    } else if (body is UIPageBodySlotGauge) {
      chartCapability = UIProtocol.gaugeCapability;
    } else {
      chartCapability = null;
    }
    if (chartCapability != null) {
      capabilities.add(chartCapability);
      final header = this.header;
      if (header != null) {
        if (header is! UIPageHeaderSlotInput) return null;
        capabilities.add(UIProtocol.inputCapability);
      }
      if (back != null) capabilities.add(UIProtocol.pageBackCapability);
      return capabilities;
    }
    if (body is UIPageBodySlotContent) {
      final content = (body as UIPageBodySlotContent).content;
      capabilities.add(UIProtocol.contentCapability);
      final header = this.header;
      if (header != null) {
        if (header is! UIPageHeaderSlotInput) return null;
        capabilities.add(UIProtocol.inputCapability);
      }
      if (back != null) capabilities.add(UIProtocol.pageBackCapability);
      if (content.selection != null || content.select != null) {
        capabilities.add(UIProtocol.contentSelectionCapability);
      }
      if (content.contextMenu != null) {
        capabilities.addAll(
            [UIProtocol.menuCapability, UIProtocol.menuAnchorCapability]);
      }
      return capabilities;
    }
    if (body is! UIPageBodySlotList) return null;
    final list = (body as UIPageBodySlotList).list;
    capabilities.addAll(
        [UIProtocol.listCapability, UIProtocol.listItemCapability]);
    final header = this.header;
    if (header != null) {
      if (header is! UIPageHeaderSlotInput) return null;
      capabilities.add(UIProtocol.inputCapability);
    }
    if (back != null) capabilities.add(UIProtocol.pageBackCapability);
    if (list.items.any((i) => i.detail != null || i.value != null)) {
      capabilities.add(UIProtocol.listItemMetadataCapability);
    }
    if (list.items.any((i) => i.activate != null)) {
      capabilities.add(UIProtocol.listItemActivateCapability);
    }
    if (list.items
        .any((i) => i.primaryRole != UIListItemPrimaryRole.staticRow)) {
      capabilities.add(UIProtocol.listItemRoleCapability);
    }
    var hasToggle = false;
    var hasStatus = false;
    var hasBadge = false;
    var hasSparkline = false;
    var hasGauge = false;
    for (final item in list.items) {
      for (final slot in [item.leading, item.trailing, item.accessory]
          .whereType<UIListItemSlot>()) {
        if (slot is UIListItemSlotToggle) {
          hasToggle = true;
        } else if (slot is UIListItemSlotStatus) {
          hasStatus = true;
        } else if (slot is UIListItemSlotBadge) {
          hasBadge = true;
        } else if (slot is UIListItemSlotSparkline) {
          hasSparkline = true;
        } else if (slot is UIListItemSlotGauge) {
          hasGauge = true;
        } else if (slot is UIListItemSlotUnsupported) {
          return null;
        }
      }
    }
    if (hasToggle) capabilities.add(UIProtocol.toggleCapability);
    if (list.items.any((i) =>
        i.busy ||
        i.labelTone != UIListItemTone.standard ||
        i.valueTone != UIListItemTone.muted ||
        i.emphasis != UIListItemEmphasis.regular ||
        i.valueMinWidth != null ||
        i.leading?.kind == 'status' ||
        i.leading?.kind == 'badge' ||
        i.trailing?.kind == 'status' ||
        i.trailing?.kind == 'badge' ||
        i.accessory?.kind == 'status' ||
        i.accessory?.kind == 'badge')) {
      capabilities.add(UIProtocol.listItemPresentationCapability);
    }
    if (list.items.any((i) =>
        i.labelRuns.isNotEmpty ||
        i.detailRuns.isNotEmpty ||
        i.valueRuns.isNotEmpty)) {
      capabilities.add(UIProtocol.listItemStyledTextCapability);
    }
    if (hasStatus) capabilities.add(UIProtocol.statusSymbolCapability);
    if (hasBadge) capabilities.add(UIProtocol.badgeCapability);
    if (hasSparkline) capabilities.add(UIProtocol.sparklineCapability);
    if (hasGauge) capabilities.add(UIProtocol.gaugeCapability);
    if (list.selectedID != null ||
        list.select != null ||
        list.scrollPadding != 0 ||
        list.pageOverlap != 1 ||
        list.pageBehavior != UIListPageBehavior.selection ||
        list.spacePagesDown) {
      capabilities.add(UIProtocol.listSelectionCapability);
    }
    if (list.contextMenu != null) {
      capabilities.addAll(
          [UIProtocol.menuCapability, UIProtocol.menuAnchorCapability]);
    }
    return capabilities;
  }

  @override
  bool operator ==(Object other) =>
      other is PageSpec &&
      other.title == title &&
      _listEq(other.tabs, tabs) &&
      other.toolbar == toolbar &&
      other.back == back &&
      other.header == header &&
      other.body == body &&
      other.footer == footer;

  @override
  int get hashCode => Object.hash(title, tabs.length, body);
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

/// Mirrors Swift `UITreePresentation`.
enum UITreePresentation {
  drillDown,
  outline;

  static UITreePresentation fromJson(String v) =>
      UITreePresentation.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UITreeItemKind`.
enum UITreeItemKind {
  parent,
  directory,
  file;

  static UITreeItemKind fromJson(String v) =>
      UITreeItemKind.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UITreeChildState`.
enum UITreeChildState {
  loaded,
  unloaded,
  loading;

  static UITreeChildState fromJson(String v) =>
      UITreeChildState.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `UITreeItem`.
final class UITreeItem {
  const UITreeItem({
    required this.id,
    required this.label,
    required this.kind,
    this.detail,
    this.hidden = false,
    this.symlink = false,
    this.childState = UITreeChildState.loaded,
    this.expanded = false,
    this.children = const [],
  });

  final String id;
  final String label;
  final UITreeItemKind kind;
  final String? detail;
  final bool hidden;
  final bool symlink;
  final UITreeChildState childState;
  final bool expanded;
  final List<UITreeItem> children;

  factory UITreeItem.fromJson(Map<String, dynamic> json) => UITreeItem(
        id: json['id'] as String,
        label: json['label'] as String,
        kind: UITreeItemKind.fromJson(json['kind'] as String),
        detail: json['detail'] as String?,
        hidden: json['hidden'] as bool? ?? false,
        symlink: json['symlink'] as bool? ?? false,
        childState: json['childState'] == null
            ? UITreeChildState.loaded
            : UITreeChildState.fromJson(json['childState'] as String),
        expanded: json['expanded'] as bool? ?? false,
        children: ((json['children'] as List?) ?? [])
            .map((c) => UITreeItem.fromJson(c as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'kind': kind.toJson(),
        if (detail != null) 'detail': detail,
        'hidden': hidden,
        'symlink': symlink,
        'childState': childState.toJson(),
        'expanded': expanded,
        if (children.isNotEmpty)
          'children': children.map((c) => c.toJson()).toList(),
      };

  @override
  bool operator ==(Object other) =>
      other is UITreeItem &&
      other.id == id &&
      other.label == label &&
      other.kind == kind &&
      other.detail == detail &&
      other.hidden == hidden &&
      other.symlink == symlink &&
      other.childState == childState &&
      other.expanded == expanded &&
      _listEq(other.children, children);

  @override
  int get hashCode => Object.hash(id, label, kind);
}

/// Mirrors Swift `UITreeFilter`.
final class UITreeFilter {
  const UITreeFilter({
    required this.id,
    required this.label,
    this.value = '',
    this.placeholder = '',
    required this.setValue,
  });

  final String id;
  final String label;
  final String value;
  final String placeholder;
  final String setValue;

  factory UITreeFilter.fromJson(Map<String, dynamic> json) => UITreeFilter(
        id: json['id'] as String,
        label: json['label'] as String,
        value: json['value'] as String? ?? '',
        placeholder: json['placeholder'] as String? ?? '',
        setValue: json['setValue'] as String,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'label': label,
        'value': value,
        'placeholder': placeholder,
        'setValue': setValue,
      };

  @override
  bool operator ==(Object other) =>
      other is UITreeFilter &&
      other.id == id &&
      other.label == label &&
      other.value == value &&
      other.placeholder == placeholder &&
      other.setValue == setValue;

  @override
  int get hashCode => Object.hash(id, label, setValue);
}

/// Mirrors Swift `UITreeActions`.
final class UITreeActions {
  const UITreeActions({
    this.select = 'tree-select',
    this.open = 'tree-open',
    this.parent = 'tree-parent',
    this.setExpanded,
  });

  final String select;
  final String open;
  final String parent;
  final String? setExpanded;

  factory UITreeActions.fromJson(Map<String, dynamic> json) =>
      UITreeActions(
        select: json['select'] as String? ?? 'tree-select',
        open: json['open'] as String? ?? 'tree-open',
        parent: json['parent'] as String? ?? 'tree-parent',
        setExpanded: json['setExpanded'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'select': select,
        'open': open,
        'parent': parent,
        if (setExpanded != null) 'setExpanded': setExpanded,
      };

  @override
  bool operator ==(Object other) =>
      other is UITreeActions &&
      other.select == select &&
      other.open == open &&
      other.parent == parent &&
      other.setExpanded == setExpanded;

  @override
  int get hashCode => Object.hash(select, open, parent, setExpanded);
}

/// Accumulator for [validateTreeItems], mirroring Swift's inout parameters.
final class _TreeValidationState {
  final ids = <String>{};
  var count = 0;
  var parentCount = 0;
}

/// Mirrors Swift's private `validateTreeItems(_:depth:ids:count:parentCount:)`.
bool validateTreeItems(
    List<UITreeItem> items, int depth, _TreeValidationState state) {
  if (depth > 32) return false;
  for (final item in items) {
    state.count += 1;
    if (state.count > 100000 ||
        !state.ids.add(item.id) ||
        item.label.contains('\n') ||
        item.label.contains('\r')) {
      return false;
    }
    switch (item.kind) {
      case UITreeItemKind.parent:
        state.parentCount += 1;
        if (!(depth == 0 && item.children.isEmpty && !item.expanded)) {
          return false;
        }
      case UITreeItemKind.file:
        if (!(item.children.isEmpty && !item.expanded)) return false;
      case UITreeItemKind.directory:
        if (!(item.childState == UITreeChildState.loaded ||
                item.children.isEmpty) ||
            !validateTreeItems(item.children, depth + 1, state)) {
          return false;
        }
    }
  }
  return true;
}

/// Mirrors Swift `UITreeSpec`, including `requiredCapabilities`.
final class UITreeSpec {
  const UITreeSpec({
    required this.label,
    required this.location,
    this.presentation = UITreePresentation.drillDown,
    this.filter,
    this.items = const [],
    this.selectedID,
    this.emptyMessage,
    this.primaryAction,
    this.contextMenu,
    this.actions = const UITreeActions(),
    this.footer = const UIFooterActionsSpec(),
  });

  final String label;
  final String location;
  final UITreePresentation presentation;
  final UITreeFilter? filter;
  final List<UITreeItem> items;
  final String? selectedID;
  final String? emptyMessage;
  final UIButtonSpec? primaryAction;
  final UIMenuSpec? contextMenu;
  final UITreeActions actions;
  final UIFooterActionsSpec footer;

  factory UITreeSpec.fromJson(Map<String, dynamic> json) => UITreeSpec(
        label: json['label'] as String,
        location: json['location'] as String,
        presentation: json['presentation'] == null
            ? UITreePresentation.drillDown
            : UITreePresentation.fromJson(json['presentation'] as String),
        filter: json['filter'] == null
            ? null
            : UITreeFilter.fromJson(json['filter'] as Map<String, dynamic>),
        items: ((json['items'] as List?) ?? [])
            .map((i) => UITreeItem.fromJson(i as Map<String, dynamic>))
            .toList(),
        selectedID: json['selectedId'] as String?,
        emptyMessage: json['emptyMessage'] as String?,
        primaryAction: json['primaryAction'] == null
            ? null
            : UIButtonSpec.fromJson(
                json['primaryAction'] as Map<String, dynamic>),
        contextMenu: json['contextMenu'] == null
            ? null
            : UIMenuSpec.fromJson(json['contextMenu'] as Map<String, dynamic>),
        actions: json['actions'] == null
            ? const UITreeActions()
            : UITreeActions.fromJson(json['actions'] as Map<String, dynamic>),
        footer: json['footer'] == null
            ? const UIFooterActionsSpec()
            : UIFooterActionsSpec.fromJson(
                json['footer'] as Map<String, dynamic>),
      );

  Map<String, dynamic> toJson() => {
        'label': label,
        'location': location,
        'presentation': presentation.toJson(),
        if (filter != null) 'filter': filter!.toJson(),
        if (items.isNotEmpty) 'items': items.map((i) => i.toJson()).toList(),
        if (selectedID != null) 'selectedId': selectedID,
        if (emptyMessage != null) 'emptyMessage': emptyMessage,
        if (primaryAction != null) 'primaryAction': primaryAction!.toJson(),
        if (contextMenu != null) 'contextMenu': contextMenu!.toJson(),
        'actions': actions.toJson(),
        'footer': footer.toJson(),
      };

  /// Mirrors Swift `UITreeSpec.requiredCapabilities`.
  List<String>? get requiredCapabilities {
    final state = _TreeValidationState();
    if (!validateTreeItems(items, 0, state) ||
        state.parentCount > 1 ||
        (selectedID != null && !state.ids.contains(selectedID!)) ||
        (presentation == UITreePresentation.outline &&
            actions.setExpanded == null) ||
        !footer.isValid ||
        footer.actions.any((a) => state.ids.contains(a.id))) {
      return null;
    }
    final capabilities = <String>[UIProtocol.treeCapability];
    if (presentation == UITreePresentation.outline ||
        items.any((i) => i.children.isNotEmpty)) {
      capabilities.add(UIProtocol.treeHierarchyCapability);
    }
    if (filter != null) capabilities.add(UIProtocol.treeFilterCapability);
    if (state.parentCount > 0) {
      capabilities.add(UIProtocol.treeParentCapability);
    }
    if (primaryAction != null) {
      capabilities.add(UIProtocol.buttonCapability);
    }
    if (contextMenu != null) {
      capabilities.addAll(
          [UIProtocol.menuCapability, UIProtocol.menuAnchorCapability]);
    }
    if (footer.status != null) {
      capabilities.add(UIProtocol.footerStatusCapability);
    }
    if (!footer.isEmpty) {
      capabilities.add(UIProtocol.footerActionsCapability);
    }
    return capabilities;
  }

  @override
  bool operator ==(Object other) =>
      other is UITreeSpec &&
      other.label == label &&
      other.location == location &&
      other.presentation == presentation &&
      other.filter == filter &&
      _listEq(other.items, items) &&
      other.selectedID == selectedID &&
      other.emptyMessage == emptyMessage &&
      other.primaryAction == primaryAction &&
      other.contextMenu == contextMenu &&
      other.actions == actions &&
      other.footer == footer;

  @override
  int get hashCode => Object.hash(label, location, presentation);
}

// ---------------------------------------------------------------------------
// Text box
// ---------------------------------------------------------------------------

/// Mirrors Swift `TextBoxTitlePosition`.
enum TextBoxTitlePosition {
  topLeft,
  topRight,
  bottomLeft,
  bottomRight;

  static TextBoxTitlePosition fromJson(String v) =>
      TextBoxTitlePosition.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `TextBoxSubmitMode`.
enum TextBoxSubmitMode {
  enter,
  never;

  static TextBoxSubmitMode fromJson(String v) =>
      TextBoxSubmitMode.values.byName(v);
  String toJson() => name;
}

/// Mirrors Swift `TextBoxTitle`.
final class TextBoxTitle {
  const TextBoxTitle({required this.text, required this.position});

  final String text;
  final TextBoxTitlePosition position;

  factory TextBoxTitle.fromJson(Map<String, dynamic> json) => TextBoxTitle(
        text: json['text'] as String,
        position:
            TextBoxTitlePosition.fromJson(json['position'] as String),
      );

  Map<String, dynamic> toJson() =>
      {'text': text, 'position': position.toJson()};

  @override
  bool operator ==(Object other) =>
      other is TextBoxTitle &&
      other.text == text &&
      other.position == position;

  @override
  int get hashCode => Object.hash(text, position);
}

/// Mirrors Swift `TextBoxKeyHint`.
final class TextBoxKeyHint {
  const TextBoxKeyHint({required this.key, required this.label});

  final String key;
  final String label;

  factory TextBoxKeyHint.fromJson(Map<String, dynamic> json) =>
      TextBoxKeyHint(
        key: json['key'] as String,
        label: json['label'] as String,
      );

  Map<String, dynamic> toJson() => {'key': key, 'label': label};

  @override
  bool operator ==(Object other) =>
      other is TextBoxKeyHint &&
      other.key == key &&
      other.label == label;

  @override
  int get hashCode => Object.hash(key, label);
}

/// Mirrors Swift `TextBoxBusy`, including its custom `encode(to:)` that
/// omits an empty `rightMeta`.
final class TextBoxBusy {
  const TextBoxBusy(
      {required this.label, this.elapsedMs = 0, this.rightMeta = ''});

  final String label;
  final int elapsedMs;
  final String rightMeta;

  factory TextBoxBusy.fromJson(Map<String, dynamic> json) => TextBoxBusy(
        label: json['label'] as String,
        elapsedMs: json['elapsedMs'] as int? ?? 0,
        rightMeta: json['rightMeta'] as String? ?? '',
      );

  Map<String, dynamic> toJson() => {
        'label': label,
        'elapsedMs': elapsedMs,
        if (rightMeta.isNotEmpty) 'rightMeta': rightMeta,
      };

  @override
  bool operator ==(Object other) =>
      other is TextBoxBusy &&
      other.label == label &&
      other.elapsedMs == elapsedMs &&
      other.rightMeta == rightMeta;

  @override
  int get hashCode => Object.hash(label, elapsedMs, rightMeta);
}

/// Mirrors Swift `TextBoxActions`.
final class TextBoxActions {
  const TextBoxActions({this.setText, this.submit});

  final String? setText;
  final String? submit;

  factory TextBoxActions.fromJson(Map<String, dynamic> json) =>
      TextBoxActions(
        setText: json['setText'] as String?,
        submit: json['submit'] as String?,
      );

  Map<String, dynamic> toJson() => {
        if (setText != null) 'setText': setText,
        if (submit != null) 'submit': submit,
      };

  @override
  bool operator ==(Object other) =>
      other is TextBoxActions &&
      other.setText == setText &&
      other.submit == submit;

  @override
  int get hashCode => Object.hash(setText, submit);
}

/// Mirrors Swift `TextBoxSpec`, including its custom `encode(to:)` and the
/// `minRows >= 1 && maxRows >= minRows` decode guard.
final class TextBoxSpec {
  const TextBoxSpec({
    this.text = '',
    this.placeholder = '',
    this.prompt = '',
    this.titles = const [],
    this.hints = const [],
    this.busy,
    this.submitMode = TextBoxSubmitMode.enter,
    this.minRows = 3,
    this.maxRows = 10,
    this.actions = const TextBoxActions(),
  });

  final String text;
  final String placeholder;
  final String prompt;
  final List<TextBoxTitle> titles;
  final List<TextBoxKeyHint> hints;
  final TextBoxBusy? busy;
  final TextBoxSubmitMode submitMode;
  final int minRows;
  final int maxRows;
  final TextBoxActions actions;

  factory TextBoxSpec.fromJson(Map<String, dynamic> json) {
    final minRows = json['minRows'] as int? ?? 3;
    final maxRows = json['maxRows'] as int? ?? 10;
    if (!(minRows >= 1 && maxRows >= minRows)) {
      throw FormatException(
          'TextBox minRows must be at least 1 and at most maxRows', json);
    }
    return TextBoxSpec(
      text: json['text'] as String? ?? '',
      placeholder: json['placeholder'] as String? ?? '',
      prompt: json['prompt'] as String? ?? '',
      titles: ((json['titles'] as List?) ?? [])
          .map((t) => TextBoxTitle.fromJson(t as Map<String, dynamic>))
          .toList(),
      hints: ((json['hints'] as List?) ?? [])
          .map((h) => TextBoxKeyHint.fromJson(h as Map<String, dynamic>))
          .toList(),
      busy: json['busy'] == null
          ? null
          : TextBoxBusy.fromJson(json['busy'] as Map<String, dynamic>),
      submitMode: json['submitMode'] == null
          ? TextBoxSubmitMode.enter
          : TextBoxSubmitMode.fromJson(json['submitMode'] as String),
      minRows: minRows,
      maxRows: maxRows,
      actions: json['actions'] == null
          ? const TextBoxActions()
          : TextBoxActions.fromJson(json['actions'] as Map<String, dynamic>),
    );
  }

  Map<String, dynamic> toJson() => {
        'text': text,
        if (placeholder.isNotEmpty) 'placeholder': placeholder,
        if (prompt.isNotEmpty) 'prompt': prompt,
        if (titles.isNotEmpty)
          'titles': titles.map((t) => t.toJson()).toList(),
        if (hints.isNotEmpty) 'hints': hints.map((h) => h.toJson()).toList(),
        if (busy != null) 'busy': busy!.toJson(),
        'submitMode': submitMode.toJson(),
        'minRows': minRows,
        'maxRows': maxRows,
        'actions': actions.toJson(),
      };

  /// Mirrors Swift `TextBoxSpec.isValid`: titles occupy unique positions.
  bool get isValid =>
      titles.length == titles.map((t) => t.position).toSet().length;

  @override
  bool operator ==(Object other) =>
      other is TextBoxSpec &&
      other.text == text &&
      other.placeholder == placeholder &&
      other.prompt == prompt &&
      _listEq(other.titles, titles) &&
      _listEq(other.hints, hints) &&
      other.busy == busy &&
      other.submitMode == submitMode &&
      other.minRows == minRows &&
      other.maxRows == maxRows &&
      other.actions == actions;

  @override
  int get hashCode => Object.hash(text, submitMode, minRows, maxRows);
}

// ---------------------------------------------------------------------------
// UIComponent, UINode
// ---------------------------------------------------------------------------

/// Mirrors Swift `UIComponent` with its type-discriminated Codable.
sealed class UIComponent {
  const UIComponent();

  String get kind;

  /// Mirrors Swift `UIComponent.requiredCapability`.
  String? get requiredCapability => requiredCapabilities?.first;

  /// Mirrors Swift `UIComponent.requiredCapabilities`.
  List<String>? get requiredCapabilities {
    final self = this;
    if (self is UIComponentCanvasPage) {
      return self.page.requiredCapabilities;
    }
    if (self is UIComponentMarkdownEditor) {
      final editor = self.editor;
      if (!((editor.insertMenu?.requiredCapabilities != null) ||
              editor.insertMenu == null) ||
          !((editor.contextMenu?.requiredCapabilities != null) ||
              editor.contextMenu == null) ||
          !(editor.commandHint?.isValid ?? true) ||
          !(editor.commandHint == null ||
              editor.actions.openMenu != null) ||
          !editor.footer.isValid) {
        return null;
      }
      final capabilities = <String>[UIProtocol.markdownEditorCapability];
      if (editor.commandHint != null) {
        capabilities.add(UIProtocol.markdownCommandHintCapability);
      }
      if (editor.insertMenu != null || editor.contextMenu != null) {
        capabilities.addAll(
            [UIProtocol.menuCapability, UIProtocol.menuAnchorCapability]);
      }
      if (editor.footer.status != null) {
        capabilities.add(UIProtocol.footerStatusCapability);
      }
      if (!editor.footer.isEmpty) {
        capabilities.add(UIProtocol.footerActionsCapability);
      }
      return capabilities;
    }
    if (self is UIComponentMedia) {
      return [UIProtocol.mediaCapability];
    }
    if (self is UIComponentMenu) {
      return self.menu.requiredCapabilities;
    }
    if (self is UIComponentPage) {
      return self.page.requiredCapabilities;
    }
    if (self is UIComponentSurface) {
      return [UIProtocol.surfaceCapability];
    }
    if (self is UIComponentTextBox) {
      if (!self.textBox.isValid) return null;
      return [UIProtocol.textBoxCapability];
    }
    if (self is UIComponentTree) {
      return self.tree.requiredCapabilities;
    }
    return null;
  }

  factory UIComponent.fromJson(Map<String, dynamic> json) {
    switch (json['type'] as String) {
      case 'canvasPage':
        return UIComponentCanvasPage(CanvasPageSpec.fromJson(json));
      case 'markdownEditor':
        return UIComponentMarkdownEditor(MarkdownEditorSpec.fromJson(json));
      case 'media':
        return UIComponentMedia(MediaSpec.fromJson(json));
      case 'menu':
        return UIComponentMenu(UIMenuSpec.fromJson(json));
      case 'page':
        return UIComponentPage(PageSpec.fromJson(json));
      case 'surface':
        return UIComponentSurface(SurfaceSpec.fromJson(json));
      case 'textBox':
        return UIComponentTextBox(TextBoxSpec.fromJson(json));
      case 'tree':
        return UIComponentTree(UITreeSpec.fromJson(json));
      default:
        return UIComponentUnsupported(json['type'] as String);
    }
  }

  Map<String, dynamic> toJson();
}

final class UIComponentCanvasPage extends UIComponent {
  const UIComponentCanvasPage(this.page);
  final CanvasPageSpec page;
  @override
  String get kind => 'canvasPage';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...page.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentCanvasPage && other.page == page;
  @override
  int get hashCode => page.hashCode;
}

final class UIComponentMarkdownEditor extends UIComponent {
  const UIComponentMarkdownEditor(this.editor);
  final MarkdownEditorSpec editor;
  @override
  String get kind => 'markdownEditor';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...editor.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentMarkdownEditor && other.editor == editor;
  @override
  int get hashCode => editor.hashCode;
}

final class UIComponentMedia extends UIComponent {
  const UIComponentMedia(this.media);
  final MediaSpec media;
  @override
  String get kind => 'media';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...media.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentMedia && other.media == media;
  @override
  int get hashCode => media.hashCode;
}

final class UIComponentMenu extends UIComponent {
  const UIComponentMenu(this.menu);
  final UIMenuSpec menu;
  @override
  String get kind => 'menu';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...menu.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentMenu && other.menu == menu;
  @override
  int get hashCode => menu.hashCode;
}

final class UIComponentPage extends UIComponent {
  const UIComponentPage(this.page);
  final PageSpec page;
  @override
  String get kind => 'page';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...page.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentPage && other.page == page;
  @override
  int get hashCode => page.hashCode;
}

final class UIComponentSurface extends UIComponent {
  const UIComponentSurface(this.surface);
  final SurfaceSpec surface;
  @override
  String get kind => 'surface';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...surface.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentSurface && other.surface == surface;
  @override
  int get hashCode => surface.hashCode;
}

final class UIComponentTextBox extends UIComponent {
  const UIComponentTextBox(this.textBox);
  final TextBoxSpec textBox;
  @override
  String get kind => 'textBox';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...textBox.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentTextBox && other.textBox == textBox;
  @override
  int get hashCode => textBox.hashCode;
}

final class UIComponentTree extends UIComponent {
  const UIComponentTree(this.tree);
  final UITreeSpec tree;
  @override
  String get kind => 'tree';
  @override
  Map<String, dynamic> toJson() => {'type': kind, ...tree.toJson()};
  @override
  bool operator ==(Object other) =>
      other is UIComponentTree && other.tree == tree;
  @override
  int get hashCode => tree.hashCode;
}

final class UIComponentUnsupported extends UIComponent {
  const UIComponentUnsupported(this.kind);
  @override
  final String kind;
  @override
  Map<String, dynamic> toJson() => {'type': kind};
  @override
  bool operator ==(Object other) =>
      other is UIComponentUnsupported && other.kind == kind;
  @override
  int get hashCode => kind.hashCode;
}

/// Mirrors Swift `UINode`: an identified component, flat-encoded with its
/// component's fields alongside `id` and `type`.
final class UINode {
  const UINode({required this.id, required this.component});

  final String id;
  final UIComponent component;

  factory UINode.fromJson(Map<String, dynamic> json) => UINode(
        id: json['id'] as String,
        component: UIComponent.fromJson(json),
      );

  Map<String, dynamic> toJson() =>
      {'id': id, ...component.toJson()};

  @override
  bool operator ==(Object other) =>
      other is UINode && other.id == id && other.component == component;

  @override
  int get hashCode => Object.hash(id, component);
}
