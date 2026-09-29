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
| clients/legacy/native/SupercliNative/Sources/SupercliNative/Presets.swift | 650 | e76c3d2 (feat/verify-delete-batch5) | pending | supercli-core/src/presets.rs::tool_usage_scanner — filesystem/PATH scanner ported | 16 Rust tests pass (tool_usage_scanner_counts_session_files, etc.) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/WorkspacePool.swift | 879 | e76c3d2 (feat/verify-delete-batch5) | pending | supercli-client/src/workspace_pool.rs — async pool driver with 5 state-machine fixes | 34 Rust tests pass |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/UnpeelWorkspaceRegistry.swift | 422 | e76c3d2 (feat/verify-delete-batch5) | pending | supercli-core/src/workspace_registry.rs — env/order/PID/launcher ported | 13 Rust tests pass |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/TerminalDropTargetMap.swift | 133 | e76c3d2 (feat/verify-delete-batch5) | pending | clients/supercli-app/lib/terminal/terminal_drop_maps.dart — decision logic in Rust FFI | 7 Dart tests (parse-checked; verified by supercli-app CI) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/TerminalPathDragMap.swift | 75 | e76c3d2 (feat/verify-delete-batch5) | pending | clients/supercli-app/lib/terminal/terminal_drop_maps.dart — path drag maps | (same 7 Dart tests) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ViewerPresence.swift | 327 | e76c3d2 (feat/verify-delete-batch5) | pending | supercli-client/src/viewer_presence.rs — has_viewers + GridReassertTracker | 11 Rust tests pass |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/OpenCodeThemeTests.swift | 220 | e76c3d2 (feat/verify-delete-batch5) | pending | supercli-core/src/provider_theme.rs::resolve_frame_background — 3 XCTest cases ported | 29 provider_theme tests pass |

## Totals

- Merged into `next`: **887 LOC** (1 file) — 0.5% of baseline
- Pending merge: **9,025 LOC** (8 files)
- All verified deletions: **9,912 LOC** (9 files) — 5.9% of baseline
