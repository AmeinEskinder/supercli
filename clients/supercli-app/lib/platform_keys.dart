/// Platform-specific keyboard modifier for shortcuts.
///
/// The primary modifier is `meta` on macOS (Cmd) and `ctrl` on Linux/Windows.
/// The gpuidart key parser accepts `meta` and maps it to the platform meta
/// key (Cmd on macOS, Super/Win on Linux/Windows), so `meta+k` is Cmd-K on
/// macOS and Ctrl-K on Linux/Windows.
///
/// Platform-neutral chords (e.g. `ctrl+tab` for the MRU switcher) stay as
/// `ctrl+` on all platforms by design. The approval overlay uses plain
/// Return / Escape (MCPApprovalPanel.swift:249-266).
library;

import 'dart:io' show Platform;

/// Returns the primary modifier for the given platform.
///
/// [isMacOS] selects `meta` (Cmd); Linux and Windows use `ctrl`.
/// Split out from [currentPrimaryModifier] so both mappings are unit-testable.
String primaryModifier({required bool isMacOS}) => isMacOS ? 'meta' : 'ctrl';

/// The primary modifier for the current platform.
String get currentPrimaryModifier => primaryModifier(isMacOS: Platform.isMacOS);
