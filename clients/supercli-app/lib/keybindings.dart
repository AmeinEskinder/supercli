/// App-wide keyboard shortcuts: palette, Ctrl-Tab MRU switcher.
///
/// Declares the [UiAction] bindings the Rust host listens for. `keys` strings
/// follow the UiAction grammar: modifiers `ctrl|alt|shift|meta` plus one key
/// (letters, digits, f1-f12, enter, escape, space, tab, arrows, home/end,
/// pageup/pagedown, delete, backspace), joined with `+`.
///
/// All chord values come from [Keymap] (`lib/keymap.dart`), the single
/// source of truth ported from the native macOS menu. The primary modifier
/// is `meta` (Cmd) on macOS and `ctrl` on Linux/Windows; platform-neutral
/// chords (Ctrl-Tab switcher) stay `ctrl+` everywhere by design. The
/// approval overlay uses plain Return / Escape (MCPApprovalPanel.swift).
///
/// NOTE (gap): gpuidart has no programmatic focus API, so opening the
/// palette cannot move keyboard focus into the filter input. Key routing
/// works through node-scoped [UiActionContext] instead; see
/// docs/gpuidart-gaps-cmdk.md G-1.
library;

import 'package:gpuidart/gpuidart.dart';

import 'keymap.dart';

/// Logical key chords for the app.
///
/// Chord values come from [Keymap], the single source of truth
/// (`lib/keymap.dart`). No chord literal may be defined here.
final class AppKeybindings {
  const AppKeybindings();

  /// Open the command palette ([Keymap.commandPalette]).
  static String get paletteOpen => Keymap.commandPalette();

  /// Cycle the MRU session/pane switcher forward / backward.
  /// Genuinely Ctrl on every platform ([Keymap.switcherNext]).
  static const String switcherNext = Keymap.switcherNext;
  static const String switcherPrevious = Keymap.switcherPrevious;

  /// Palette list navigation (scoped to the open palette node).
  static const paletteUp = 'up';
  static const paletteDown = 'down';
  static const paletteConfirm = 'enter';
  static const paletteDismiss = 'escape';

  /// Dismiss the MRU switcher overlay without switching.
  static const switcherDismiss = 'escape';

  /// Confirm the highlighted MRU entry (Enter). Releasing Ctrl also
  /// confirms on the host side; Enter covers keyboard-only flows.
  static const switcherConfirm = 'enter';

  /// Global (unscoped) bindings. The host registers these once at startup.
  List<UiAction> globalActions() => [
        UiAction(name: 'palette.open', keys: paletteOpen),
        UiAction(name: 'switcher.next', keys: switcherNext),
        UiAction(name: 'switcher.previous', keys: switcherPrevious),
      ];

  /// Bindings scoped to the open palette overlay node.
  List<UiAction> paletteActions(String paletteNodeId) => [
        UiAction(
          name: 'palette.up',
          keys: paletteUp,
          context: UiActionContext.node(paletteNodeId),
        ),
        UiAction(
          name: 'palette.down',
          keys: paletteDown,
          context: UiActionContext.node(paletteNodeId),
        ),
        UiAction(
          name: 'palette.confirm',
          keys: paletteConfirm,
          context: UiActionContext.node(paletteNodeId),
        ),
        UiAction(
          name: 'palette.dismiss',
          keys: paletteDismiss,
          context: UiActionContext.node(paletteNodeId),
        ),
      ];

  /// Bindings scoped to the open MRU switcher overlay node.
  List<UiAction> switcherActions(String switcherNodeId) => [
        UiAction(
          name: 'switcher.confirm',
          keys: switcherConfirm,
          context: UiActionContext.node(switcherNodeId),
        ),
        UiAction(
          name: 'switcher.dismiss',
          keys: switcherDismiss,
          context: UiActionContext.node(switcherNodeId),
        ),
      ];
}
