# Swift deletion ledger

Permanent record of Swift files deleted toward the Swift-0% goal.
Baseline: **167,628 Swift LOC** (the original count before any deletions).

Deletion decisions are Claude's review + Amein's, and nobody else's.

**Maintainer contract:** when a deletion branch merges into `next`, move its rows
from `pending` to `merged` in the Status column and re-run
`scripts/generate-swift-port-map.py`. The generator parses this table
(columns: File | LOC | Deleting commit | Status | Rust/Dart target | Tests)
and sums LOC for rows with Status `merged` into the map headline
`Deleted: X of 167,628 baseline LOC (Y%)`. Do not change the column order.

## Deletions

| File | LOC | Deleting commit | Status | Rust/Dart target | Tests |
| ---- | --- | --------------- | ------ | ---------------- | ----- |
| generated/GeneratedRuntimeCatalog.swift | 887 | 8b1c3a9 (merged via a250aa1) | merged | generated/runtime-catalog.json — JSON catalog (15 runtimes), single source of truth; stale Swift file was shipping in CLI archives | `bun scripts/generate-runtime-client-catalog.mjs --check` PASS; release suite 53/0 |
| clients/legacy/shared/SupercliShared/Sources/SupercliShared/PairedHostRecord.swift | 88 | 5a80d1e6 (feat/verify-delete-batch1) | merged | supercli-client::types::PairedHostRecord | 4 Rust tests pass (paired_host_upsert_is_stable, paired_host_records_roundtrip, client_for_paired_host_routes_by_scheme, paired_host_link_enabled_narrows_only) |
| clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/PairedHostRecordTests.swift | 82 | 5a80d1e6 (feat/verify-delete-batch1) | merged | superseded by Rust tests above | (same 4) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/SelectedHostScope.swift | 101 | a07b6925 (feat/verify-delete-batch2) | merged | supercli-client/src/scope.rs::SelectedHostScope — all 9 behaviours ported | 5 Rust unit tests pass (direct ports of the XCTest) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/SelectedHostScopeTests.swift | 57 | a07b6925 (feat/verify-delete-batch2) | merged | superseded by Rust tests above | (same 5) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/RemoteHostRuntime.swift | 4198 | b65ec68 (feat/verify-delete-batch3) | merged | supercli-client/src/remote_runtime.rs (on feat/port-remote-runtime-v2@a53b0d4) — honest gap: Swift async/MainActor modeled as synchronous deterministic state machine | 89/89 remote_runtime tests pass; 280/280 lib tests pass |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/RemoteHostRuntimeTests.swift | 3510 | b65ec68 (feat/verify-delete-batch3) | merged | superseded by Rust tests above (all 81 XCTest cases ported 1:1) | (same 89) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ProjectWorkspaceMove.swift | 635 | b65ec68 (feat/verify-delete-batch3) | merged | supercli-core/src/workspace_move.rs | 8/8 tests pass on next |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ProjectWorkspaceMoveTests.swift | 354 | b65ec68 (feat/verify-delete-batch3) | merged | superseded by Rust tests above (8 XCTest cases mapped 1:1) | (same 8) |
| clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/DevSettings.swift | 29 | merged (feat/verify-delete-batch4) | merged | supercli-client/src/dev_settings.rs::DevSettings — toggle state + persistence key + UserDefaults abstraction | 4 Rust tests pass (defaults_to_off, toggle_persists_to_store, toggle_off_clears_persisted_value, uses_stable_defaults_key) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ProviderThemeReadRequest.swift | 30 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/provider_theme_request.rs::ProviderThemeReadRequest — read() + matches(), tail-only output.bin sampling | 7 Rust tests pass (incl. read_samples_only_the_tail_for_large_output_bin) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ClickablePath.swift | 209 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/clickable_path.rs — match_in_row, resolve_file, absolute_path, file_url_match | Rust tests pass (sidecar: 6 behaviours mapped) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ClickablePathTests.swift | 82 | merged (feat/verify-delete-batch4) | merged | superseded by Rust tests in supercli-core/src/clickable_path.rs | (same as above) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ActivityLog.swift | 209 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/activity_log.rs — ActivityLogEntry, refresh_from_host, append, append_collapsing, compact | Rust tests pass (sidecar: 4 behaviours mapped) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ActivityLogHostConsumerTests.swift | 43 | merged (feat/verify-delete-batch4) | merged | superseded by Rust test in supercli-core/src/activity_log.rs | 1 Rust test: refresh_from_host picks up external appends without writing |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/FeatureFlags.swift | 273 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/feature_flags.rs::AppFeature — single implementation after dedup (feat/port-feature-flags@6ebd362) | 26 Rust tests pass |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ModuleResources.swift | 32 | merged (feat/verify-delete-batch4) | merged | supercli-native-bridge/src/macos/app_shell.rs — resource bundle lookup order (Contents/Resources/ then next-to-binary) | 2 Rust tests pass (resource_lookup_prefers_packaged_app_dir, resource_lookup_bare_executable_has_single_candidate) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/Presets.swift | 650 | merged (feat/verify-delete-batch5) | merged | supercli-core/src/presets.rs::tool_usage_scanner — filesystem/PATH scanner ported | 16 Rust tests pass (tool_usage_scanner_counts_session_files, etc.) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/WorkspacePool.swift | 879 | merged (feat/verify-delete-batch5) | merged | supercli-client/src/workspace_pool.rs — async pool driver with 5 state-machine fixes | 34 Rust tests pass |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/UnpeelWorkspaceRegistry.swift | 422 | merged (feat/verify-delete-batch5) | merged | supercli-core/src/workspace_registry.rs — env/order/PID/launcher ported | 13 Rust tests pass |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/TerminalDropTargetMap.swift | 133 | merged (feat/verify-delete-batch5) | merged | clients/supercli-app/lib/terminal/terminal_drop_maps.dart — decision logic in Rust FFI | 7 Dart tests (parse-checked; verified by supercli-app CI) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/TerminalPathDragMap.swift | 75 | merged (feat/verify-delete-batch5) | merged | clients/supercli-app/lib/terminal/terminal_drop_maps.dart — path drag maps | (same 7 Dart tests) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ViewerPresence.swift | 327 | merged (feat/verify-delete-batch5) | merged | supercli-client/src/viewer_presence.rs — has_viewers + GridReassertTracker | 11 Rust tests pass |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/OpenCodeThemeTests.swift | 220 | merged (feat/verify-delete-batch5) | merged | supercli-core/src/provider_theme.rs::resolve_frame_background — 3 XCTest cases ported | 29 provider_theme tests pass |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/WorkspacePoolTests.swift | 716 | merged (feat/verify-delete-batch5) | merged | supercli-client/src/workspace_pool.rs — superseded by Rust tests | (same 34) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/PresetsTests.swift | 248 | merged (feat/verify-delete-batch5) | merged | supercli-core/src/presets.rs — superseded by Rust tests | (same 16) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ViewerPresenceTests.swift | 148 | merged (feat/verify-delete-batch5) | merged | supercli-client/src/viewer_presence.rs — superseded by Rust tests | (same 11) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/UnpeelWorkspaceRegistryTests.swift | 117 | merged (feat/verify-delete-batch5) | merged | supercli-core/src/workspace_registry.rs — superseded by Rust tests | (same 13) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/TerminalDropTargetMapTests.swift | 55 | merged (feat/verify-delete-batch5) | merged | supercli-core/src/terminal_drop_maps.rs — superseded by Rust tests | (same 7) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/TerminalPathDragMapTests.swift | 80 | merged (feat/verify-delete-batch5) | merged | supercli-core/src/terminal_drop_maps.rs::PathDragMap::load_from_dir — oversized marker loader case ported via std::fs::metadata gate | 7 terminal_drop_maps tests pass (incl. loader_rejects_oversized_markers) |

## Totals

- Merged into `next`: **14,889 LOC** (30 files) — 8.9% of baseline
- Pending merge: **0 LOC** (0 files)
- All verified deletions: **14,889 LOC** (30 files) — 8.9% of baseline
