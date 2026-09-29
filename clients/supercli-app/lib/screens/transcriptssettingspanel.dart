/// Local Transcripts settings panel.
///
/// Port of `TranscriptsSettingsPanel` (SettingsView.swift, 4060-4197).
///
/// Which content types the Markdown transcript includes and how much of
/// it. Shared by the session context menu's "Copy transcript" action
/// (desktop and phone) and the Sessions MCP `read_transcript` tool (as its
/// defaults), so all of them stay in sync.
///
/// Renders through the RLE fallback pattern (UiRow/UiText/UiButton/UiInput)
/// since gpuidart has no native settings widgets (see
/// docs/gpuidart-gaps-settings.md).
library;

import 'package:gpuidart/gpuidart.dart';

import 'settingsprimitives.dart';
import 'settingspanels.dart' as base;

/// Range options for the transcript range picker
/// (Swift: the `maxEntries` Picker in `transcriptSection`).
const List<int> transcriptRangeOptions = [0, 20, 50, 100];

/// Label for a transcript range option (Swift: picker tags).
String transcriptRangeLabel(int entries) {
  switch (entries) {
    case 0:
      return 'Whole conversation';
    case 20:
      return 'Last 20 entries';
    case 50:
      return 'Last 50 entries';
    case 100:
      return 'Last 100 entries';
    default:
      return 'Last $entries entries';
  }
}

/// Local Transcripts settings panel
/// (Swift: `TranscriptsSettingsPanel`).
final class TranscriptsSettingsPanel {
  const TranscriptsSettingsPanel({
    this.includeSessionInfo = true,
    this.includeUser = true,
    this.includeAssistant = true,
    this.includeReasoning = true,
    this.includeTools = true,
    this.includeFileChanges = true,
    this.includePlanUpdates = true,
    this.maxEntries = 50,
  });

  final bool includeSessionInfo;
  final bool includeUser;
  final bool includeAssistant;
  final bool includeReasoning;
  final bool includeTools;
  final bool includeFileChanges;
  final bool includePlanUpdates;
  final int maxEntries;

  UiNode _toggle({
    required String id,
    required String title,
    String subtitle = '',
    required bool value,
  }) {
    return UiColumn('$id-labeled', [
      base.SettingsToggle(id: id, label: title, value: value).fallback(),
      if (subtitle.isNotEmpty) UiText('$id-subtitle', subtitle),
    ]);
  }

  UiNode build() {
    return UiColumn('transcripts-settings', [
      const SettingsPaneHeader(
        title: 'Transcripts',
        description:
            'A session\'s conversation, rendered as Markdown — '
            'what "Copy transcript" copies and what agents read.',
      ).build(),
      UiColumn('transcripts-content', [
        const SettingsSectionHeader(
          title: 'Transcript content',
          description:
              'What "Copy transcript" (right-click a session) puts on '
              'the clipboard as Markdown. These options also drive the defaults '
              'for agents reading a session\'s transcript. Range is the default '
              'for agent reads; the Copy transcript menu picks its own range.',
        ).build(),
        _toggle(
          id: 'transcripts-session-info',
          title: 'Session info header',
          subtitle:
              'Start with the session\'s title, ID, CLI, and model. '
              'The ID lets another agent target this session with the '
              'Sessions MCP tools.',
          value: includeSessionInfo,
        ),
        _toggle(
          id: 'transcripts-user',
          title: 'User messages',
          value: includeUser,
        ),
        _toggle(
          id: 'transcripts-assistant',
          title: 'Assistant messages',
          value: includeAssistant,
        ),
        _toggle(
          id: 'transcripts-reasoning',
          title: 'Reasoning',
          subtitle: 'The agent\'s thinking blocks.',
          value: includeReasoning,
        ),
        _toggle(
          id: 'transcripts-tools',
          title: 'Tool calls & results',
          subtitle: 'Commands the agent ran and their output.',
          value: includeTools,
        ),
        _toggle(
          id: 'transcripts-file-changes',
          title: 'File changes & diffs',
          value: includeFileChanges,
        ),
        _toggle(
          id: 'transcripts-plan-updates',
          title: 'Plan updates',
          value: includePlanUpdates,
        ),
        UiColumn('transcripts-range-labeled', [
          base.SettingsSelect(
            id: 'transcripts-range',
            label: 'Range',
            selected: transcriptRangeLabel(maxEntries),
            options: [
              for (final o in transcriptRangeOptions) transcriptRangeLabel(o),
            ],
          ).fallback(),
          const UiText(
            'transcripts-range-subtitle',
            'How much of the conversation to include.',
          ),
        ]),
      ]),
    ]);
  }
}
