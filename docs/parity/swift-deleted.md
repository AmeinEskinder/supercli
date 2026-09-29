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

## Totals

- Merged into `next`: **10,819 LOC** (17 files) — 6.5% of baseline
- Pending merge: **4,072 LOC** (13 files, feat/verify-delete-batch6)
- All verified deletions: **14,891 LOC** (30 files) — 8.9% of baseline
| clients/legacy/ios/SupercliIOS/Sources/SupercliIOS/DevSettings.swift | 29 | merged (feat/verify-delete-batch4) | merged | supercli-client/src/dev_settings.rs::DevSettings — toggle state + persistence key + UserDefaults abstraction | 4 Rust tests pass (defaults_to_off, toggle_persists_to_store, toggle_off_clears_persisted_value, uses_stable_defaults_key) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ProviderThemeReadRequest.swift | 30 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/provider_theme_request.rs::ProviderThemeReadRequest — read() + matches(), tail-only output.bin sampling | 7 Rust tests pass (incl. read_samples_only_the_tail_for_large_output_bin) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ClickablePath.swift | 209 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/clickable_path.rs — match_in_row, resolve_file, absolute_path, file_url_match | Rust tests pass (sidecar: 6 behaviours mapped) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ClickablePathTests.swift | 82 | merged (feat/verify-delete-batch4) | merged | superseded by Rust tests in supercli-core/src/clickable_path.rs | (same as above) |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ActivityLog.swift | 209 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/activity_log.rs — ActivityLogEntry, refresh_from_host, append, append_collapsing, compact | Rust tests pass (sidecar: 4 behaviours mapped) |
| clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ActivityLogHostConsumerTests.swift | 43 | merged (feat/verify-delete-batch4) | merged | superseded by Rust test in supercli-core/src/activity_log.rs | 1 Rust test: refresh_from_host picks up external appends without writing |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/FeatureFlags.swift | 273 | merged (feat/verify-delete-batch4) | merged | supercli-core/src/feature_flags.rs::AppFeature — single implementation after dedup (feat/port-feature-flags@6ebd362) | 26 Rust tests pass |
| clients/legacy/native/SupercliNative/Sources/SupercliNative/ModuleResources.swift | 32 | merged (feat/verify-delete-batch4) | merged | supercli-native-bridge/src/macos/app_shell.rs — resource bundle lookup order (Contents/Resources/ then next-to-binary) | 2 Rust tests pass (resource_lookup_prefers_packaged_app_dir, resource_lookup_bare_executable_has_single_candidate) |
| clients/legacy/app-kit/swift/Sources/SupercliAppKitUI/UIUnixSessionClient.swift | 513 | (feat/verify-delete-batch6) | pending | Dart: clients/supercli-app/lib/host_client.dart (HostClient HTTP) — deliberate architecture decision, GAP-APPKIT-1, Unix-socket transport not ported | N/A — transport-specific reconnect/frame-decode/event-dispatch has no Dart equivalent |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Package.swift | 59 | (feat/verify-delete-batch6) | pending | SwiftPM manifest, demo-only — no Dart equivalent | N/A — build file |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/AppFixtures.swift | 188 | (feat/verify-delete-batch6) | pending | demo-only test fixtures — no Dart product equivalent (Dart appkit widget tests use inline fixtures) | N/A — demo fixtures |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/ComponentTreeView.swift | 177 | (feat/verify-delete-batch6) | pending | demo-only SwiftUI view — no Dart product equivalent | N/A — demo view |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/ContentView.swift | 610 | (feat/verify-delete-batch6) | pending | demo-only SwiftUI view — no Dart product equivalent | N/A — demo view |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/KitchenSinkApp.swift | 44 | (feat/verify-delete-batch6) | pending | demo-only app entry — no Dart equivalent | N/A — demo entry |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/MiniHost.swift | 889 | (feat/verify-delete-batch6) | pending | demo-only test-double Host for SwiftUI previews — no Dart product equivalent | N/A — demo test double |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/SemanticWalkthrough.swift | 266 | (feat/verify-delete-batch6) | pending | demo-only walkthrough — no Dart product equivalent | N/A — demo walkthrough |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/SurfaceMiniHost.swift | 857 | (feat/verify-delete-batch6) | pending | demo-only test-double Host (Surface variant) — no Dart product equivalent | N/A — demo test double |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/TerminalPane.swift | 273 | (feat/verify-delete-batch6) | pending | demo-only terminal pane — no Dart product equivalent | N/A — demo view |
| clients/legacy/app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/WebComponentPane.swift | 151 | (feat/verify-delete-batch6) | pending | demo-only web pane — no Dart product equivalent | N/A — demo view |
| clients/legacy/app-kit/swift/Package.swift | 17 | (feat/verify-delete-batch6) | pending | SwiftPM manifest, build file — no Dart equivalent | N/A — build file |
| clients/legacy/shared/SupercliShared/Package.swift | 28 | (feat/verify-delete-batch6) | pending | SwiftPM manifest — no Rust equivalent (Rust uses Cargo) | N/A — build file |
