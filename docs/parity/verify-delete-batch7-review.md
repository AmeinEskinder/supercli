# Verify-Delete Batch 7 — Review Note

**Branch:** `feat/verify-delete-batch7`
**Base:** `origin/next@7f8f4e4`
**Date:** 2026-09-29

This batch prepares deletions for reviewer approval. Ledger rows are marked `pending`
(NOT `merged`). No Swift files are deleted on this branch — the reviewer must approve
before the actual deletion merges.

## Ready for deletion

These files are marked `ported` on `origin/next`, have all behaviours verified by
named passing tests, and have **zero** Swift callers in `clients/legacy`.

### 1. RemoteUnpeelClient.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteUnpeelClient.swift`
- **LOC:** 172
- **Ported destination:** Rust `supercli-client` (`crates/supercli-client/src/remote_supercli_peer.rs`)
- **Behaviours → tests (all verified present in `remote_supercli_peer.rs`):**
  - `RemoteSupercliPeer.parse` → `tests::parse_valid_remote_key`
  - `RemoteSupercliSession.fromRow` → `tests::session_from_row_full`
  - Certificate pinning (SHA-256 fingerprint) → `tests::fingerprint_match_accepts`
  - `RemoteSupercliError` 401 vs generic → `tests::error_401_is_unauthorized`
  - `RemoteSupercliPeerStore` atomic write/load/clear → `tests::peer_file_roundtrip`
- **Frozen Swift callers:** none
- **Safe to delete because:** All 5 behaviours are ported to Rust with passing unit
  tests, and no Swift file in `clients/legacy` references `RemoteUnpeelClient`,
  `RemoteSupercliPeer`, or `RemoteSupercliSession`.

### 2. OpenCodeTheme.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Sources/SupercliNative/OpenCodeTheme.swift`
- **LOC:** 1,093
- **Ported destination:** Rust `supercli-core::provider_theme` (`crates/supercli-core/src/provider_theme.rs`)
- **Behaviours → tests (verified present in `provider_theme.rs`):**
  - `TerminalFrameStyle.Background` → `tests::background_signature_matches_swift_format`
  - `OpenCodeThemeResolver.background(workingDirectory:)` → `tests::opencode_config_theme_is_resolved_from_working_dir`, `opencode_custom_theme_file_background_wins`, `opencode_system_theme_resolves_to_none`
  - 43 built-in backgrounds → `tests::aura_uses_opencode_canvas_background`, `default_opencode_background_matches_built_in_theme`, `single_mode_themes_do_not_invent_light_backgrounds`
  - `normalizeThemeName` / `parseHexColor` / `resolveHex` → corresponding Rust fns with tests
- **Frozen Swift callers:** none
- **Safe to delete because:** All theme-resolution behaviours are ported to Rust with
  passing tests (29 provider_theme tests), and no Swift file references `OpenCodeTheme`.

### 3. HostHardware.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Sources/SupercliNative/HostHardware.swift`
- **LOC:** 70
- **Ported destination:** Rust `supercli-core::host_hardware` (`crates/supercli-core/src/host_hardware.rs`)
- **Behaviours → tests (verified present in `host_hardware.rs`):**
  - `HostHardware.resolve()` model mapping → `tests::mac_studio_ids`, `macbook_family_prefix`, `mac_mini_prefix`, `unrecognized_board_reports_raw_id`
  - `modelIdentifier()` sysctl read → `tests::device_kind_and_model_do_not_panic`
- **Frozen Swift callers:** none
- **Safe to delete because:** Hardware identification is ported to Rust with passing
  tests, and no Swift file references `HostHardware`.

## Blocked — cannot delete yet

These files were on the reviewer's candidate list but are **not** included in the
`pending` ledger rows for the reasons below.

### 4. PaneLayoutState.swift — BLOCKED (Swift callers)

- **LOC:** 1,394 · **Status on next:** ported → Rust `supercli-client` (pane_layout)
- **Swift callers (9):**
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/PaneLayoutController.swift`
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteDTOAdapters.swift`
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/UnpeelStore.swift`
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/Views/SidebarWorkspaceDots.swift`
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/Views/TerminalPaneView.swift`
  - `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/MobilePaneGroupProjectionTests.swift`
  - `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/PaneLayoutOperationsConformanceTests.swift`
  - `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/PaneLayoutStateTests.swift`
  - `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/SidebarPaneProjectionTests.swift`
- **Action required:** Callers must be ported or deleted before `PaneLayoutState.swift`
  can be deleted. `UnpeelStore.swift` and `TerminalPaneView.swift` are in active porting.

### 5. RemoteTerminalWebSocket.swift — BLOCKED (Swift callers)

- **LOC:** 304 · **Status on next:** ported → Rust `supercli-client`
- **Swift callers (2):**
  - `clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/RemoteGhosttyTerminalView.swift` (gpuidart gap — iOS UI)
  - `clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalStreamTransport.swift`
- **Action required:** iOS callers are blocked on gpuidart mobile. Cannot delete until
  the iOS tree is resolved.

### 6. StartupPresentationCache.swift — BLOCKED (Swift callers)

- **LOC:** 50 · **Status on next:** ported → Rust `supercli-client::startup_presentation_cache`
- **Swift callers (2):**
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/UnpeelStore.swift`
  - `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/StartupPerformanceTests.swift`
- **Action required:** `UnpeelStore.swift` is in active porting; the test file needs
  a superseded-by-Rust disposition.

### 7. PresetStateFile.swift — BLOCKED (Swift callers)

- **LOC:** 142 · **Status on next:** ported → Rust `supercli-core::preset_state_file`
- **Swift callers (3):**
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/PaneLayoutController.swift`
  - `clients/legacy/native/SupercliNative/Sources/SupercliNative/UnpeelStore.swift`
  - `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/PresetStateFileTests.swift`
- **Action required:** Callers in active porting files must be resolved first.

### 8. NativeRemoteBackend.swift — BLOCKED (not yet ported on next)

- **LOC:** 2,195 · **Status on next:** `todo`
- **Note:** Port branch `feat/port-nativeremotebackend@009a008` is reviewer-approved
  ("OK to merge after the map regen") but has **not yet merged** into `next`.
  The ledger row cannot be marked `pending` until the row is `ported` on `next`.

### 9. PaneLayoutStateTests.swift — BLOCKED (not yet ported on next)

- **LOC:** 253 · **Status on next:** `todo`
- **Note:** Orphan test file with no ledger row. Per the batch7 recon, the Rust
  `pane_layout` module runs the same fixture in `cargo test`, but the Swift test
  file itself has not been dispositioned. Needs a `dropped:` or `ported:` row
  before it can be deleted.

## Summary

| File | LOC | Disposition |
|------|-----|-------------|
| RemoteUnpeelClient.swift | 172 | **pending** — ready |
| OpenCodeTheme.swift | 1,093 | **pending** — ready |
| HostHardware.swift | 70 | **pending** — ready |
| PaneLayoutState.swift | 1,394 | blocked — 9 Swift callers |
| RemoteTerminalWebSocket.swift | 304 | blocked — 2 Swift callers (iOS) |
| StartupPresentationCache.swift | 50 | blocked — 2 Swift callers |
| PresetStateFile.swift | 142 | blocked — 3 Swift callers |
| NativeRemoteBackend.swift | 2,195 | blocked — port not yet merged |
| PaneLayoutStateTests.swift | 253 | blocked — not ported |
| **Batch 7 pending total** | **1,335** | 3 files |

After this batch merges (pending → merged), deleted goes from 18,961 to **20,296 LOC**
(12.1% of the 167,628 baseline).
