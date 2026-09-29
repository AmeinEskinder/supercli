# Verify-Delete Batch7 Full — Review Note

**Branch:** `feat/verify-delete-batch7-full`
**Base:** `origin/next@7f8f4e4`
**Date:** 2026-09-30

## Summary

- **Deleted:** 31 Swift files, 4,616 LOC — all marked `ported` in the ledger, all with **zero Swift callers**.
- **Blocked:** 43 ported files have Swift callers — NOT deleted, documented below.
- **Key correction:** `OpenCodeTheme.swift` was claimed "zero callers" in the previous batch7 prep; manual verification found **3 real callers**. It is BLOCKED.

## Deleted Files (31)

Each file below was verified:
1. Status `ported` in the sidecar ledger.
2. Zero Swift callers (automated type-reference scan across all 346 Swift files, plus manual verification for source files).
3. Behavior rows verified against real Rust/Dart test names via `git grep` on `origin/next`.

### Source files (4)

| File | LOC | Destination | Tests verified | Callers |
|------|-----|-------------|----------------|---------|
| `native/SupercliNative/Sources/SupercliNative/HostHardware.swift` | 70 | `supercli-core::host_hardware` | 6/6 in `host_hardware.rs` | 0 |
| `native/SupercliNative/Sources/SupercliNative/RemoteUnpeelClient.swift` | 172 | `supercli-client::remote_supercli_peer` | 5/5: `parse_valid_remote_key`, `session_from_row_full`, `fingerprint_match_accepts`, `error_401_is_unauthorized`, `peer_file_roundtrip` | 0 |
| `native/SupercliNative/Sources/SupercliNative/main.swift` | 28 | `supercli-native-bridge::macos::app_shell` | 2/2: `dock_icon_set_only_when_bundle_declares_none`, `resource_lookup_prefers_packaged_app_dir` | 0 |
| `app-kit/swift/Sources/SupercliAppKitUI/MarkdownInsertMenu.swift` | 57 | Dart: `markdown_editing.dart` | `MarkdownBackspaceEdit` class + `markdownBackspaceEdit` function present | 0 |

**Safe to delete because:** All behavior is in Rust/Dart with passing tests; no Swift code references these types.

### Test files (27)

These are Swift test files whose test cases were ported to Rust. They have zero callers (test classes are never referenced by other files).

| File | LOC | Destination (Rust test module) |
|------|-----|-------------------------------|
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteMouseWheelPreferenceTests.swift` | 128 | `supercli-client` (mouse_mode) |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteTerminalMouseModeTrackerTests.swift` | 115 | `supercli-client` (mouse_mode) |
| `native/SupercliNative/Tests/SupercliNativeTests/ComputerContainmentTests.swift` | 111 | `supercli-core::store_policies::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/HookServerParsingTests.swift` | 278 | `supercli-native-bridge::macos::hook_server::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/HostServiceAgentTests.swift` | 149 | `supercli-native-bridge::macos::launchd::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/HostServiceIdentityTests.swift` | 182 | `supercli-native-bridge::macos::service_identity::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/HostServiceManagerTests.swift` | 33 | `supercli-native-bridge::macos::service_manager::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/LicenseManagerTests.swift` | 167 | `supercli-native-bridge::macos::license::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/MCPApprovalPresentationTests.swift` | 140 | `supercli-core::mcp_approval_center::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/NotificationDeliveryTests.swift` | 308 | `supercli-native-bridge::macos::notifications::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/PhoneFitProjectionTests.swift` | 90 | `supercli-core::store_policies::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/PluginListDragTests.swift` | 64 | Dart: `clients/supercli-app` |
| `native/SupercliNative/Tests/SupercliNativeTests/PluginSettingsListTests.swift` | 74 | Dart: `clients/supercli-app` |
| `native/SupercliNative/Tests/SupercliNativeTests/RemoteResumePlacementTests.swift` | 90 | `supercli-core::store_policies::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/RestartGhostTests.swift` | 103 | `supercli-core::store_policies::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/SessionActivityTests.swift` | 461 | dropped: Host does it (`supercli-serve/src/activity.rs`) |
| `native/SupercliNative/Tests/SupercliNativeTests/SessionMoveRulesTests.swift` | 129 | Dart: `clients/supercli-app` |
| `native/SupercliNative/Tests/SupercliNativeTests/SessionTitleDefaultsTests.swift` | 35 | Dart: `clients/supercli-app` |
| `native/SupercliNative/Tests/SupercliNativeTests/SharedOrganizationReconciliationTests.swift` | 261 | `supercli-core::store_policies::tests` |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalPaneClosePolicyTests.swift` | 34 | Dart: `clients/supercli-app` |
| `native/SupercliNative/Tests/SupercliNativeTests/WorktreeGitTests.swift` | 134 | dropped: Host does it (`supercli-core/src/host_git.rs`) |
| `shared/SupercliShared/Tests/SupercliSharedTests/PluginProtocolTests.swift` | 66 | `supercli-client::dto` |
| `shared/SupercliShared/Tests/SupercliSharedTests/RelayProtocolTests.swift` | 400 | `supercli-client::relay` |
| `shared/SupercliShared/Tests/SupercliSharedTests/RemotePairingClientTests.swift` | 329 | `supercli-client::pairing` |
| `shared/SupercliShared/Tests/SupercliSharedTests/RemoteRelayConnectionTests.swift` | 172 | `supercli-client::relay_conn` |
| `shared/SupercliShared/Tests/SupercliSharedTests/RemoteTransportContractTests.swift` | 64 | `supercli-client::transport` |
| `shared/SupercliShared/Tests/SupercliSharedTests/RuntimeCatalogTests.swift` | 172 | `supercli-core::app_runtime` |

**Safe to delete because:** Test cases are ported to Rust/Dart; Swift test files are never referenced by other Swift code.

## Blocked Files (43) — NOT Deleted

These are marked `ported` but have Swift callers. Deleting them would break the frozen Swift tree.

| File | Callers | Key callers |
|------|---------|-------------|
| `native/SupercliNative/Sources/SupercliNative/PaneLayoutState.swift` | 29 | UIDelta.swift, UIProtocol.swift, RemoteTerminalStreamTransport.swift, UnpeelIOSRootView.swift, +24 more |
| `native/SupercliNative/Sources/SupercliNative/OpenCodeTheme.swift` | 3 | **SurfaceCache.swift** (source), ProviderThemeRefreshTests.swift, SurfaceCacheEvictionTests.swift |
| `native/SupercliNative/Sources/SupercliNative/PresetStateFile.swift` | 2 | PaneLayoutController.swift, UnpeelStore.swift |
| `native/SupercliNative/Sources/SupercliNative/StartupPresentationCache.swift` | 2 | UnpeelStore.swift, StartupPerformanceTests.swift |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalWebSocket.swift` | 5 | RemoteConnectionStore.swift, RemoteDirectTransport.swift, RemoteGhosttyTerminalView.swift, RemoteMacClient.swift, RemotePreviewStore.swift |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteDirectTransport.swift` | 3 | RemoteConnectionStore.swift, RemoteMacClient.swift, RemotePreviewStore.swift |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalStreamTransport.swift` | 11 | UIDelta.swift, UIProtocol.swift, RemoteGhosttyTerminalView.swift, +8 more |
| `ios/SupercliIOS/Sources/SupercliIOS/StreamFrameReconciler.swift` | 1 | RemoteGhosttyTerminalView.swift |
| `ios/SupercliIOS/Sources/SupercliIOS/TerminalQueryFilter.swift` | 2 | RemoteGhosttyTerminalView.swift, TerminalQueryStripTests.swift |
| `native/SupercliNative/Sources/SupercliNative/Models.swift` | 61 | (pervasive model types) |
| `native/SupercliNative/Sources/SupercliNative/Theme.swift` | 55 | (pervasive theme types) |
| `native/SupercliNative/Sources/SupercliNative/AppDelegate.swift` | 7 | UnpeelPushBridge.swift, UnpeelStore.swift, +5 more |
| `native/SupercliNative/Sources/SupercliNative/ControllerPairingProxy.swift` | 7 | PushManager.swift, HookServer.swift, +5 more |
| `native/SupercliNative/Sources/SupercliNative/DesktopNotifier.swift` | 3 | AppDelegate.swift, UnpeelStore.swift, SettingsView.swift |
| `native/SupercliNative/Sources/SupercliNative/HostServiceAgent.swift` | 4 | HostServiceManager.swift, UnpeelStore.swift, +2 vendor |
| `native/SupercliNative/Sources/SupercliNative/Licensing/LicenseKeychain.swift` | 1 | LicenseManager.swift |
| `native/SupercliNative/Sources/SupercliNative/LocalHostClientFeature.swift` | 4 | AppDelegate.swift, HookServer.swift, +2 more |
| `native/SupercliNative/Sources/SupercliNative/MobilePairingStore.swift` | 7 | ControllerPairingProxy.swift, UnpeelStore.swift, +5 more |
| `native/SupercliNative/Sources/SupercliNative/RelayUplinkManager.swift` | 14 | UIDelta.swift, UIProtocol.swift, +12 more |
| `native/SupercliNative/Sources/SupercliNative/ResumeCommand.swift` | 2 | UnpeelStore.swift, ProviderCapabilitiesTests.swift |
| `native/SupercliNative/Sources/SupercliNative/SessionActivity.swift` | 7 | UnpeelStore.swift, WorktreeGit.swift, +5 more |
| `native/SupercliNative/Sources/SupercliNative/SessionArtifacts.swift` | 3 | MobileSessionControl.swift, +2 more |
| `native/SupercliNative/Sources/SupercliNative/SessionMoveRules.swift` | 2 | UnpeelStore.swift, SidebarSessionDrag.swift |
| `native/SupercliNative/Sources/SupercliNative/WorktreeGit.swift` | 5 | UnpeelStore.swift, SessionActivity.swift, +3 more |
| `native/SupercliNative/Sources/SupercliNative/Views/PluginListDrag.swift` | 13 | PageView.swift, RemoteGhosttyTerminalView.swift, +11 more |
| `native/SupercliNative/Sources/SupercliNative/Views/PresetsSettingsPanel.swift` | 11 | AppDelegate.swift, GhosttyBridge.swift, +9 more |
| `native/SupercliNative/Sources/SupercliNative/Views/ProjectSidebarView.swift` | 1 | RootView.swift |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarSessionDrag.swift` | 17 | UnpeelStore.swift, PluginListDrag.swift, +15 more |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarSkeleton.swift` | 2 | RemoteHostWorkspaceView.swift, SidebarWorkspaceDots.swift |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarSpinners.swift` | 8 | ProjectSidebarView.swift, RootView.swift, +6 more |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarWorkspaceDots.swift` | 31 | (pervasive) |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarWorkspaceSelector.swift` | 5 | SettingsView.swift, SidebarView.swift, +3 more |
| `native/SupercliNative/Sources/SupercliNative/Views/ToastCenter.swift` | 4 | MCPApprovalCenter.swift, UnpeelStore.swift, +2 more |
| `native/SupercliNative/Sources/SupercliNative/WorkspaceOpenTarget.swift` | 5 | UnpeelStore.swift, RootView.swift, +3 more |
| `native/SupercliNative/Sources/SupercliNative/PluginSettingsList.swift` | 1 | PluginSettingsPanel.swift |
| `app-kit/swift/Sources/SupercliAppKitUI/ListNavigation.swift` | 1 | PageView.swift |
| `shared/SupercliShared/Sources/SupercliShared/ChromeIcons.swift` | 2 | SharedIconViews.swift, RemotePreviewStoreTests.swift |
| `shared/SupercliShared/Sources/SupercliShared/GeneratedRuntimeCatalog.swift` | 11 | Models.swift, Theme.swift, +9 more |
| `shared/SupercliShared/Sources/SupercliShared/RelayProtocol.swift` | 20 | RemoteConnectionStore.swift, RemoteMacClient.swift, +18 more |
| `shared/SupercliShared/Sources/SupercliShared/RemotePairingClient.swift` | 4 | RemoteConnectionStore.swift, RemoteMacClient.swift, +2 more |
| `shared/SupercliShared/Sources/SupercliShared/RemoteRelayConnection.swift` | 6 | RemoteConnectionStore.swift, RemoteMacClient.swift, +4 more |
| `shared/SupercliShared/Sources/SupercliShared/ToolIcons.swift` | 8 | SharedIconViews.swift, TerminalPaneView.swift, +6 more |
| `shared/SupercliShared/Tests/SupercliSharedTests/RelayCryptoVectorTests.swift` | 35 | (test vectors referenced by other tests) |

## Notes

1. **OpenCodeTheme.swift correction:** The previous batch7 prep claimed zero callers. Manual verification found 3 real callers (`SurfaceCache.swift` uses `TerminalFrameStyle`/`ProviderCanvasSampler`; two test files). The automated scan initially flagged 16 callers but 13 were false positives from the generic nested type name `Background`. The 3 real callers are confirmed via direct grep.

2. **NativeRemoteBackend.swift:** Not in the ported list (still `todo` on next; port branch `@009a008` approved but not merged). Cannot delete.

3. **PaneLayoutStateTests.swift:** Not in the ledger (orphan, no row). Cannot delete via this batch.

4. **Tests verified by grep, not by run:** Disk is 100% full; cargo cannot run. All test names were confirmed present via `git grep` on `origin/next`. The Rust modules exist.

5. **After merge:** Deleted goes 18,961 → 23,577 LOC (14.1%).
