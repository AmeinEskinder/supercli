/// Usage app pane: per-provider quota gauges, token history, monthly tables.
///
/// Real implementation against the [StubUsageDataSource] model layer (the
/// Host has no usage backend route yet — see docs/gpuidart-gaps-apps.md
/// GAP-A2). Renders through the RLE fallback (UiRow/UiText/UiColumn) with
/// text gauge bars; no gpuidart APIs beyond the upstream set are used.
///
/// Covers parity row 143 (per-provider quota gauges, token history,
/// project/total monthly tables, alerts, themes).
library;

import 'package:gpuidart/gpuidart.dart';

import '../widgets/git_widgets.dart' show gaugeBar;

/// Usage record for one provider.
final class ProviderUsage {
  const ProviderUsage({
    required this.provider,
    required this.quotaUsed,
    required this.quotaLimit,
    required this.tokensToday,
    required this.tokensMonth,
    required this.alertThreshold,
  });

  final String provider;
  final double quotaUsed;
  final double quotaLimit;
  final int tokensToday;
  final int tokensMonth;

  /// Alert when usage exceeds this fraction of quota (e.g. 0.8).
  final double alertThreshold;

  double get fraction =>
      quotaLimit <= 0 ? 0 : (quotaUsed / quotaLimit).clamp(0.0, 1.0);

  bool get alerting => fraction >= alertThreshold;

  String get percentLabel => '${(fraction * 100).toStringAsFixed(0)}%';
}

/// One row of the monthly token table.
final class MonthlyUsageRow {
  const MonthlyUsageRow({
    required this.project,
    required this.tokens,
    required this.costUsd,
  });

  final String project;
  final int tokens;
  final double costUsd;
}

/// Stub usage data source.
///
/// STUB: the Host has no usage backend route yet, so the Usage pane cannot
/// show live provider quotas. Representative data stands in until the
/// backend exists (GAP-A2 in docs/gpuidart-gaps-apps.md).
final class StubUsageDataSource {
  const StubUsageDataSource();

  List<ProviderUsage> get providers => const [
    ProviderUsage(
      provider: 'claude',
      quotaUsed: 82,
      quotaLimit: 100,
      tokensToday: 145230,
      tokensMonth: 2890341,
      alertThreshold: 0.8,
    ),
    ProviderUsage(
      provider: 'codex',
      quotaUsed: 31,
      quotaLimit: 100,
      tokensToday: 48210,
      tokensMonth: 912455,
      alertThreshold: 0.8,
    ),
    ProviderUsage(
      provider: 'muse',
      quotaUsed: 12,
      quotaLimit: 100,
      tokensToday: 8930,
      tokensMonth: 210330,
      alertThreshold: 0.8,
    ),
    ProviderUsage(
      provider: 'grok',
      quotaUsed: 5,
      quotaLimit: 100,
      tokensToday: 2110,
      tokensMonth: 88412,
      alertThreshold: 0.8,
    ),
  ];

  List<MonthlyUsageRow> get monthlyTable => const [
    MonthlyUsageRow(project: 'supercli', tokens: 2890341, costUsd: 43.35),
    MonthlyUsageRow(project: 'harness', tokens: 912455, costUsd: 13.69),
    MonthlyUsageRow(project: 'scratch', tokens: 298742, costUsd: 4.48),
  ];

  /// Last 14 days of total tokens (for the history sparkline).
  List<int> get tokenHistory => const [
    120400,
    135020,
    98010,
    142300,
    160450,
    110230,
    95400,
    130120,
    148900,
    139440,
    125300,
    152010,
    138220,
    145230,
  ];
}

/// The Usage app pane: gauges, history sparkline, monthly table, alerts.
final class UsagePaneView {
  UsagePaneView({
    required this.paneId,
    this.dataSource = const StubUsageDataSource(),
    this.theme = UsageTheme.dark,
  });

  final String paneId;
  final StubUsageDataSource dataSource;
  final UsageTheme theme;

  UiNode build() {
    return UiColumn(
      'usagepane-$paneId',
      [
        _titleRow(),
        _alertsRow(),
        _gaugesSection(),
        _historySection(),
        _monthlyTable(),
      ],
      style: UiStyle(
        background: UiColor.hex('#1e1e1e'),
        padding: const [8, 8, 8, 8],
        gap: 10,
      ),
    );
  }

  UiNode _titleRow() {
    return UiText(
      'usagepane-$paneId-title',
      'Usage',
      style: UiStyle(
        foreground: UiColor.hex('#eeeeec'),
        fontSize: 16,
        fontWeight: UiFontWeight.bold,
      ),
    );
  }

  UiNode _alertsRow() {
    final alerting = dataSource.providers.where((p) => p.alerting).toList();
    if (alerting.isEmpty) {
      return UiText(
        'usagepane-$paneId-alerts-ok',
        '✓ all providers within quota',
        style: UiStyle(foreground: UiColor.hex('#8ae234'), fontSize: 12),
      );
    }
    return UiColumn('usagepane-$paneId-alerts', [
      for (final p in alerting)
        UiText(
          'usagepane-$paneId-alert-${p.provider}',
          '⚠ ${p.provider}: ${p.percentLabel} of quota used',
          style: UiStyle(
            foreground: UiColor.hex('#ef2929'),
            fontSize: 12,
            fontWeight: UiFontWeight.bold,
          ),
        ),
    ], style: UiStyle(gap: 2));
  }

  UiNode _gaugesSection() {
    return UiColumn('usagepane-$paneId-gauges', [
      UiText(
        'usagepane-$paneId-gauges-label',
        'Provider quotas',
        style: UiStyle(
          foreground: UiColor.hex('#ad7fa8'),
          fontSize: 12,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      for (final p in dataSource.providers)
        UiRow('usagepane-$paneId-gauge-${p.provider}', [
          UiText(
            'usagepane-$paneId-gauge-${p.provider}-name',
            p.provider.padRight(8),
            style: UiStyle(foreground: UiColor.hex('#d3d7cf'), fontSize: 13),
          ),
          UiText(
            'usagepane-$paneId-gauge-${p.provider}-bar',
            gaugeBar(p.fraction, 20),
            style: UiStyle(
              foreground: UiColor.hex(p.alerting ? '#ef2929' : '#8ae234'),
              fontSize: 13,
            ),
          ),
          UiText(
            'usagepane-$paneId-gauge-${p.provider}-pct',
            p.percentLabel,
            style: UiStyle(
              foreground: UiColor.hex(p.alerting ? '#ef2929' : '#8a8a8a'),
              fontSize: 12,
              fontWeight: p.alerting ? UiFontWeight.bold : UiFontWeight.normal,
            ),
          ),
          UiText(
            'usagepane-$paneId-gauge-${p.provider}-tok',
            '${_fmtTokens(p.tokensToday)} today',
            style: UiStyle(foreground: UiColor.hex('#555753'), fontSize: 11),
          ),
        ], style: UiStyle(gap: 8)),
    ], style: UiStyle(gap: 4));
  }

  UiNode _historySection() {
    final history = dataSource.tokenHistory;
    final max = history.reduce((a, b) => a > b ? a : b);
    // Text sparkline: 8 block levels.
    const blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    final spark = history
        .map((v) => blocks[((v / max) * 7).round().clamp(0, 7)])
        .join();
    return UiColumn('usagepane-$paneId-history', [
      UiText(
        'usagepane-$paneId-history-label',
        'Token history (14 days)',
        style: UiStyle(
          foreground: UiColor.hex('#ad7fa8'),
          fontSize: 12,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      UiText(
        'usagepane-$paneId-history-spark',
        spark,
        style: UiStyle(foreground: UiColor.hex('#729fcf'), fontSize: 16),
      ),
      UiText(
        'usagepane-$paneId-history-range',
        '${_fmtTokens(history.first)} → ${_fmtTokens(history.last)} tokens/day',
        style: UiStyle(foreground: UiColor.hex('#555753'), fontSize: 11),
      ),
    ], style: UiStyle(gap: 2));
  }

  UiNode _monthlyTable() {
    final rows = dataSource.monthlyTable;
    final totalTokens = rows.fold<int>(0, (s, r) => s + r.tokens);
    final totalCost = rows.fold<double>(0, (s, r) => s + r.costUsd);
    return UiColumn('usagepane-$paneId-monthly', [
      UiText(
        'usagepane-$paneId-monthly-label',
        'Monthly by project',
        style: UiStyle(
          foreground: UiColor.hex('#ad7fa8'),
          fontSize: 12,
          fontWeight: UiFontWeight.bold,
        ),
      ),
      UiRow('usagepane-$paneId-monthly-head', [
        UiText(
          'usagepane-$paneId-monthly-hp',
          'project'.padRight(12),
          style: _headStyle(),
        ),
        UiText(
          'usagepane-$paneId-monthly-ht',
          'tokens'.padLeft(12),
          style: _headStyle(),
        ),
        UiText(
          'usagepane-$paneId-monthly-hc',
          'cost'.padLeft(10),
          style: _headStyle(),
        ),
      ], style: UiStyle(gap: 8)),
      for (final r in rows)
        UiRow('usagepane-$paneId-monthly-${r.project}', [
          UiText(
            'usagepane-$paneId-monthly-${r.project}-p',
            r.project.padRight(12),
            style: _cellStyle(),
          ),
          UiText(
            'usagepane-$paneId-monthly-${r.project}-t',
            _fmtTokens(r.tokens).padLeft(12),
            style: _cellStyle(),
          ),
          UiText(
            'usagepane-$paneId-monthly-${r.project}-c',
            '\$${r.costUsd.toStringAsFixed(2)}'.padLeft(10),
            style: _cellStyle(),
          ),
        ], style: UiStyle(gap: 8)),
      UiRow('usagepane-$paneId-monthly-total', [
        UiText(
          'usagepane-$paneId-monthly-tp',
          'total'.padRight(12),
          style: _totalStyle(),
        ),
        UiText(
          'usagepane-$paneId-monthly-tt',
          _fmtTokens(totalTokens).padLeft(12),
          style: _totalStyle(),
        ),
        UiText(
          'usagepane-$paneId-monthly-tc',
          '\$${totalCost.toStringAsFixed(2)}'.padLeft(10),
          style: _totalStyle(),
        ),
      ], style: UiStyle(gap: 8)),
    ], style: UiStyle(gap: 3));
  }

  UiStyle _headStyle() => UiStyle(
    foreground: UiColor.hex('#555753'),
    fontSize: 11,
    fontWeight: UiFontWeight.bold,
  );

  UiStyle _cellStyle() =>
      UiStyle(foreground: UiColor.hex('#d3d7cf'), fontSize: 12);

  UiStyle _totalStyle() => UiStyle(
    foreground: UiColor.hex('#eeeeec'),
    fontSize: 12,
    fontWeight: UiFontWeight.bold,
  );

  static String _fmtTokens(int n) {
    if (n >= 1000000) return '${(n / 1000000).toStringAsFixed(1)}M';
    if (n >= 1000) return '${(n / 1000).toStringAsFixed(1)}k';
    return '$n';
  }
}

/// Usage pane themes (parity row 143: themes).
enum UsageTheme { dark, light }
