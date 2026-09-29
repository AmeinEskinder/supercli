/// Session sidebar: project/session tree with pins, groups, worktree folders,
/// attention dots and busy spinners.
///
/// Port of `SidebarView.swift` (SupercliNative/Views, 4406 lines). Covers:
/// session rows with presence avatars, workspace groups, project aggregate
/// rollups (attention/unread/busy shimmer), context menus, unread badges,
/// filter field, footer strip (+ add menu, settings, collapse-all state),
/// branch labels, quick-preset strip, new-session menu sections, resume
/// affordances, empty states, scroll targets/cues, motion constants, folder
/// drop highlight, list fade mask, hover marquee timing, and row action
/// buttons.
///
/// gpuidart rendering: there is no native list/tree node upstream, so rows
/// render through the UiRow/UiText fallback pattern (same as the terminal
/// RLE fallback in lib/terminal/terminal_pane.dart). Per-row tap, drag
/// reorder and popup context menus need framework APIs that do not exist
/// yet — see docs/gpuidart-gaps-sidebar.md. Until then, rows are static
/// and interactions travel as [UiAction]s scoped to node IDs.
///
/// Platform-only pieces of SidebarView.swift are NOT ported (AppKit/SwiftUI
/// drag-drop delegates, NSViewRepresentable hover monitors,
/// CoreText glyph metrics, NSImage icon pipelines): SidebarDragState,
/// SidebarReorderDropDelegate, SidebarContainerDropDelegate,
/// EmptyDragPreview, HoverReporter, RemoveConfirmDismissMonitor,
/// SelectedRowGlass, FolderColorMenuSwatch, SessionRowEnterModifier,
/// SessionListContentModifier, and the detached-drag controllers (covered
/// by lib/screens/projectsidebarview.dart's SidebarSessionDrag port).
library;

import 'package:gpuidart/gpuidart.dart';

import '../keymap.dart';
import '../models.dart';
import '../native_client.dart';

/// A session as the sidebar sees it: the host summary plus sidebar-local
/// state (pin, group, worktree, attention, busy).
final class SidebarSession {
  const SidebarSession({
    required this.summary,
    this.projectId = '',
    this.groupId,
    this.worktree,
    this.pinned = false,
    this.attention = false,
    this.busy = false,
    this.viewers = const [],
  });

  final SessionSummary summary;
  final String projectId;

  /// Group inside the project, or null for the ungrouped list.
  final String? groupId;

  /// Worktree folder this session belongs to, or null.
  final String? worktree;

  final bool pinned;
  final bool attention;
  final bool busy;
  final List<String> viewers;

  String get id => summary.id;
  String get title => summary.title;
  String get displayTitle => summary.displayTitle;
  int get unreadCount => summary.unreadCount;

  /// Build a sidebar session from a live Host [SessionSummary], deriving
  /// the sidebar-local status signals from the Host's `activity`/`pinned`
  /// fields (SidebarView.swift: session.status .attention/.starting/.busy).
  /// Sessions without a Host project land in the default "Sessions" project.
  factory SidebarSession.fromHostSummary(SessionSummary summary) {
    return SidebarSession(
      summary: summary,
      projectId: summary.projectId.isEmpty ? 'Sessions' : summary.projectId,
      pinned: summary.pinned,
      attention: summary.needsAttention,
      busy: summary.isBusy,
    );
  }
}

/// A named group of sessions inside a project.
final class SidebarGroup {
  const SidebarGroup({
    required this.id,
    required this.title,
    this.color,
    this.collapsed = false,
    this.sessions = const [],
  });

  final String id;
  final String title;

  /// Folder color as #RRGGBB, or null for default.
  final String? color;
  final bool collapsed;
  final List<SidebarSession> sessions;
}

/// A project: top-level sidebar node with groups, worktree folders and
/// ungrouped sessions.
final class SidebarProject {
  const SidebarProject({
    required this.id,
    required this.name,
    this.folderColor,
    this.collapsed = false,
    this.groups = const [],
    this.sessions = const [],
    this.worktrees = const [],
  });

  final String id;
  final String name;

  /// Folder color as #RRGGBB, or null for default.
  final String? folderColor;
  final bool collapsed;
  final List<SidebarGroup> groups;

  /// Sessions not in any group.
  final List<SidebarSession> sessions;

  /// Worktree folder names under this project.
  final List<String> worktrees;

  /// All sessions in this project (grouped + ungrouped).
  List<SidebarSession> get allSessions => [
    ...sessions,
    for (final g in groups) ...g.sessions,
  ];
}

/// One row of the sidebar. Rendered as a UiRow of UiText cells:
/// [attention][pin][title][spinner][unread][avatars].
final class SidebarRow {
  const SidebarRow._();

  /// Attention dot: red ● when the session needs attention.
  static String attentionGlyph(bool attention) => attention ? '●' : '';

  /// Busy spinner: ◌ while the session's agent is working.
  static String spinnerGlyph(bool busy) => busy ? '◌' : '';

  /// Pin glyph for pinned sessions.
  static String pinGlyph(bool pinned) => pinned ? '📌' : '';

  static final _red = UiColor.hex('#ef2929');
  static final _selectedBg = UiColor.hex('#2d4a6f');
  static final _selectedText = UiColor.hex('#ffffff');

  static UiRow sessionRow(SidebarSession session, {required bool selected}) {
    final id = session.id;
    return UiRow(
      'session-$id',
      [
        if (session.attention)
          UiText(
            'attn-$id',
            '●',
            style: UiStyle(foreground: _red, fontSize: 11),
          ),
        if (session.pinned)
          UiText('pin-$id', '📌', style: const UiStyle(fontSize: 11)),
        UiText(
          'title-$id',
          session.title,
          style: UiStyle(
            fontSize: 13,
            fontWeight: selected || session.unreadCount > 0
                ? UiFontWeight.semibold
                : UiFontWeight.normal,
            foreground: selected ? _selectedText : null,
          ),
        ),
        if (session.busy)
          UiText('spin-$id', ' ◌', style: const UiStyle(fontSize: 11)),
        if (session.unreadCount > 0)
          UiText(
            'unread-$id',
            ' ${session.unreadCount}',
            style: UiStyle(
              fontSize: 11,
              foreground: _red,
              fontWeight: UiFontWeight.bold,
            ),
          ),
        if (session.viewers.isNotEmpty)
          UiText(
            'viewers-$id',
            ' ${session.viewers.map((v) => v.isNotEmpty ? v[0] : '?').join()}',
            style: const UiStyle(fontSize: 11),
          ),
      ],
      style: UiStyle(
        background: selected ? _selectedBg : null,
        padding: const [3, 8, 3, 8],
        gap: 6,
      ),
    );
  }
}

/// The full session sidebar.
///
/// Layout (top to bottom): workspace dots, filter input, new-session
/// button, pinned section, per-project trees (header + worktree folders +
/// groups + sessions), archived footer.
final class SidebarView {
  SidebarView({
    this.workspaces = const [],
    this.activeWorkspaceId,
    this.projects = const [],
    this.pinned = const [],
    this.filterText = '',
    this.selectedSessionId,
    this.localVerbsVisible = true,
    this.addMenuOpen = false,
  });

  final List<String> workspaces;
  final String? activeWorkspaceId;
  final List<SidebarProject> projects;
  final List<SidebarSession> pinned;
  final String filterText;
  final String? selectedSessionId;

  /// False while a remote Host is selected: the session-tree local verbs
  /// (Add Project) disappear from the footer "+" menu.
  final bool localVerbsVisible;

  /// Whether the footer "+" add popover is open.
  final bool addMenuOpen;

  /// Sessions matching [filterText] (case-insensitive substring on title).
  bool matchesFilter(SidebarSession s) {
    if (filterText.isEmpty) return true;
    return s.title.toLowerCase().contains(filterText.toLowerCase());
  }

  UiNode build() {
    final children = <UiNode>[
      // Workspace quick-switch dots.
      UiRow('workspace-dots', [
        for (final w in workspaces)
          UiButton('wsdot-$w', w == activeWorkspaceId ? '● $w' : '○ $w'),
      ]),
      const UiInput('sidebar-filter', placeholder: 'Filter sessions…'),
      const UiButton('new-session', '+ New Session'),
    ];

    final visiblePinned = pinned.where(matchesFilter).toList();
    if (visiblePinned.isNotEmpty) {
      children.add(const UiText('section-pinned', 'Pinned'));
      for (final s in visiblePinned) {
        children.add(
          SidebarRow.sessionRow(s, selected: s.id == selectedSessionId),
        );
      }
    }

    for (final project in projects) {
      children.add(_projectNode(project));
    }

    children.add(const UiButton('show-archived', 'Archived Sessions'));
    // Footer strip (SidebarView.swift: SidebarFooter): the "+" add popover
    // (Add Project… hidden while a remote Host is scoped) + Settings.
    // Collapse-all lives in the menu bar (Session ▸ Collapse All Folders),
    // not the footer. The canonical Cmd-,/Ctrl-, chord cannot be registered
    // natively (gpuidart rejects punctuation keys — see
    // docs/gpuidart-gaps-keys.md), so Settings is reachable from this
    // visible menu button (handled via SupercliApp.handleClick) and from
    // the command palette ('Open settings').
    children.add(
      UiRow('sidebar-footer', [
        const UiButton('footer-add', '＋'),
        const UiButton('open-settings', '⚙ Settings'),
      ]),
    );
    if (addMenuOpen) {
      final menu = SidebarFooterAddMenu(localVerbsVisible: localVerbsVisible);
      children.add(
        UiColumn('footer-add-menu', [
          for (final (action, label) in menu.items)
            UiButton('footer-add-$action', label),
        ]),
      );
    }
    return UiColumn('sidebar', children);
  }

  UiNode _projectNode(SidebarProject project) {
    final children = <UiNode>[
      UiRow('project-${project.id}', [
        UiText(
          'project-color-${project.id}',
          '■', // folder color swatch
          style: UiStyle(
            fontSize: 12,
            foreground: project.folderColor != null
                ? UiColor.hex(project.folderColor!)
                : UiColor.hex('#8b8b8b'),
          ),
        ),
        UiText(
          'project-name-${project.id}',
          project.name,
          style: const UiStyle(fontSize: 13, fontWeight: UiFontWeight.semibold),
        ),
        UiText(
          'project-collapse-${project.id}',
          project.collapsed ? '▸' : '▾',
          style: const UiStyle(fontSize: 11),
        ),
      ]),
    ];
    if (!project.collapsed) {
      // Worktree folders.
      for (final wt in project.worktrees) {
        final wtSessions = project.allSessions
            .where((s) => s.worktree == wt && matchesFilter(s))
            .toList();
        children.add(
          UiText(
            'worktree-${project.id}-$wt',
            '  📁 $wt',
            style: const UiStyle(fontSize: 12),
          ),
        );
        for (final s in wtSessions) {
          children.add(
            SidebarRow.sessionRow(s, selected: s.id == selectedSessionId),
          );
        }
      }
      // Groups.
      for (final group in project.groups) {
        children.add(
          UiRow('group-${group.id}', [
            if (group.color != null)
              UiText(
                'group-color-${group.id}',
                '■',
                style: UiStyle(
                  fontSize: 12,
                  foreground: UiColor.hex(group.color!),
                ),
              ),
            UiText(
              'group-title-${group.id}',
              group.title,
              style: const UiStyle(fontSize: 12),
            ),
            UiText(
              'group-collapse-${group.id}',
              group.collapsed ? '▸' : '▾',
              style: const UiStyle(fontSize: 11),
            ),
          ]),
        );
        if (!group.collapsed) {
          for (final s in group.sessions.where(matchesFilter)) {
            children.add(
              SidebarRow.sessionRow(s, selected: s.id == selectedSessionId),
            );
          }
        }
      }
      // Ungrouped sessions (excluding those shown under worktree folders).
      for (final s in project.sessions.where(
        (s) => s.worktree == null && matchesFilter(s),
      )) {
        children.add(
          SidebarRow.sessionRow(s, selected: s.id == selectedSessionId),
        );
      }
    }
    return UiColumn('project-tree-${project.id}', children);
  }

  List<UiAction> actions() => [
    UiAction(
      name: 'sidebar.filter',
      keys: Keymap.find(),
      context: UiActionContext.node('sidebar'),
    ),
    UiAction(name: 'session.new', keys: Keymap.newSession()),
    UiAction(
      name: 'session.select-next',
      keys: Keymap.switcherNext,
      context: UiActionContext.node('sidebar'),
    ),
  ];
}

/// Session context menu (#153).
///
/// Items: rename, copy ID, copy transcript, notify when done, clear
/// attention, resume, restart app, reveal, pin, stop and archive, remove.
/// Rendered as a column of buttons; popup positioning needs a framework
/// menu primitive (see docs/gpuidart-gaps-sidebar.md G-2).
final class SessionContextMenu {
  const SessionContextMenu({required this.sessionId});

  final String sessionId;

  /// (action name, label) pairs, in menu order.
  static const List<(String, String)> items = [
    ('session.rename', 'Rename…'),
    ('session.copy-id', 'Copy Session ID'),
    ('session.copy-transcript', 'Copy Transcript'),
    ('session.notify-when-done', 'Notify When Done'),
    ('session.clear-attention', 'Clear Attention'),
    ('session.resume', 'Resume'),
    ('session.restart-app', 'Restart App'),
    ('session.reveal', 'Reveal in Finder'),
    ('session.pin', 'Pin'),
    ('session.stop-archive', 'Stop and Archive'),
    ('session.remove', 'Remove…'),
  ];

  UiNode build() {
    return UiColumn('session-menu-$sessionId', [
      for (final (name, label) in items)
        UiButton('menu-$sessionId-$name', label),
    ]);
  }

  /// Menu item activation arrives as a native `click` event whose id is the
  /// button id (`menu-<sessionId>-<action>`); the host decodes the action
  /// from the id suffix. No key bindings: menu items are pointer-driven.
  static String actionForButtonId(String buttonId) {
    const prefix = 'menu-';
    if (!buttonId.startsWith(prefix)) return '';
    final rest = buttonId.substring(prefix.length);
    final dash = rest.indexOf('-');
    if (dash < 0) return '';
    return rest.substring(dash + 1);
  }
}

/// Project context menu (#154).
///
/// Items: new worktree, new group, rename, stop all, sort, folder color,
/// archived, open in editor, move to workspace.
final class ProjectContextMenu {
  const ProjectContextMenu({required this.projectId});

  final String projectId;

  static const List<(String, String)> items = [
    ('project.new-worktree', 'New Worktree…'),
    ('project.new-group', 'New Group'),
    ('project.rename', 'Rename…'),
    ('project.stop-all', 'Stop All Sessions'),
    ('project.sort', 'Sort Sessions'),
    ('project.folder-color', 'Folder Color…'),
    ('project.archived', 'Show Archived'),
    ('project.open-in-editor', 'Open in Editor'),
    ('project.move-to-workspace', 'Move to Workspace…'),
  ];

  UiNode build() {
    return UiColumn('project-menu-$projectId', [
      for (final (name, label) in items)
        UiButton('menu-$projectId-$name', label),
    ]);
  }

  /// Same click-id decoding as [SessionContextMenu.actionForButtonId].
  static String actionForButtonId(String buttonId) =>
      SessionContextMenu.actionForButtonId(buttonId);
}

// ─── SidebarView.swift remaining behaviours (feat/port-sidebarview-b) ───
// The sections above cover the tree models, row rendering, and context
// menus (#151-154). The sections below port the remaining SwiftUI views
// from SidebarView.swift (4,406 lines) that are portable to gpuidart's
// UiNode model. Platform-only pieces (AppKit/SwiftUI drag-drop plumbing,
// NSViewRepresentable hover monitors, CoreText glyph metrics) are noted
// inline and in docs/parity/swift-port/sidebar.yml rather than ported.

/// A cubic-bezier animation spec (gpuidart has no Animation type; the
/// renderer consumes these numbers).
final class SidebarAnimation {
  const SidebarAnimation({
    required this.c1x,
    required this.c1y,
    required this.c2x,
    required this.c2y,
    required this.durationMs,
    this.delayMs = 0,
  });

  final double c1x;
  final double c1y;
  final double c2x;
  final double c2y;
  final int durationMs;
  final int delayMs;
}

/// Motion constants (SidebarView.swift: SidebarMotion, Svelte parity).
///
/// - Worktrees slide-in: fly x ±140, 200ms cubicOut (Sidebar.svelte:465,491)
/// - Accordion open: 340ms cubic-bezier(0.16,1,0.3,1); content fade 220ms
///   ease-out from translateY(-6) scale(0.992) (ProjectItem.svelte:2225-2262)
/// - Accordion close: 240ms cubicInOut, content fade 140ms
///   (ProjectItem.svelte:568-613)
/// - Row entrance: 380ms cubic-bezier(0.18,0.86,0.26,1), stagger 14ms/row,
///   from translate(-5,-4) scale(0.988) (ProjectItem.svelte:2264-2270)
final class SidebarMotion {
  const SidebarMotion._();

  /// Svelte `fly` default easing is cubicOut ≈ cubic-bezier(0.33,1,0.68,1).
  static const slide = SidebarAnimation(
    c1x: 0.33,
    c1y: 1,
    c2x: 0.68,
    c2y: 1,
    durationMs: 200,
  );

  /// Accordion open: 340ms cubic-bezier(0.16, 1, 0.3, 1).
  static const accordionOpen = SidebarAnimation(
    c1x: 0.16,
    c1y: 1,
    c2x: 0.3,
    c2y: 1,
    durationMs: 340,
  );

  /// Accordion close: 240ms cubicInOut ≈ cubic-bezier(0.65, 0, 0.35, 1).
  static const accordionClose = SidebarAnimation(
    c1x: 0.65,
    c1y: 0,
    c2x: 0.35,
    c2y: 1,
    durationMs: 240,
  );

  /// Row entrance: 380ms cubic-bezier(0.18, 0.86, 0.26, 1), 14ms stagger.
  static SidebarAnimation rowEnter(int index) => SidebarAnimation(
        c1x: 0.18,
        c1y: 0.86,
        c2x: 0.26,
        c2y: 1,
        durationMs: 380,
        delayMs: index * 14,
      );

  /// Session-list content fade: 220ms in, 140ms out.
  static const sessionListFadeInMs = 220;
  static const sessionListFadeOutMs = 140;

  /// Rows fade (140ms) with the collapsing shell on removal.
  static const rowRemoveFadeMs = 140;

  /// Sidebar pane slide distance (project tree flies to x:-140, settings
  /// nav flies in from x:+140; both fade).
  static const panelSlideOffset = 140.0;
}

/// Folder-drop highlight overlay (SidebarView.swift:
/// SidebarFolderDropHighlight).
///
/// Shows while a folder hovers empty list space (container-level onDrop
/// targeting) OR directly over a row (row delegates' hover counter) — both
/// mean "drop to add project". Rendered as a non-hit-testing overlay so it
/// never steals the drop.
final class SidebarFolderDropHighlight {
  const SidebarFolderDropHighlight._();

  /// Visibility rule: externally targeted (empty list space) or any row
  /// hover counter active.
  static bool visible({
    required bool externallyTargeted,
    required int folderHoverCount,
  }) =>
      externallyTargeted || folderHoverCount > 0;

  static const cornerRadius = 12.0;
  static const padding = 6.0;

  /// Accent wash: 8% fill (#4a9eff14) + 55% stroke (#4a9eff8c)
  /// (Theme.accent), encoded as #RRGGBBAA hex (UiColor has no withOpacity).
  static final _stroke = UiColor.hex('#4a9eff8c');

  static UiNode build({
    required bool externallyTargeted,
    required int folderHoverCount,
  }) {
    if (!visible(
      externallyTargeted: externallyTargeted,
      folderHoverCount: folderHoverCount,
    )) {
      return const UiText('folder-drop-hidden', '');
    }
    return UiText(
      'folder-drop-highlight',
      '⬢ Drop to add project',
      style: UiStyle(foreground: _stroke, fontSize: 12),
    );
  }
}

/// Hermite smoothstep: zero first derivative at both ends, so gradient
/// ramps built from it start and finish without visible edges.
double sidebarSmoothstep(double t) {
  final x = t.clamp(0.0, 1.0);
  return x * x * (3 - 2 * x);
}

/// Vertical fade mask over the sidebar lists (SidebarView.swift:
/// SidebarListFadeMask). The top stays faintly visible so rows blur into
/// the chrome instead of disappearing behind a hard transparent cut.
final class SidebarListFadeMask {
  const SidebarListFadeMask._();

  static const topMinOpacity = 0.0;

  /// Opaque 76pt down from the top; 26pt bottom fade.
  static const opaqueHeight = 76.0;
  static const bottomFadeHeight = 26.0;

  /// Gradient stops sample the smoothstep densely enough to stay smooth
  /// (8 steps; the renderer interpolates linearly between stops).
  static List<double> topStopAlphas({int steps = 8}) {
    final alphas = <double>[topMinOpacity];
    for (var step = 0; step <= steps; step++) {
      final t = step / steps;
      alphas.add(topMinOpacity + (1 - topMinOpacity) * sidebarSmoothstep(t));
    }
    return alphas;
  }
}

/// Scroll-target geometry for session rows (SidebarView.swift:
/// SessionScrollTarget). Each row registers an invisible scroll target
/// that extends [margin] past the row on both ends, so a minimal scrollTo
/// stops with the row clear of the top chrome veil and the bottom fade.
final class SessionScrollTarget {
  const SessionScrollTarget._();

  static const margin = 48.0;

  static String id(String sessionID) => 'scroll-target:$sessionID';
}

/// Absolute-top scroll anchor for the project tree (a 1pt marker pinned to
/// the padded content's top edge, so `anchor: .top` means offset 0).
const treeTopScrollId = 'supercli.sidebar.tree-top';

/// The inputs that drive the project tree's programmatic scrolling
/// (SidebarView.swift: SidebarScrollCue): scope switch → top; selection
/// change → reveal the row.
final class SidebarScrollCue {
  const SidebarScrollCue({required this.scope, this.selection});

  final String scope;
  final String? selection;
}

/// Programmatic scroll decision derived from a cue change.
final class SidebarScrollDecision {
  const SidebarScrollDecision._({this.targetId, this.toTop = false});

  const SidebarScrollDecision.top() : this._(toTop: true);
  SidebarScrollDecision.session(String sessionId)
      : this._(targetId: SessionScrollTarget.id(sessionId));
  const SidebarScrollDecision.none() : this._();

  final String? targetId;
  final bool toTop;

  bool get isNone => !toTop && targetId == null;
}

/// Resolve a scroll-cue change to a scroll decision: a scope switch scrolls
/// to the top without animation; a selection change reveals the row with a
/// 150ms ease-out (only when the row actually needs revealing).
SidebarScrollDecision decideSidebarScroll({
  SidebarScrollCue? previous,
  required SidebarScrollCue current,
  required bool selectionNeedsReveal,
}) {
  if (previous != null && previous.scope != current.scope) {
    return const SidebarScrollDecision.top();
  }
  final selection = current.selection;
  if (selection != null &&
      selection != previous?.selection &&
      selectionNeedsReveal) {
    return SidebarScrollDecision.session(selection);
  }
  return const SidebarScrollDecision.none();
}

/// A destination in the session context menu's "Move to" flyout
/// (SidebarView.swift: SessionMoveTarget).
final class SessionMoveTarget {
  const SessionMoveTarget({required this.id, required this.name});

  final String id;
  final String name;
}

/// Branch label for a project row (SidebarView.swift: SidebarBranchLabel).
///
/// Shows the checked-out branch in monospaced 10pt, muted at 55% opacity.
/// Renders nothing (empty string) when the branch equals the project name —
/// the branch is only interesting when it differs.
final class SidebarBranchLabel {
  const SidebarBranchLabel({required this.branch, required this.projectName});

  final String branch;
  final String projectName;

  /// Whether the label renders: hidden when branch == projectName.
  bool get visible => branch != projectName;

  /// Display text: empty when not visible.
  String get text => visible ? branch : '';

  UiNode build(String nodeId) {
    if (!visible) return const UiText('branch-hidden', '');
    return UiText(
      nodeId,
      '⎇ $branch',
      style: const UiStyle(fontSize: 10),
    );
  }
}

/// Active-project branch label (SidebarView.swift: ActiveProjectBranchLabel).
///
/// Shows the [SidebarBranchLabel] only for the project holding the selected
/// session. A leaf on purpose: it observes the selection and the async
/// branch resolution without invalidating the project row around it.
/// Renders nothing for every other project, and nothing while the project
/// is a worktree (branch shown elsewhere).
final class ActiveProjectBranchLabel {
  const ActiveProjectBranchLabel({
    required this.projectId,
    required this.selectedSessionProjectId,
    required this.branchName,
    required this.projectName,
    this.isWorktree = false,
  });

  final String projectId;
  final String? selectedSessionProjectId;
  final String? branchName;
  final String projectName;
  final bool isWorktree;

  /// Whether this project holds the selected session.
  bool get isActiveProject => selectedSessionProjectId == projectId;

  /// Whether the label renders at all.
  bool get visible =>
      isActiveProject && !isWorktree && branchName != null;

  UiNode build() {
    if (!visible) return const UiText('active-branch-hidden', '');
    return SidebarBranchLabel(
      branch: branchName!,
      projectName: projectName,
    ).build('active-branch-$projectId');
  }
}

/// Sidebar footer strip (SidebarView.swift: SidebarFooter).
///
/// One "+" for both create verbs (Add Project…, Add Workspace…) plus the
/// Settings button. Add Project is a Controller-local filesystem verb, so
/// its row hides while a remote Host is scoped ([localVerbsVisible] false);
/// Add Workspace always applies. Collapse-all moved to the menu bar
/// (Session ▸ Collapse All Folders, ⌥⌘B) — the footer keeps only "+",
/// the workspace dots, and settings.
final class SidebarFooter {
  const SidebarFooter({this.localVerbsVisible = true});

  /// False while a remote Host is selected: the session-tree local verbs
  /// (Add Project) disappear.
  final bool localVerbsVisible;

  SidebarFooterAddMenu get addMenu =>
      SidebarFooterAddMenu(localVerbsVisible: localVerbsVisible);
}

/// The footer "+" popover rows (SidebarView.swift: FooterAddMenuRow).
/// A tap row rather than a Button, matching the workspace picker's rows.
final class SidebarFooterAddMenu {
  const SidebarFooterAddMenu({this.localVerbsVisible = true});

  final bool localVerbsVisible;

  /// (action name, label) pairs, in menu order.
  List<(String, String)> get items => [
        if (localVerbsVisible) ('project.add', 'Add Project…'),
        ('workspace.add', 'Add Workspace…'),
      ];

  UiNode build() {
    return UiColumn('footer-add-menu', [
      for (final (action, label) in items)
        UiButton('footer-add-$action', label),
    ]);
  }
}

/// Collapse-all state (SidebarView.swift: the menu-bar Session ▸
/// Collapse All Folders item, ⌥⌘B). Disabled while nothing is expanded
/// (mirrors the Svelte binding `disabled={$expandedProjectIds.size===0}`).
final class SidebarCollapseAll {
  const SidebarCollapseAll({this.expandedProjectCount = 0});

  final int expandedProjectCount;

  bool get enabled => expandedProjectCount > 0;
}

/// Row hover action buttons (SidebarView.swift: ArchiveActionButton,
/// RemoveActionButton, RestartActionButton).
///
/// 22×22 radius-6 buttons shown in the row's meta slot on hover, muted →
/// foreground + hover-row bg on hover:
/// - Archive (for resumable sessions): archives the session.
/// - Remove/X (for non-resumable sessions): immediate kill/delete; the
///   context-menu Remove verb still confirms.
/// - Restart (for exited rows): Resume session (continues the conversation).
final class RowActionButtons {
  const RowActionButtons._();

  /// Whether the archive button shows (session can resume → archive it).
  static bool showsArchive(bool canResume) => canResume;

  /// Whether the remove button shows (session cannot resume → remove it).
  static bool showsRemove(bool canResume) => !canResume;

  /// Whether the restart/resume affordance shows (stopped, resumable).
  static bool showsRestart(bool canRestart) => canRestart;

  static const buttonSize = 22.0;
  static const cornerRadius = 6.0;

  static UiNode archiveButton(String sessionId) =>
      UiButton('row-archive-$sessionId', '🗃');

  static UiNode removeButton(String sessionId) =>
      UiButton('row-remove-$sessionId', '✕');

  static UiNode restartButton(String sessionId) =>
      UiButton('row-restart-$sessionId', '↻');
}

/// Exact resume affordance rendered by a sidebar row (SidebarView.swift:
/// SessionRowResumePresentation). Keeping this decision pure makes the
/// active-runtime, returned-shell, and archived states testable without
/// reaching through the view tree.
enum SessionRowResumePresentation {
  none,
  resumeAgent,
  resumeSession,
  restore,
  restoreAndResume,
}

extension SessionRowResumePresentationX on SessionRowResumePresentation {
  String? get title => switch (this) {
        SessionRowResumePresentation.none => null,
        SessionRowResumePresentation.resumeAgent => 'Resume Agent',
        SessionRowResumePresentation.resumeSession => 'Resume',
        SessionRowResumePresentation.restore => 'Restore from archive',
        SessionRowResumePresentation.restoreAndResume => 'Restore & Resume',
      };
}

/// Resume-affordance decision (SidebarView.swift:
/// sessionRowResumePresentation).
SessionRowResumePresentation sessionRowResumePresentation({
  required bool isArchived,
  required bool canRestart,
  required bool canResumeAgent,
  required bool isLive,
  required bool isStarting,
}) {
  if (isArchived) {
    return canRestart
        ? SessionRowResumePresentation.restoreAndResume
        : SessionRowResumePresentation.restore;
  }
  if (isStarting) return SessionRowResumePresentation.none;
  if (isLive) {
    return canResumeAgent
        ? SessionRowResumePresentation.resumeAgent
        : SessionRowResumePresentation.none;
  }
  return canRestart
      ? SessionRowResumePresentation.resumeSession
      : SessionRowResumePresentation.none;
}

/// Whether the resume affordance renders inline on the row
/// (SidebarView.swift: sessionRowShowsInlineResume).
bool sessionRowShowsInlineResume(SessionRowResumePresentation presentation) =>
    presentation == SessionRowResumePresentation.resumeAgent ||
    presentation == SessionRowResumePresentation.resumeSession ||
    presentation == SessionRowResumePresentation.restoreAndResume;

/// Which command the row's activity spinner represents (SidebarView.swift:
/// sessionRowActivitySpinnerCommand). The representative is only the
/// sidebar anchor for a pane group: preserve its existing tint when it is
/// working, otherwise let the first working pane drive the collapsed row's
/// spinner so activity never disappears with a hidden member row.
/// Needs-input wins over a sibling spinner so the attention badge stays
/// visible on a collapsed pane group waiting for an MCP approval.
String? sessionRowActivitySpinnerCommand({
  required bool needsAttention,
  required bool isWorking,
  required String? presentationCommand,
  required List<String> paneWorkingCommands,
}) {
  if (needsAttention) return null;
  if (isWorking) return presentationCommand;
  return paneWorkingCommands.isEmpty ? null : paneWorkingCommands.first;
}

/// Whether the row shows "Copy Transcript" (SidebarView.swift:
/// sessionRowShowsCopyTranscript). A collapsed multi-pane row cannot
/// identify which conversation the user means, so each pane's own
/// ellipsis menu carries the action instead.
bool sessionRowShowsCopyTranscript({
  required bool paneItemsEmpty,
  required bool supportsTranscriptCopy,
}) =>
    paneItemsEmpty && supportsTranscriptCopy;

/// Quick-preset strip on project rows (SidebarView.swift: QuickPresetStrip).
///
/// Collapsed to a 28pt chip; expands on hover (or forced) to
/// `(quickGroups.count + 1) * 23 + 30` pt. Strip order is row-reverse: the
/// first group (topmost starred CLI) renders rightmost, next to "+"; the
/// blank terminal chip sits leftmost. A CLI with 2+ starred presets renders
/// as a menu chip opening that CLI's starred presets.
final class QuickPresetStrip {
  const QuickPresetStrip({
    this.quickGroupCount = 0,
    this.hovering = false,
    this.forceExpanded = false,
  });

  final int quickGroupCount;
  final bool hovering;
  final bool forceExpanded;

  bool get expanded => hovering || forceExpanded;

  double get expandedWidth => (quickGroupCount + 1) * 23 + 30;

  double get collapsedWidth => 28;

  static const height = 24.0;

  /// Strip expand animation: 280ms cubic-bezier(0.22, 1, 0.36, 1).
  static const expandAnimation = SidebarAnimation(
    c1x: 0.22,
    c1y: 1,
    c2x: 0.36,
    c2y: 1,
    durationMs: 280,
  );

  /// Whether preset group at [index] renders as a menu chip (2+ starred
  /// presets) rather than a single quick-launch button.
  static bool isMenuChip(int presetCount) => presetCount > 1;

  /// Builds the strip for FFI-loaded [groups] (see [quickPresetGroups]).
  factory QuickPresetStrip.fromGroups(
    List<QuickPresetGroupView> groups, {
    bool hovering = false,
    bool forceExpanded = false,
  }) =>
      QuickPresetStrip(
        quickGroupCount: groups.length,
        hovering: hovering,
        forceExpanded: forceExpanded,
      );
}

/// One quick-preset group from the Rust `collect_quick_preset_groups`
/// (via FFI). Grouping logic lives in Rust; this is the UI-side view used
/// to render the [QuickPresetStrip] chips.
final class QuickPresetGroupView {
  const QuickPresetGroupView({
    required this.id,
    this.cliId,
    this.appId,
    this.appName,
    this.presets = const [],
  });

  final String id;
  final String? cliId;
  final String? appId;
  final String? appName;
  final List<LaunchPreset> presets;

  factory QuickPresetGroupView.fromJson(Map<String, dynamic> json) {
    return QuickPresetGroupView(
      id: json['id'] as String? ?? '',
      cliId: json['cli_id'] as String?,
      appId: json['app_id'] as String?,
      appName: json['app_name'] as String?,
      presets: [
        for (final p
            in (json['presets'] as List?)?.whereType<Map<String, dynamic>>() ??
                const <Map<String, dynamic>>[])
          LaunchPreset(
            id: p['id'] as String? ?? '',
            label: p['label'] as String? ?? '',
            command: p['command'] as String? ?? '',
            enabled: (p['enabled'] as bool?) ?? true,
            quickLaunch: (p['quick_launch'] as bool?) ?? false,
          ),
      ],
    );
  }

  /// Whether this group renders as a menu chip (2+ starred presets).
  bool get isMenuChip => QuickPresetStrip.isMenuChip(presets.length);
}

/// Loads quick-preset groups through the Rust `collect_quick_preset_groups`
/// (the single source of truth) via FFI. [pluginCommands] and [appCatalog]
/// carry the Host App-catalog classifications ([appCatalog] maps executable
/// head -> `[app_id, app_name]`). Only presets with `quickLaunch: true` can
/// form groups, mirroring the Rust filter.
List<QuickPresetGroupView> quickPresetGroups({
  required List<LaunchPreset> presets,
  Set<String> pluginCommands = const {},
  Map<String, List<String>> appCatalog = const {},
}) {
  final raw = SupercliNative.quickPresetGroups(
    presets: [
      for (final p in presets)
        {
          'id': p.id,
          'label': p.label,
          'command': p.command,
          'enabled': p.enabled,
          'quick_launch': p.quickLaunch,
        },
    ],
    pluginCommands: pluginCommands,
    appCatalog: appCatalog,
  );
  return [for (final g in raw) QuickPresetGroupView.fromJson(g)];
}

/// One launch preset for the new-session menu / quick-preset strip.
final class LaunchPreset {
  const LaunchPreset({
    required this.id,
    required this.label,
    required this.command,
    this.pluginId,
    this.enabled = true,
    this.quickLaunch = false,
  });

  final String id;
  final String label;
  final String command;

  /// Set for plugin-backed presets; null for plain agent presets.
  final String? pluginId;

  final bool enabled;
  final bool quickLaunch;

  bool get isPlugin => pluginId != null && pluginId!.isNotEmpty;
}

/// Splits [presets] into the Agents and Plugins sections of the new-session
/// menu through the Rust `split_presets_for_new_session_menu` (the single
/// source of truth) via FFI. Plugin identity comes from the Host's App
/// catalog, expressed here as each preset's [LaunchPreset.isPlugin]; the
/// section rule itself lives in Rust.
({List<LaunchPreset> agents, List<LaunchPreset> plugins}) splitPresetsForMenu(
  List<LaunchPreset> presets,
) {
  final split = SupercliNative.presetSplitForMenu(
    presets: [
      for (final p in presets)
        {
          'id': p.id,
          'label': p.label,
          'command': p.command,
          'enabled': p.enabled,
          'quick_launch': p.quickLaunch,
        },
    ],
    pluginCommands: {for (final p in presets) if (p.isPlugin) p.command},
  );
  // Rust's Preset has no plugin_id field, so it cannot round-trip; recover the
  // plugin tag from the input by stable ID.
  final pluginIdById = {for (final p in presets) p.id: p.pluginId};
  List<LaunchPreset> at(String key) => [
    for (final j in split[key] ?? const <Map<String, dynamic>>[])
      LaunchPreset(
        id: j['id'] as String? ?? '',
        label: j['label'] as String? ?? '',
        command: j['command'] as String? ?? '',
        pluginId: pluginIdById[j['id'] as String?],
        enabled: (j['enabled'] as bool?) ?? true,
        quickLaunch: (j['quick_launch'] as bool?) ?? false,
      ),
  ];
  return (agents: at('agents'), plugins: at('plugins'));
}

/// New-session menu model (SidebarView.swift: newSessionMenuContent).
///
/// The Agents/Plugins split is computed by the Rust
/// `split_presets_for_new_session_menu` (single source of truth) and passed
/// in pre-split — see [splitPresetsForMenu] / [newSessionMenuModelFor].
/// Section structure (order, manage buttons, archived row) is UI layout and
/// stays here.
///
/// Sections, in order:
/// 1. Blank terminal ("New Terminal") — always first.
/// 2. Agents (+ "Manage Agents…" when [showsManagePresets]).
/// 3. Plugins (+ "Manage Plugins…" when [showsManagePresets] and
///    [showsManagePlugins]).
/// 4. Archived (count) — only when [archivedCount] > 0.
final class NewSessionMenuModel {
  NewSessionMenuModel({
    required this.agents,
    required this.plugins,
    this.showsManagePresets = true,
    this.showsManagePlugins = false,
    this.archivedCount = 0,
  });

  final List<LaunchPreset> agents;
  final List<LaunchPreset> plugins;
  final bool showsManagePresets;
  final bool showsManagePlugins;
  final int archivedCount;

  List<NewSessionMenuSection> get sections {
    final sections = <NewSessionMenuSection>[
      const NewSessionMenuSection.blankTerminal(),
    ];
    if (agents.isNotEmpty || showsManagePresets) {
      sections.add(
        NewSessionMenuSection.agents(
          agents,
          showManage: showsManagePresets,
        ),
      );
    }
    if (plugins.isNotEmpty ||
        (showsManagePresets && showsManagePlugins)) {
      sections.add(
        NewSessionMenuSection.plugins(
          plugins,
          showManage: showsManagePresets && showsManagePlugins,
        ),
      );
    }
    if (archivedCount > 0) {
      sections.add(NewSessionMenuSection.archived(archivedCount));
    }
    return sections;
  }
}

/// Builds a [NewSessionMenuModel] by splitting [presets] through the Rust
/// single source of truth ([splitPresetsForMenu]).
NewSessionMenuModel newSessionMenuModelFor({
  required List<LaunchPreset> presets,
  bool showsManagePresets = true,
  bool showsManagePlugins = false,
  int archivedCount = 0,
}) {
  final split = splitPresetsForMenu(presets);
  return NewSessionMenuModel(
    agents: split.agents,
    plugins: split.plugins,
    showsManagePresets: showsManagePresets,
    showsManagePlugins: showsManagePlugins,
    archivedCount: archivedCount,
  );
}

/// One section of the new-session menu.
final class NewSessionMenuSection {
  const NewSessionMenuSection._({
    required this.kind,
    this.presets = const [],
    this.showManage = false,
    this.archivedCount = 0,
  });

  const NewSessionMenuSection.blankTerminal() : this._(kind: 'blank');
  const NewSessionMenuSection.agents(List<LaunchPreset> presets,
      {bool showManage = false})
      : this._(kind: 'agents', presets: presets, showManage: showManage);
  const NewSessionMenuSection.plugins(List<LaunchPreset> presets,
      {bool showManage = false})
      : this._(kind: 'plugins', presets: presets, showManage: showManage);
  const NewSessionMenuSection.archived(int count)
      : this._(kind: 'archived', archivedCount: count);

  final String kind;
  final List<LaunchPreset> presets;
  final bool showManage;
  final int archivedCount;
}

/// Empty-sessions placeholder row (SidebarView.swift:
/// EmptySessionsPlaceholderRow). The label opens the same new-session menu
/// as the "+" chip; leading indent is 28pt + 14pt per depth step.
final class EmptySessionsPlaceholderRow {
  const EmptySessionsPlaceholderRow({
    this.label = 'No sessions yet.',
    this.depth = 0,
    this.archivedCount = 0,
  });

  final String label;
  final int depth;
  final int archivedCount;

  double get leadingIndent => 28 + depth * 14;

  bool get showsArchived => archivedCount > 0;

  UiNode build() {
    return UiColumn('empty-sessions', [
      UiText('empty-sessions-label', label),
      if (showsArchived)
        UiButton('empty-sessions-archived', 'Archived ($archivedCount)'),
    ]);
  }
}

/// Empty-projects view (SidebarView.swift: SidebarEmptyProjectsView):
/// folder icon + prominent "Add Project" CTA, centered.
final class SidebarEmptyProjectsView {
  const SidebarEmptyProjectsView._();

  static UiNode build() {
    return UiColumn('empty-projects', const [
      UiText('empty-projects-icon', '📁'),
      UiButton('empty-projects-add', 'Add Project'),
    ]);
  }
}

/// Text-glyph chevron (SidebarView.swift: ChevronGlyph). The Swift version
/// measures CoreText ink bounds to center the glyph on its ink rather than
/// its line box; gpuidart renders the glyph directly — direction is the
/// portable behaviour.
final class ChevronGlyph {
  const ChevronGlyph._();

  /// Expanded → ▾, collapsed → ▸.
  static String glyphFor({required bool expanded}) =>
      expanded ? '▾' : '▸';
}

/// Attention indicator (SidebarView.swift: AttentionDot, DESIGN.md §5 /
/// .session-status.attention): static 6px dot with a 14px halo at 20% of
/// the dot color. No animation.
final class AttentionDot {
  const AttentionDot._();

  static const dotSize = 6.0;
  static const haloSize = 14.0;
  static const haloOpacity = 0.20;

  /// Attention glyph used in the UiNode fallback renderer.
  static const glyph = '●';
}

/// Pinned-session indicator (SidebarView.swift: SidebarPinnedIndicator):
/// passive 12px pin glyph, 14×18 frame, non-hit-testing, "Pinned" label.
/// Pinning is context-menu-only — unpinned rows never gain pin chrome on
/// hover.
final class SidebarPinnedIndicator {
  const SidebarPinnedIndicator._();

  static const accessibilityLabel = 'Pinned';
  static const glyphSize = 12.0;

  static UiNode build(String nodeId) => UiText(
        nodeId,
        '📌',
        style: const UiStyle(fontSize: 12),
      );
}

/// Hover wash behind a group/worktree cluster (SidebarView.swift:
/// GroupClusterBackground): rounded-rect at 50% hover-row, highlighted
/// while hovering OR while a descendant session is selected (inline folder
/// shells retain their wash while one of their descendants is active).
final class GroupClusterBackground {
  const GroupClusterBackground({
    required this.isHovering,
    required this.selectedSessionId,
    required this.descendantSessionIds,
  });

  final bool isHovering;
  final String? selectedSessionId;
  final List<String> descendantSessionIds;

  bool get highlighted =>
      isHovering ||
      (selectedSessionId != null &&
          descendantSessionIds.contains(selectedSessionId));

  static const cornerRadius = 10.0;

  /// Wash fill: hover-row at 50%.
  static const washOpacity = 0.5;
}

/// Hover marquee for overflowing session titles (SidebarView.swift:
/// HoverMarqueeSessionTitle). At rest the title truncates with a tail; on
/// hover an overflowing title glides through the same viewport beneath a
/// 10pt edge fade. The moving copy lives in an overlay so starting a
/// marquee never moves adjacent controls.
final class HoverMarqueeTitle {
  const HoverMarqueeTitle._();

  /// Glide speed: 28pt/s.
  static const pointsPerSecond = 28.0;

  /// Pause before the glide starts / at the end of travel.
  static const initialPauseMs = 500;
  static const endPauseMs = 900;

  /// Edge-fade width on each side.
  static const fadeWidth = 10.0;

  /// Overflow past the viewport (0 when it fits).
  static double overflowOf(double titleWidth, double viewportWidth) =>
      titleWidth > viewportWidth ? titleWidth - viewportWidth : 0;

  /// Travel duration: max(1.2s, overflow / 28pt/s), in milliseconds.
  /// Returns 0 when there is no overflow (no marquee).
  static double travelDurationMs(double overflow) {
    if (overflow <= 0) return 0;
    final ms = overflow / pointsPerSecond * 1000;
    return ms < 1200 ? 1200 : ms;
  }
}

/// Non-interactive runtime mark shown right of the date (SidebarView.swift:
/// SessionCommandIcon). Fixed-size so the hover swap in the adjacent meta
/// slot never reflows it. The asset comes from the runtime catalog
/// (`display.kind` + optional `icon_asset`), not a client provider table.
final class SessionCommandIconSpec {
  const SessionCommandIconSpec({this.kind = '', this.iconAsset});

  final String kind;
  final String? iconAsset;

  bool get visible => iconAsset != null && iconAsset!.isNotEmpty;
}

/// Spec for the pane-group icon stack (SidebarView.swift:
/// SessionCommandIconStack/SessionCommandIconStackTile). A pane group is one
/// sidebar row representing several sessions: up to 4 small opaque tiles
/// overlap like an avatar stack (hairline keeps same-color marks distinct).
/// Tiles after the 4th are not rendered.
final class SessionCommandIconStackSpec {
  const SessionCommandIconStackSpec({required this.itemCount});

  final int itemCount;

  /// Max tiles rendered; the rest are dropped, not paged.
  static const maxVisibleTiles = 4;

  int get visibleTileCount =>
      itemCount < maxVisibleTiles ? itemCount : maxVisibleTiles;

  /// Stack enter transition: scale 0.82 + opacity, spring(0.36, 0.76).
  static const enterScale = 0.82;
  static const springResponse = 0.36;
  static const springDampingFraction = 0.76;
}

/// Inline rename field state (SidebarView.swift: SessionRowRenameField).
/// The field edits a draft, claims focus once on appear, commits on
/// confirm and cancels on escape; a commit triggered by focus loss right
/// after cancel is suppressed.
final class SessionRowRenameState {
  SessionRowRenameState({required String initialLabel})
      : draft = initialLabel;

  String draft;
  bool didClaimFocus = false;
  bool suppressCommit = false;

  /// Called once when the field appears; returns true when focus was claimed.
  bool claimFocus() {
    if (didClaimFocus) return false;
    didClaimFocus = true;
    return true;
  }

  /// Resolve the field: commit unless a cancel just suppressed it.
  /// Returns the committed label, or null when suppressed/cancelled.
  String? resolve({required bool cancelled}) {
    if (cancelled) {
      suppressCommit = true;
      return null;
    }
    if (suppressCommit) {
      suppressCommit = false;
      return null;
    }
    return draft;
  }
}

/// Selection wash spec (SidebarView.swift: SessionRowSelectionPaint).
/// 9pt continuous rounded rect; selected rows take the glass tint (or the
/// theme's active-row tint where glass is unavailable), hovered rows the
/// hover tint. Selection is navigation, not an animated transition: the
/// fill swaps with animations disabled. The Liquid Glass variant
/// (SelectedRowGlass/WindowKeyState) is AppKit-only and not ported.
final class SessionRowSelectionSpec {
  const SessionRowSelectionSpec({
    required this.isSelected,
    this.isHovering = false,
    this.glassAvailable = false,
  });

  final bool isSelected;
  final bool isHovering;
  final bool glassAvailable;

  static const cornerRadius = 9.0;

  /// Which fill the paint uses: 'glass', 'activeTint', 'hover', or 'clear'.
  String get fillKind {
    if (isSelected) return glassAvailable ? 'glass' : 'activeTint';
    if (isHovering) return 'hover';
    return 'clear';
  }

  /// Selection never animates the fill swap.
  static const animatesFillSwap = false;
}

/// Quick-preset button spec (SidebarView.swift: QuickPresetButton/
/// QuickPresetMenuChip). 22x22 icon chip; the icon rests at 72% opacity and
/// goes full on hover over an 8pt-radius hover wash. Tooltip: "Start <label>"
/// ("Start <group>…" for the menu chip).
final class QuickPresetButtonSpec {
  const QuickPresetButtonSpec({this.hovering = false});

  final bool hovering;

  static const size = 22.0;
  static const cornerRadius = 8.0;
  static const restingOpacity = 0.72;

  double get iconOpacity => hovering ? 1.0 : restingOpacity;

  static String tooltipForPreset(String label) => 'Start $label';
  static String tooltipForGroup(String displayName) => 'Start $displayName…';
}

/// Inline confirm pill (SidebarView.swift: the confirm control used by the
/// remove/archive confirm rows): label + destructive styling.
final class ConfirmPill {
  const ConfirmPill({required this.label, this.destructive = false});

  final String label;
  final bool destructive;

  UiNode build(String nodeId) => UiButton(nodeId, label);
}

/// Project-state rollups (SidebarView.swift: ProjectNodeView's
/// aggregateHasAttention / aggregateHasUnread / showsBusyShimmer).
///
/// - Collapsed-project attention rollup: any descendant session needing
///   attention (precedence attention > unread, mirroring the
///   `project-state-dot` markup).
/// - Unread rollup: any descendant session unread.
/// - Busy shimmer on the project name: any descendant session starting or
///   busy; attention outranks busy, so attention suppresses the shimmer.
///   A busy session hidden inside a folded subtree still reads as activity
///   on the visible row.
///
/// Note: the Dart [SidebarProject] models worktrees as names only, so the
/// recursive worktree-descendant walk is approximated by [allSessions],
/// which already includes the worktree-folder sessions.
extension SidebarProjectRollups on SidebarProject {
  bool get aggregateHasAttention =>
      allSessions.any((s) => s.attention);

  bool get aggregateHasUnread =>
      allSessions.any((s) => s.unreadCount > 0);

  bool get showsBusyShimmer =>
      !aggregateHasAttention && allSessions.any((s) => s.busy);
}
