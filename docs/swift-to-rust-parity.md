# Swift to Rust Parity Inventory

> Generated 2026-09-26. Maps every Swift file under `clients/legacy/` to its Rust/gpuidart destination.
> **Do not edit Swift files.** Legacy stays frozen until parity is proven; deleting it is Amein's call.

## Summary

| Category | Count | Description |
|----------|-------|-------------|
| (a) Business logic | 83 | Port to Rust crates |
| (b) Platform glue | 9 | objc2 in supercli-native-bridge |
| (c) UI views | 64 | gpuidart screens/widgets |
| (d) Drop | 129 | Obsolete/vendored |
| Test files | 103 | Port meaningful cases |
| **Total** | **388** | |

### Business logic status

| Status | Count |
|--------|-------|
| exists | 5 |
| to-port | 73 |
| to-create | 5 |
| needs-review | 0 |

### By group

| Group | Total | (a) | (b) | (c) | (d) | test |
|-------|-------|-----|-----|-----|-----|------|
| SupercliIOS | 45 | 13 | 4 | 12 | 1 | 15 |
| SupercliNative (macOS) | 293 | 62 | 5 | 40 | 113 | 77 |
| SupercliShared | 18 | 8 | 0 | 0 | 1 | 9 |
| app-kit | 29 | 4 | 0 | 12 | 11 | 2 |
| dioxus (legacy) | 3 | 0 | 0 | 0 | 3 | 0 |

## SupercliIOS (45 files)

| Swift File | Cat | Rust Destination | Status | Notes |
|------------|-----|------------------|--------|-------|
| `ios/SupercliIOS/App/UnpeelIOSApp.swift` | b | `supercli-native-bridge (iOS)` | to-create | iOS app lifecycle |
| `ios/SupercliIOS/Package.swift` | d | `-` | drop | SwiftPM manifest; obsolete |
| `ios/SupercliIOS/Sources/SupercliIOS/AppLock.swift` | a | `supercli-core/src/app_lock.rs` | to-port | App lock |
| `ios/SupercliIOS/Sources/SupercliIOS/ArchivedSessionsSheet.swift` | c | `clients/supercli-app/lib/screens/mobile/archivedsessionssheet.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/BrowserGalleryPanel.swift` | c | `clients/supercli-app/lib/screens/mobile/browsergallerypanel.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/DevSettings.swift` | a | `supercli-core/src/dev_settings.rs` | to-port | Dev settings |
| `ios/SupercliIOS/Sources/SupercliIOS/DictationReflection.swift` | b | `supercli-native-bridge (objc2)` | to-create | Dictation |
| `ios/SupercliIOS/Sources/SupercliIOS/ImageAnnotationView.swift` | c | `clients/supercli-app/lib/screens/mobile/imageannotationview.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/MascotView.swift` | c | `clients/supercli-app/lib/screens/mobile/mascotview.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/PairingView.swift` | c | `clients/supercli-app/lib/screens/mobile/pairingview.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/ProjectOrganizeSheet.swift` | c | `clients/supercli-app/lib/screens/mobile/projectorganizesheet.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/PushManager.swift` | b | `supercli-native-bridge (objc2)` | to-create | Push notifications |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteConnectionStore.swift` | a | `supercli-core/src/remote_connection.rs` | to-port | Remote connection store |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteDirectTransport.swift` | a | `supercli-core/src/direct_connection.rs` | to-port | Direct transport |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteGhosttyTerminalView.swift` | c | `clients/supercli-app/lib/screens/mobile/remoteghosttyterminalview.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteMacClient.swift` | a | `supercli-client` | to-port | Reuse supercli-client |
| `ios/SupercliIOS/Sources/SupercliIOS/RemotePreviewStore.swift` | a | `supercli-core/src/preview.rs` | to-port | Preview store |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalPrediction.swift` | a | `supercli-core/src/terminal_predict.rs` | to-port | Terminal prediction |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalScrollPrediction.swift` | a | `supercli-core/src/terminal_predict.rs` | to-port | Scroll prediction |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalStreamTransport.swift` | a | `supercli-core/src/terminal_stream.rs` | to-port | Terminal stream |
| `ios/SupercliIOS/Sources/SupercliIOS/RemoteTerminalWebSocket.swift` | a | `supercli-core/src/terminal_ws.rs` | to-port | Terminal WebSocket |
| `ios/SupercliIOS/Sources/SupercliIOS/SessionOrganizeSheet.swift` | c | `clients/supercli-app/lib/screens/mobile/sessionorganizesheet.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/SharedIconViews.swift` | c | `clients/supercli-app` | to-create | Shared icons (gpuidart) |
| `ios/SupercliIOS/Sources/SupercliIOS/StreamFrameReconciler.swift` | a | `supercli-core/src/stream_reconcile.rs` | to-port | Frame reconciler |
| `ios/SupercliIOS/Sources/SupercliIOS/TerminalDetailView.swift` | c | `clients/supercli-app/lib/screens/mobile/terminaldetailview.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/TerminalQueryFilter.swift` | a | `supercli-core/src/terminal.rs` | to-port | Query filter |
| `ios/SupercliIOS/Sources/SupercliIOS/TerminalSessionCache.swift` | a | `supercli-core/src/session_cache.rs` | to-port | Session cache |
| `ios/SupercliIOS/Sources/SupercliIOS/TerminalTextSelectionSheet.swift` | c | `clients/supercli-app/lib/screens/mobile/terminaltextselectionsheet.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/UnpeelIOSRootView.swift` | c | `clients/supercli-app/lib/screens/mobile/unpeeliosrootview.dart` | to-create | iOS SwiftUI -> gpuidart mobile |
| `ios/SupercliIOS/Sources/SupercliIOS/VoiceDictationController.swift` | b | `supercli-native-bridge (objc2)` | to-create | Voice dictation |
| `ios/SupercliIOS/Tests/SupercliIOSTests/MenuPromptApprovalTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/PairedMacStorageTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/PushManagerTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteDirectTransportTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteMouseWheelPreferenceTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemotePreviewStoreTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteTerminalCanvasLayoutTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteTerminalMouseModeTrackerTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteTerminalPredictionEngineTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteTerminalScrollPredictionEngineTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/RemoteTerminalStreamTransportTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/ResumableArtifactUploaderTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/SessionLRUIndexTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/StreamFrameReconcilerTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |
| `ios/SupercliIOS/Tests/SupercliIOSTests/TerminalQueryStripTests.swift` | test | `N/A (iOS-specific; manual)` | to-port | Port meaningful cases |

## SupercliNative (macOS) (293 files)

| Swift File | Cat | Rust Destination | Status | Notes |
|------------|-----|------------------|--------|-------|
| `native/SupercliNative/Package.swift` | d | `-` | drop | SwiftPM manifest; obsolete |
| `native/SupercliNative/Sources/SupercliNative/ActivityLog.swift` | a | `supercli-core/src/activity_log.rs` | to-port | Activity log |
| `native/SupercliNative/Sources/SupercliNative/AppDelegate.swift` | b | `supercli-native-bridge (objc2)` | to-create | App lifecycle (NSApplicationDelegate) |
| `native/SupercliNative/Sources/SupercliNative/ChromeIcons.swift` | a | `supercli-core/src/icons.rs` | to-port | Chrome icons |
| `native/SupercliNative/Sources/SupercliNative/ClickablePath.swift` | c | `clients/supercli-app` | to-create | Clickable path (gpuidart widget) |
| `native/SupercliNative/Sources/SupercliNative/ControllerPairingProxy.swift` | a | `supercli-client/src/pairing.rs` | to-port | Pairing proxy |
| `native/SupercliNative/Sources/SupercliNative/DesktopNotifier.swift` | b | `supercli-native-bridge (objc2)` | to-create | User notifications |
| `native/SupercliNative/Sources/SupercliNative/FeatureFlags.swift` | a | `supercli-core/src/feature_flags.rs` | to-port | Feature flags |
| `native/SupercliNative/Sources/SupercliNative/GhosttyBridge.swift` | a | `supercli-core/src/ghostty_vt.rs` | to-port | Reuse ghostty-vt (do not duplicate) |
| `native/SupercliNative/Sources/SupercliNative/GlobalActivityMenu.swift` | c | `clients/supercli-app` | to-create | Activity menu (gpuidart) |
| `native/SupercliNative/Sources/SupercliNative/HookServer.swift` | a | `supercli-core/src/hook_server.rs` | to-port | Hook server |
| `native/SupercliNative/Sources/SupercliNative/HostHardware.swift` | a | `supercli-native-bridge/src/hardware.rs` | to-port | Hardware info via objc2 |
| `native/SupercliNative/Sources/SupercliNative/HostManagementState.swift` | a | `supercli-core/src/host_management.rs` | to-port | Host management state |
| `native/SupercliNative/Sources/SupercliNative/HostServiceAgent.swift` | a | `supercli-host/src/agent.rs` | to-port | Host agent |
| `native/SupercliNative/Sources/SupercliNative/HostServiceIdentity.swift` | a | `supercli-core/src/host_identity.rs` | to-port | Host identity |
| `native/SupercliNative/Sources/SupercliNative/HostServiceManager.swift` | a | `supercli-host/src/service.rs` | to-port | Host service lifecycle |
| `native/SupercliNative/Sources/SupercliNative/LaunchConfig.swift` | a | `supercli-core/src/launch_config.rs` | to-port | Launch config |
| `native/SupercliNative/Sources/SupercliNative/Licensing/LicenseKeychain.swift` | b | `supercli-native-bridge (objc2)` | to-create | Keychain (license) |
| `native/SupercliNative/Sources/SupercliNative/Licensing/LicenseManager.swift` | a | `supercli-core/src/license.rs` | to-port | License manager |
| `native/SupercliNative/Sources/SupercliNative/LocalHostClientFeature.swift` | a | `supercli-core/src/local_host.rs` | to-port | Local host feature |
| `native/SupercliNative/Sources/SupercliNative/LocalHostControl.swift` | a | `supercli-core/src/local_host.rs` | to-port | Local host control |
| `native/SupercliNative/Sources/SupercliNative/MCPApprovalCenter.swift` | a | `supercli-core/src/approval_center.rs` | to-port | MCP approval center |
| `native/SupercliNative/Sources/SupercliNative/MCPApprovalPanel.swift` | c | `clients/supercli-app` | to-create | Approval screen (gpuidart) |
| `native/SupercliNative/Sources/SupercliNative/MenuBarController.swift` | b | `supercli-native-bridge (objc2)` | to-create | Menu bar extra |
| `native/SupercliNative/Sources/SupercliNative/MobilePairingStore.swift` | a | `supercli-core/src/mobile_pairing.rs` | to-port | Mobile pairing store |
| `native/SupercliNative/Sources/SupercliNative/MobileSessionControl.swift` | a | `supercli-core/src/mobile_session.rs` | to-port | Mobile session control |
| `native/SupercliNative/Sources/SupercliNative/Models.swift` | a | `supercli-core/src/models.rs` | to-port | Data models |
| `native/SupercliNative/Sources/SupercliNative/ModuleResources.swift` | a | `supercli-core/src/app_resources.rs` | to-port | Resource bundle lookup |
| `native/SupercliNative/Sources/SupercliNative/NativeControllerRouter.swift` | a | `supercli-core/src/controller_api.rs` | to-port | Controller routing |
| `native/SupercliNative/Sources/SupercliNative/NativeOverlaySnapshotAdapter.swift` | a | `supercli-core/src/snapshot.rs` | to-port | Snapshots |
| `native/SupercliNative/Sources/SupercliNative/NativeRelayBridge.swift` | a | `supercli-core/src/relay_connection.rs` | to-port | Relay bridge |
| `native/SupercliNative/Sources/SupercliNative/NativeRemoteBackend.swift` | a | `supercli-core/src/remote_backend.rs` | to-port | Remote backend adapter |
| `native/SupercliNative/Sources/SupercliNative/NearbyHostBrowser.swift` | a | `supercli-core/src/nearby_hosts.rs` | to-port | mDNS host browser |
| `native/SupercliNative/Sources/SupercliNative/OpenCodeTheme.swift` | a | `supercli-core/src/theme.rs` | to-port | Theme |
| `native/SupercliNative/Sources/SupercliNative/PaneLayoutController.swift` | a | `supercli-core/src/pane_layout.rs` | to-port | Pane layout controller |
| `native/SupercliNative/Sources/SupercliNative/PaneLayoutState.swift` | a | `supercli-core/src/pane_layout.rs` | to-port | Pane layout state |
| `native/SupercliNative/Sources/SupercliNative/PluginSettingsList.swift` | a | `supercli-core/src/plugin_settings.rs` | to-port | Plugin settings |
| `native/SupercliNative/Sources/SupercliNative/PresetStateFile.swift` | a | `supercli-core/src/presets.rs` | to-port | Preset state file |
| `native/SupercliNative/Sources/SupercliNative/Presets.swift` | a | `supercli-core/src/presets.rs` | to-port | Presets |
| `native/SupercliNative/Sources/SupercliNative/ProjectWorkspaceMove.swift` | a | `supercli-core/src/workspace.rs` | to-port | Workspace move |
| `native/SupercliNative/Sources/SupercliNative/ProviderCapabilities.swift` | a | `supercli-core/src/provider.rs` | to-port | Provider capabilities |
| `native/SupercliNative/Sources/SupercliNative/ProviderThemeReadRequest.swift` | a | `supercli-core/src/theme.rs` | to-port | Theme read request |
| `native/SupercliNative/Sources/SupercliNative/RelayUplinkManager.swift` | a | `supercli-core/src/relay_uplink.rs` | to-port | Relay uplink manager |
| `native/SupercliNative/Sources/SupercliNative/RemoteDTOAdapters.swift` | a | `supercli-core/src/remote_dto.rs` | to-port | DTO adapters |
| `native/SupercliNative/Sources/SupercliNative/RemoteHostRuntime.swift` | a | `supercli-core/src/remote_host.rs` | to-port | Remote host runtime |
| `native/SupercliNative/Sources/SupercliNative/RemoteHosts.swift` | a | `supercli-core/src/remote_hosts.rs` | to-port | Remote hosts list |
| `native/SupercliNative/Sources/SupercliNative/RemoteUnpeelClient.swift` | a | `supercli-client` | to-port | Reuse supercli-client (do not duplicate) |
| `native/SupercliNative/Sources/SupercliNative/ResumeCommand.swift` | a | `supercli-core/src/resume.rs` | to-port | Resume command |
| `native/SupercliNative/Sources/SupercliNative/SelectedHostScope.swift` | a | `supercli-core/src/host_scope.rs` | to-port | Host scope |
| `native/SupercliNative/Sources/SupercliNative/SessionActivity.swift` | a | `supercli-core/src/session_activity.rs` | to-port | Session activity |
| `native/SupercliNative/Sources/SupercliNative/SessionArtifacts.swift` | a | `supercli-core/src/session_artifacts.rs` | to-port | Session artifacts |
| `native/SupercliNative/Sources/SupercliNative/SessionMoveRules.swift` | a | `supercli-core/src/session.rs` | to-port | Session move rules |
| `native/SupercliNative/Sources/SupercliNative/SidebarProjectionChanges.swift` | a | `supercli-core/src/sidebar.rs` | to-port | Sidebar projection |
| `native/SupercliNative/Sources/SupercliNative/Snapshot.swift` | a | `supercli-core/src/snapshot.rs` | to-port | Snapshots |
| `native/SupercliNative/Sources/SupercliNative/StartupPresentationCache.swift` | a | `supercli-core/src/startup_cache.rs` | to-port | Startup cache |
| `native/SupercliNative/Sources/SupercliNative/SurfaceCache.swift` | a | `supercli-core/src/surface_cache.rs` | to-port | Surface cache |
| `native/SupercliNative/Sources/SupercliNative/TerminalDropTargetMap.swift` | a | `supercli-core/src/terminal.rs` | to-port | Terminal drop targets |
| `native/SupercliNative/Sources/SupercliNative/TerminalFindBar.swift` | c | `clients/supercli-app` | to-create | Find bar (gpuidart widget) |
| `native/SupercliNative/Sources/SupercliNative/TerminalPaneWindow.swift` | c | `clients/supercli-app` | to-create | Terminal pane window (gpuidart) |
| `native/SupercliNative/Sources/SupercliNative/TerminalPathDragMap.swift` | a | `supercli-core/src/terminal.rs` | to-port | Terminal path drag |
| `native/SupercliNative/Sources/SupercliNative/Theme.swift` | a | `supercli-core/src/theme.rs` | to-port | Theme |
| `native/SupercliNative/Sources/SupercliNative/ToolIcons.swift` | a | `supercli-core/src/icons.rs` | to-port | Tool icons |
| `native/SupercliNative/Sources/SupercliNative/UnpeelStore.swift` | a | `supercli-core/src/app_state.rs` | to-port | App state store |
| `native/SupercliNative/Sources/SupercliNative/UnpeelWorkspaceRegistry.swift` | a | `supercli-core/src/workspace_registry.rs` | to-port | Workspace registry |
| `native/SupercliNative/Sources/SupercliNative/ViewerPresence.swift` | a | `supercli-core/src/presence.rs` | to-port | Viewer presence |
| `native/SupercliNative/Sources/SupercliNative/Views/AgentAccessSettingsPanel.swift` | c | `clients/supercli-app/lib/screens/agentaccesssettingspanel.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/ArchivedSessionsView.swift` | c | `clients/supercli-app/lib/screens/archivedsessionsview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/BrowserAccessSections.swift` | c | `clients/supercli-app/lib/screens/browseraccesssections.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/Chrome.swift` | c | `clients/supercli-app/lib/screens/chrome.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/CommandPaletteView.swift` | c | `clients/supercli-app/lib/screens/commandpaletteview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/HostPickerView.swift` | c | `clients/supercli-app/lib/screens/hostpickerview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/LicenseSettingsPanel.swift` | c | `clients/supercli-app/lib/screens/licensesettingspanel.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/LocalSiteMenu.swift` | c | `clients/supercli-app/lib/screens/localsitemenu.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/PluginListDrag.swift` | c | `clients/supercli-app/lib/screens/pluginlistdrag.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/PluginSettingsPanel.swift` | c | `clients/supercli-app/lib/screens/pluginsettingspanel.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/PresetsSettingsPanel.swift` | c | `clients/supercli-app/lib/screens/presetssettingspanel.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/ProjectSidebarView.swift` | c | `clients/supercli-app/lib/screens/projectsidebarview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/RecentActivityView.swift` | c | `clients/supercli-app/lib/screens/recentactivityview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/RemoteFolderPicker.swift` | c | `clients/supercli-app/lib/screens/remotefolderpicker.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/RemoteHostWorkspaceView.swift` | c | `clients/supercli-app/lib/screens/remotehostworkspaceview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/RootView.swift` | c | `clients/supercli-app/lib/screens/rootview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SessionGalleryMarkup.swift` | c | `clients/supercli-app/lib/screens/sessiongallerymarkup.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SessionGalleryPanel.swift` | c | `clients/supercli-app/lib/screens/sessiongallerypanel.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SessionLauncherView.swift` | c | `clients/supercli-app/lib/screens/sessionlauncherview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SessionScreenshotCapture.swift` | c | `clients/supercli-app/lib/screens/sessionscreenshotcapture.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SessionsAccessSections.swift` | c | `clients/supercli-app/lib/screens/sessionsaccesssections.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SettingsView.swift` | c | `clients/supercli-app/lib/screens/settingsview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarSessionDrag.swift` | c | `clients/supercli-app/lib/screens/sidebarsessiondrag.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarSkeleton.swift` | c | `clients/supercli-app/lib/screens/sidebarskeleton.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarSpinners.swift` | c | `clients/supercli-app/lib/screens/sidebarspinners.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarView.swift` | c | `clients/supercli-app/lib/screens/sidebarview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarWorkspaceDots.swift` | c | `clients/supercli-app/lib/screens/sidebarworkspacedots.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/SidebarWorkspaceSelector.swift` | c | `clients/supercli-app/lib/screens/sidebarworkspaceselector.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/TerminalArea.swift` | c | `clients/supercli-app/lib/screens/terminalarea.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/TerminalPaneView.swift` | c | `clients/supercli-app/lib/screens/terminalpaneview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/ToastCenter.swift` | c | `clients/supercli-app/lib/screens/toastcenter.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/ViewerAvatarsView.swift` | c | `clients/supercli-app/lib/screens/vieweravatarsview.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/WorkspaceOpenMenu.swift` | c | `clients/supercli-app/lib/screens/workspaceopenmenu.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/WorkspacesSettingsPanel.swift` | c | `clients/supercli-app/lib/screens/workspacessettingspanel.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/Views/WorktreesSettingsPanel.swift` | c | `clients/supercli-app/lib/screens/worktreessettingspanel.dart` | to-create | SwiftUI -> gpuidart screen |
| `native/SupercliNative/Sources/SupercliNative/WorkspaceOpenTarget.swift` | a | `supercli-core/src/workspace.rs` | to-port | Workspace open target |
| `native/SupercliNative/Sources/SupercliNative/WorkspacePool.swift` | a | `supercli-core/src/workspace.rs` | to-port | Workspace pool |
| `native/SupercliNative/Sources/SupercliNative/WorktreeGit.swift` | a | `supercli-core/src/worktree.rs` | to-port | Worktree git ops |
| `native/SupercliNative/Sources/SupercliNative/main.swift` | b | `supercli-native-bridge (objc2)` | to-create | App entry point |
| `native/SupercliNative/Tests/SupercliNativeTests/ActivityLogHostConsumerTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ActivityMenuSessionsTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/AgentWorktreeAdoptionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/AppStateFileTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ClickablePathTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ComputerContainmentTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ControllerPairingProxyTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/GhosttySurfaceKeybindTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/GlobalActivityMenuProjectionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/HookServerParsingTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/HostServiceAgentTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/HostServiceIdentityTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/HostServiceManagerTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/HostedSessionManifestTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/LaunchConfigAttachCommandTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/LicenseManagerTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/LocalHostClientFeatureTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/MCPApprovalPresentationTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/MobilePairingStoreTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/MobilePaneGroupProjectionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/MobileSessionControlTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/NativeControllerRouterTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/NativeOverlaySnapshotAdapterTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/NativeRelayBridgeTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/NativeRemoteBackendTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/NearbyHostBrowserTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/NotificationDeliveryTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/OpenCodeThemeTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/OpenURLSanitizerTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PaneLayoutControllerTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PaneLayoutOperationsConformanceTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PaneLayoutStateTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PaneWorkingDirectoryTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PhoneFitProjectionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PlatformAdapterCallbackTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PluginListDragTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PluginSettingsListTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PresetStateFileTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/PresetsTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ProjectWorkspaceMoveTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ProviderCapabilitiesTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ProviderThemeRefreshTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RelayUplinkManagerTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RemoteContentBannerPolicyTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RemoteDTOAdaptersTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RemoteGhosttyPaneRetentionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RemoteHostRuntimeTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RemoteHostStoreTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RemoteResumePlacementTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/RestartGhostTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ResumeCommandTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SelectedHostScopeTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SessionActivityTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SessionMoveRulesTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SessionSelectionPerformanceTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SessionTitleDefaultsTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SharedOrganizationReconciliationTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SidebarGroupDropTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SidebarLocalPageSourceTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SidebarPagerMathTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SidebarPaneProjectionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SidebarSessionDragLiftTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SidebarSessionOrderTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/StartupPerformanceTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/SurfaceCacheEvictionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalDropTargetMapTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalFontModelTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalLinkRegressionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalPaneClosePolicyTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalPaneDropTargetTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalPaneWindowTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/TerminalPathDragMapTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ThemeColorTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/UnpeelWorkspaceRegistryTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/ViewerPresenceTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/WorkspacePoolTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/SupercliNative/Tests/SupercliNativeTests/WorktreeGitTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `native/dmg-background.swift` | d | `-` | drop | DMG build artifact |
| `native/vendor/libghostty-spm/Package.local.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Package.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyKit/GhosttyKit.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Configuration/GhosttyConfigRenderer.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Configuration/TerminalColorScheme+SwiftUI.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Configuration/TerminalColorScheme.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Configuration/TerminalConfiguration.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Configuration/TerminalTheme+Defaults.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Configuration/TerminalTheme.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Controller/TerminalCallbackLifetime.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Controller/TerminalController+Callbacks.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Controller/TerminalController+Config.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Controller/TerminalController+Surface.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Controller/TerminalController.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Debug/TerminalDebugLog.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/InMemory/InMemoryTerminalSession.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/InMemory/InMemoryTerminalViewport.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/InMemory/TerminalCallbackBridge.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/InMemory/TerminalSessionBackend.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Metrics/TerminalGridMetrics.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Metrics/TerminalInputModifiers.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Metrics/TerminalScrollModifiers.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Metrics/TerminalViewportMetrics.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/AppTerminalView+Input.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/AppTerminalView+Lifecycle.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/AppTerminalView+NSTextInputClient.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/AppTerminalView+PublicInput.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/AppTerminalView.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/KeyboardLayout.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/TerminalKeyEventHandler@AppKit.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/AppKit/TerminalTextInputHandler@AppKit.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/Shared/TerminalCommittedTextRouter.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/Shared/TerminalHardwareKeyRouter.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/Shared/TerminalInputText.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/Shared/TerminalMainActor.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/Shared/TerminalMarkedTextState.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/TerminalInputAccessoryStyle.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/TerminalInputAccessoryView.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/TerminalInputBarKey.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/TerminalStickyModifierState.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/TerminalTextInputHandler@UIKit.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/TerminalTextPosition.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView+InputAccessory.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView+Interaction.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView+Keyboard.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView+Lifecycle.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView+PinchZoom.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView+PublicSticky.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView+UITextInput.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Platform/UIKit/UITerminalView.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/State/TerminalViewState+Delegate.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/State/TerminalViewState+Mutation.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/State/TerminalViewState.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Surface/TerminalSelectionAnchor.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Surface/TerminalSurface.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Surface/TerminalSurfaceContext.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Surface/TerminalSurfaceCoordinator.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Surface/TerminalSurfaceOptions.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Surface/TerminalSurfaceView.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/Surface/TerminalSurfaceViewDelegate.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/View/TerminalView.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/View/TerminalViewRepresentable.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/View/TerminalViewRepresentable@AppKit.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTerminal/View/TerminalViewRepresentable@UIKit.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/GhosttyThemeCatalog.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/GhosttyThemeDefinition+TerminalConfiguration.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/GhosttyThemeDefinition.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/ThemeCatalog_Generated.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_A.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_B.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_C.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_D.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_E.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_F.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_G.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_H.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_I.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_J.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_K.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_L.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_M.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_N.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_O.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_P.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_R.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_S.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_Symbols.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_T.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_U.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_V.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_W.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_X.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/GhosttyTheme/Themes/Themes_Z.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/ShellCraftKit/Definition/SandboxShell.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/ShellCraftKit/Definition/ShellCommand.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/ShellCraftKit/Definition/ShellDefinition.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/ShellCraftKit/Session/ShellSession+Bridge.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/ShellCraftKit/Session/ShellSession+Engine.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Sources/ShellCraftKit/Session/ShellSession.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/InMemoryTerminalSessionHostBytesTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/InMemoryTerminalSessionViewportTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/ShellCraftKitTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalDebugLogTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalHardwareKeyRouterTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalInputTextTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalLifecycleTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalMarkedTextStateTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalSelectionAnchorTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalSurfaceViewFocusAPITests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/TerminalThemeConfigurationTests.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |
| `native/vendor/libghostty-spm/Tests/GhosttyKitTest/Test.swift` | d | `-` | drop | Vendored libghostty-spm; use ghostty-vt in supercli-core |

## SupercliShared (18 files)

| Swift File | Cat | Rust Destination | Status | Notes |
|------------|-----|------------------|--------|-------|
| `shared/SupercliShared/Package.swift` | d | `-` | drop | SwiftPM manifest; obsolete |
| `shared/SupercliShared/Sources/SupercliShared/ChromeIcons.swift` | a | `supercli-core/src/icons.rs` | to-port | Chrome icons |
| `shared/SupercliShared/Sources/SupercliShared/GeneratedRuntimeCatalog.swift` | a | `supercli-core/src/runtime_catalog.rs` | to-create | Amein: Rust codegen from runtime.toml |
| `shared/SupercliShared/Sources/SupercliShared/PairedHostRecord.swift` | a | `supercli-core/src/paired_hosts.rs` | exists | Verify coverage |
| `shared/SupercliShared/Sources/SupercliShared/RelayProtocol.swift` | a | `supercli-core/src/relay_protocol.rs` | exists | Verify coverage |
| `shared/SupercliShared/Sources/SupercliShared/RemoteControlProtocol.swift` | a | `supercli-core/src/controller_protocol.rs` | exists | Verify coverage |
| `shared/SupercliShared/Sources/SupercliShared/RemotePairingClient.swift` | a | `supercli-client/src/pairing.rs` | exists | Reuse supercli-client |
| `shared/SupercliShared/Sources/SupercliShared/RemoteRelayConnection.swift` | a | `supercli-core/src/relay_connection.rs` | exists | Verify coverage |
| `shared/SupercliShared/Sources/SupercliShared/ToolIcons.swift` | a | `supercli-core/src/icons.rs` | to-port | Tool icons |
| `shared/SupercliShared/Tests/SupercliSharedTests/PairedHostRecordTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/PluginProtocolTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/RelayCryptoVectorTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/RelayProtocolTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/RemoteControlProtocolTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/RemotePairingClientTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/RemoteRelayConnectionTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/RemoteTransportContractTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |
| `shared/SupercliShared/Tests/SupercliSharedTests/RuntimeCatalogTests.swift` | test | `supercli-core/tests/ (port)` | to-port | Port meaningful cases |

## app-kit (29 files)

| Swift File | Cat | Rust Destination | Status | Notes |
|------------|-----|------------------|--------|-------|
| `app-kit/swift/Examples/KitchenSink/Package.swift` | d | `-` | drop | SwiftPM manifest; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/AppFixtures.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/ComponentTreeView.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/ContentView.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/KitchenSinkApp.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/MiniHost.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/SemanticWalkthrough.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/SurfaceMiniHost.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/TerminalPane.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Examples/KitchenSink/Sources/KitchenSink/WebComponentPane.swift` | d | `-` | drop | Demo app; obsolete |
| `app-kit/swift/Package.swift` | d | `-` | drop | SwiftPM manifest; obsolete |
| `app-kit/swift/Sources/SupercliAppKitUI/CanvasPageView.swift` | c | `clients/supercli-app/lib/widgets/canvaspageview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/FooterActionsView.swift` | c | `clients/supercli-app/lib/widgets/footeractionsview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/ListNavigation.swift` | c | `clients/supercli-app/lib/widgets/list_navigation.dart` | to-create | List navigation widget |
| `app-kit/swift/Sources/SupercliAppKitUI/MarkdownEditorView.swift` | c | `clients/supercli-app/lib/widgets/markdowneditorview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/MarkdownInsertMenu.swift` | c | `clients/supercli-app/lib/widgets/markdowninsertmenu.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/MediaView.swift` | c | `clients/supercli-app/lib/widgets/mediaview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/PageView.swift` | c | `clients/supercli-app/lib/widgets/pageview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/ReadOnlyContentView.swift` | c | `clients/supercli-app/lib/widgets/readonlycontentview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/SemanticMenuView.swift` | c | `clients/supercli-app/lib/widgets/semanticmenuview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/SurfaceComponentView.swift` | c | `clients/supercli-app/lib/widgets/surfacecomponentview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/TextBoxView.swift` | c | `clients/supercli-app/lib/widgets/textboxview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/TreeView.swift` | c | `clients/supercli-app/lib/widgets/treeview.dart` | to-create | app-kit UI -> gpuidart widget |
| `app-kit/swift/Sources/SupercliAppKitUI/UIDelta.swift` | a | `supercli-core/src/app_ui_protocol.rs` | to-create | UI protocol; port to Rust |
| `app-kit/swift/Sources/SupercliAppKitUI/UIParticipantToken.swift` | a | `supercli-core/src/app_ui_protocol.rs` | to-create | UI protocol; port to Rust |
| `app-kit/swift/Sources/SupercliAppKitUI/UIProtocol.swift` | a | `supercli-core/src/app_ui_protocol.rs` | to-create | UI protocol; port to Rust |
| `app-kit/swift/Sources/SupercliAppKitUI/UIUnixSessionClient.swift` | a | `supercli-core/src/app_ui_protocol.rs` | to-create | UI protocol; port to Rust |
| `app-kit/swift/Tests/SupercliAppKitUITests/MarkdownInsertMenuTests.swift` | test | `port to Rust #[test]` | to-port | Port meaningful cases |
| `app-kit/swift/Tests/SupercliAppKitUITests/ProtocolTests.swift` | test | `port to Rust #[test]` | to-port | Port meaningful cases |

## dioxus (legacy) (3 files)

| Swift File | Cat | Rust Destination | Status | Notes |
|------------|-----|------------------|--------|-------|
| `dioxus/native-shell/UnpeelPushBridge.swift` | d | `-` | drop | Dioxus legacy bridge; obsolete |
| `dioxus/native-shell/UnpeelReflectBridge.swift` | d | `-` | drop | Dioxus legacy bridge; obsolete |
| `dioxus/native-shell/UnpeelSpeechBridge.swift` | d | `-` | drop | Dioxus legacy bridge; obsolete |

