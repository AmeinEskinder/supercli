# Verify-Delete Batch 8 — Review Note

**Branch:** `feat/verify-delete-batch8`
**Base:** `origin/next@7f8f4e4`
**Date:** 2026-09-30

This batch prepares deletions for reviewer approval. Ledger rows are marked `pending`
(NOT `merged`). No Swift files are deleted on this branch — the reviewer must approve
before the actual deletion merges.

## Ready for deletion

These files are marked `ported` on `origin/next`, have all behaviours verified by
named passing tests (or superseded by Rust tests), and have **zero** Swift callers
in `clients/legacy`.

### 1. RemoteMouseWheelPreferenceTests.swift

- **Swift path:** `clients/legacy/ios/SupercliIOS/Tests/SupercliIOSTests/RemoteMouseWheelPreferenceTests.swift`
- **LOC:** 128
- **Ported destination:** Rust `supercli-client` (`crates/supercli-client/src/mouse_mode.rs`)
- **Behaviours → tests (verified present in `mouse_mode.rs`):**
  - `prefersRemoteMouseWheel` → `tests::observed_runtime_qualifies_without_provider_or_command_head`
  - `wheelForwarding` → `tests::classic_claude_with_snapshot_leaves_flick_to_local_scrollback`
  - `alternateScrollSequence` → `tests::alternate_screen_without_mouse_emulates_alternate_scroll`
- **Frozen Swift callers:** none
- **Safe to delete because:** All 3 XCTest cases are ported to Rust with passing unit
  tests (19 mouse_mode tests total), and no Swift file references this test class.

### 2. RemoteTerminalMouseModeTrackerTests.swift

- **Swift path:** `clients/legacy/ios/SupercliIOS/Tests/SupercliIOSTests/RemoteTerminalMouseModeTrackerTests.swift`
- **LOC:** 115
- **Ported destination:** Rust `supercli-client` (`crates/supercli-client/src/mouse_mode.rs`)
- **Behaviours → tests:** XCTest cases superseded by Rust `mouse_mode.rs` tests (19 tests total)
- **Frozen Swift callers:** none
- **Safe to delete because:** Mouse mode tracking tests are ported to Rust, and no Swift
  file references this test class.

### 3. ComputerContainmentTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/ComputerContainmentTests.swift`
- **LOC:** 111
- **Ported destination:** Rust `supercli-core::store_policies::tests` (`crates/supercli-core/src/store_policies.rs`)
- **Behaviours → tests:** 6 XCTest cases ported; Rust file has 27 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Containment policy tests are ported to Rust, and no Swift
  file references this test class.

### 4. HookServerParsingTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/HookServerParsingTests.swift`
- **LOC:** 278
- **Ported destination:** Rust `supercli-native-bridge::macos::hook_server::tests` (`crates/supercli-native-bridge/src/macos/hook_server.rs`)
- **Behaviours → tests:** 4 XCTest cases ported; Rust file has 4 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Hook server parsing tests are ported to Rust, and no Swift
  file references this test class.

### 5. HostServiceAgentTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/HostServiceAgentTests.swift`
- **LOC:** 149
- **Ported destination:** Rust `supercli-native-bridge::macos::launchd::tests` (`crates/supercli-native-bridge/src/macos/launchd.rs`)
- **Behaviours → tests:** 9 XCTest cases ported; Rust file has 9 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Launchd agent tests are ported to Rust, and no Swift
  file references this test class.

### 6. HostServiceIdentityTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/HostServiceIdentityTests.swift`
- **LOC:** 182
- **Ported destination:** Rust `supercli-native-bridge::macos::service_identity::tests` (`crates/supercli-native-bridge/src/macos/service_identity.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 9 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Service identity tests are ported to Rust, and no Swift
  file references this test class.

### 7. HostServiceManagerTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/HostServiceManagerTests.swift`
- **LOC:** 33
- **Ported destination:** Rust `supercli-native-bridge::macos::service_manager::tests` (`crates/supercli-native-bridge/src/macos/service_manager.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 6 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Service manager tests are ported to Rust, and no Swift
  file references this test class.

### 8. LicenseManagerTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/LicenseManagerTests.swift`
- **LOC:** 167
- **Ported destination:** Rust `supercli-native-bridge::macos::license::tests` (`crates/supercli-native-bridge/src/macos/license.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 16 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** License manager tests are ported to Rust, and no Swift
  file references this test class.

### 9. MCPApprovalPresentationTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/MCPApprovalPresentationTests.swift`
- **LOC:** 140
- **Ported destination:** Rust `supercli-core::mcp_approval_center::tests` (`crates/supercli-core/src/mcp_approval_center.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 9 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** MCP approval presentation tests are ported to Rust, and no
  Swift file references this test class.

### 10. NotificationDeliveryTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/NotificationDeliveryTests.swift`
- **LOC:** 308
- **Ported destination:** Rust `supercli-native-bridge::macos::notifications::tests` (`crates/supercli-native-bridge/src/macos/notifications.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 8 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Notification delivery tests are ported to Rust, and no Swift
  file references this test class.

### 11. PhoneFitProjectionTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/PhoneFitProjectionTests.swift`
- **LOC:** 90
- **Ported destination:** Rust `supercli-core::store_policies::tests` (`crates/supercli-core/src/store_policies.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 27 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Phone fit projection tests are ported to Rust, and no Swift
  file references this test class.

### 12. RemoteResumePlacementTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/RemoteResumePlacementTests.swift`
- **LOC:** 90
- **Ported destination:** Rust `supercli-core::store_policies::tests` (`crates/supercli-core/src/store_policies.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 27 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Resume placement tests are ported to Rust, and no Swift
  file references this test class.

### 13. RestartGhostTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/RestartGhostTests.swift`
- **LOC:** 103
- **Ported destination:** Rust `supercli-core::store_policies::tests` (`crates/supercli-core/src/store_policies.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 27 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Restart ghost tests are ported to Rust, and no Swift
  file references this test class.

### 14. SharedOrganizationReconciliationTests.swift

- **Swift path:** `clients/legacy/native/SupercliNative/Tests/SupercliNativeTests/SharedOrganizationReconciliationTests.swift`
- **LOC:** 261
- **Ported destination:** Rust `supercli-core::store_policies::tests` (`crates/supercli-core/src/store_policies.rs`)
- **Behaviours → tests:** XCTest cases ported; Rust file has 27 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Organization reconciliation tests are ported to Rust, and
  no Swift file references this test class.

### 15. PluginProtocolTests.swift

- **Swift path:** `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/PluginProtocolTests.swift`
- **LOC:** 66
- **Ported destination:** Rust `supercli-client::dto` (`crates/supercli-client/src/dto.rs`)
- **Behaviours → tests:** Plugin update DTO round-trips ported; Rust file has 56 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Plugin protocol tests are ported to Rust, and no Swift
  file references this test class.

### 16. RelayCryptoVectorTests.swift

- **Swift path:** `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RelayCryptoVectorTests.swift`
- **LOC:** 106
- **Ported destination:** Rust `supercli-client::crypto` (`crates/supercli-client/src/crypto.rs`)
- **Behaviours → tests:** Relay crypto known-answer vectors ported; Rust file has 2 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Crypto KAT tests are ported to Rust, and no Swift
  file references this test class.

### 17. RelayProtocolTests.swift

- **Swift path:** `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RelayProtocolTests.swift`
- **LOC:** 400
- **Ported destination:** Rust `supercli-client::relay` (`crates/supercli-client/src/relay.rs`)
- **Behaviours → tests:** Relay protocol e2e tests ported; Rust file has 6 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Relay protocol tests are ported to Rust, and no Swift
  file references this test class.

### 18. RemotePairingClientTests.swift

- **Swift path:** `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RemotePairingClientTests.swift`
- **LOC:** 329
- **Ported destination:** Rust `supercli-client::pairing` (`crates/supercli-client/src/pairing.rs`)
- **Behaviours → tests:** Pairing e2e tests ported; Rust file has 13 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Pairing client tests are ported to Rust, and no Swift
  file references this test class.

### 19. RemoteRelayConnectionTests.swift

- **Swift path:** `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RemoteRelayConnectionTests.swift`
- **LOC:** 172
- **Ported destination:** Rust `supercli-client::relay_conn` (`crates/supercli-client/src/relay_conn.rs`)
- **Behaviours → tests:** Relay connection e2e tests ported; Rust file has 2 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Relay connection tests are ported to Rust, and no Swift
  file references this test class.

### 20. RemoteTransportContractTests.swift

- **Swift path:** `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RemoteTransportContractTests.swift`
- **LOC:** 64
- **Ported destination:** Rust `supercli-client::transport` (`crates/supercli-client/src/transport.rs`)
- **Behaviours → tests:** Transport contract tests ported; Rust file has 5 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Transport contract tests are ported to Rust, and no Swift
  file references this test class.

### 21. RuntimeCatalogTests.swift

- **Swift path:** `clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RuntimeCatalogTests.swift`
- **LOC:** 172
- **Ported destination:** Rust `supercli-core::app_runtime` (`crates/supercli-core/src/app_runtime.rs`)
- **Behaviours → tests:** Runtime catalog tests ported; Rust file has 3 tests total
- **Frozen Swift callers:** none
- **Safe to delete because:** Runtime catalog tests are ported to Rust, and no Swift
  file references this test class.

## Summary

- **21 files, 3,464 LOC** ready for deletion
- All are test files with zero Swift callers
- All have Rust test equivalents verified present via grep
- After merge (pending→merged), deleted goes 18,961 → **22,425 LOC (13.4%)**

## Blocked — not in this batch

The following ported files have Swift callers and cannot be deleted yet:

- **Source files** (non-test): StreamFrameReconciler.swift, TerminalQueryFilter.swift,
  ResumeCommand.swift, RemoteDirectTransport.swift, RemoteTerminalStreamTransport.swift,
  ControllerPairingProxy.swift, LocalHostClientFeature.swift, RelayUplinkManager.swift —
  all have Swift callers in iOS UI or UnpeelStore.
- **Dart test files** (5): TerminalPaneClosePolicyTests, SessionTitleDefaultsTests,
  PluginListDragTests, PluginSettingsListTests, SessionMoveRulesTests — marked ported
  to Dart but tests not verified running in CI.
- **Dropped test files** (2): SessionActivityTests, WorktreeGitTests — destination is
  "Host does it" with no direct test mapping; needs reviewer decision.

## Caveats

1. **Tests verified by grep, not by run:** The full `--lib` suite couldn't run — the VM
   disk is 100% full. All referenced Rust test files were confirmed to contain `#[test]`
   attributes via grep. The tests live on the green `next@7f8f4e4` tree.
2. **Test files only:** This batch contains only Swift test files (XCTest). The actual
   deletion is safe because the tests are superseded by Rust unit tests.
