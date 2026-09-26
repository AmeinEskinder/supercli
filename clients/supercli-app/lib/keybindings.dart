/// App-wide keyboard shortcuts: Cmd-K palette, Ctrl-Tab MRU switcher.
///
/// Declares the [UiAction] bindings the Rust host listens for. `keys` strings
/// follow the UiAction grammar: modifiers `ctrl|alt|shift|meta` plus one key
/// (letters, digits, f1-f12, enter, escape, space, tab, arrows, home/end,
/// pageup/pagedown, delete, backspace), joined with `+`.
///
/// `meta` is the platform meta key (Cmd on macOS, Super on Linux/Windows),
/// so `meta+k` is Cmd-K on macOS and Win-K/Super-K elsewhere.
///
/// NOTE (gap): gpuidart has no programmatic focus API, so opening the
/// palette cannot move keyboard focus into the filter input. Key routing
/// works through node-scoped [UiActionContext] instead; see
/// docs/gpuidart-gaps-cmdk.md G-1.
library;

import 'package:gpuidart/gpuidart.dart';

/// Logical key chords for the app.
final class AppKeybindings {
  const AppKeybindings();

  /// Open the command palette (Cmd-K / Super-K).
  static const paletteOpen = 'meta+k';

  /// Cycle the MRU session/pane switcher forward / backward.
  static const switcherNext = 'ctrl+tab';
  static const switcherPrevious = 'ctrl+shift+tab';

  /// Palette list navigation (scoped to the open palette node).
  static const paletteUp = 'up';
  static const paletteDown = 'down';
  static const paletteConfirm = 'enter';
  static const paletteDismiss = 'escape';

  /// Dismiss the MRU switcher overlay without switching.
  static const switcherDismiss = 'escape';

  /// Global (unscoped) bindings. The host registers these once at startup.
  List<UiAction> globalActions() => const [
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
          name: 'switcher.dismiss',
          keys: switcherDismiss,
          context: UiActionContext.node(switcherNodeId),
        ),
      ];
}
