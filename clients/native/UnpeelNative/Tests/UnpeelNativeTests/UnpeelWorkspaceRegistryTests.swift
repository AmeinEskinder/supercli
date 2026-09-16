import XCTest
@testable import UnpeelNative

final class UnpeelWorkspaceRegistryTests: XCTestCase {
    func testDecodesReleasedProfilesRegistryWithoutChangingHomes() throws {
        let data = Data(
            #"{"version":1,"profiles":[{"id":"work","name":"Work","home":"/Users/test/.unpeel/profiles/work","createdAt":1234}]}"#.utf8
        )

        XCTAssertEqual(
            try UnpeelWorkspaceRegistry.decodeRegistry(data),
            [
                UnpeelWorkspaceRecord(
                    id: "work",
                    name: "Work",
                    home: "/Users/test/.unpeel/profiles/work",
                    createdAt: 1234
                )
            ]
        )
    }

    func testEncodesReleasedProfilesKeyInsteadOfNewWorkspaceKey() throws {
        let record = UnpeelWorkspaceRecord(
            id: "studio",
            name: "Studio",
            home: "/Users/test/.unpeel/profiles/studio",
            createdAt: 5678
        )
        let data = try UnpeelWorkspaceRegistry.encodeRegistry([record])
        let root = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        let stored = try XCTUnwrap(root["profiles"] as? [[String: Any]])

        XCTAssertEqual(root["version"] as? Int, 1)
        XCTAssertNil(root["workspaces"])
        XCTAssertEqual(stored.count, 1)
        XCTAssertEqual(stored[0]["id"] as? String, record.id)
        XCTAssertEqual(stored[0]["name"] as? String, record.name)
        XCTAssertEqual(stored[0]["home"] as? String, record.home)
        XCTAssertEqual(stored[0]["createdAt"] as? Int, Int(record.createdAt))
    }

    func testLegacyRegistryLocationsAndNewSlugFallbackStayExplicit() {
        XCTAssertTrue(
            UnpeelWorkspaceRegistry.registryURL.path.hasSuffix("/.unpeel/profiles.json")
        )
        XCTAssertTrue(
            UnpeelWorkspaceRegistry.workspaceHomesRoot.path.hasSuffix("/.unpeel/profiles")
        )
        XCTAssertEqual(UnpeelWorkspaceRegistry.slugify("✨"), "workspace")
        XCTAssertEqual(UnpeelWorkspaceRegistry.slugify("Client Work"), "client-work")
    }

    func testWorkspaceFeatureRetainsReleasedPreferenceAndLegacyEnvAlias() {
        let feature = AppFeature.workspaces

        XCTAssertEqual(feature.key, "profiles")
        XCTAssertEqual(feature.defaultsKey, "unpeel.experimental.profiles")
        XCTAssertEqual(feature.envOverride, "UNPEEL_DEV_WORKSPACES")
        XCTAssertEqual(feature.legacyEnvOverrides, ["UNPEEL_DEV_PROFILES"])
        XCTAssertEqual(
            feature.envOverrides,
            ["UNPEEL_DEV_WORKSPACES", "UNPEEL_DEV_PROFILES"]
        )
    }

    func testLocalWorkspacePickerDoesNotDependOnRemoteDevelopmentGate() {
        XCTAssertTrue(WorkspaceFeature.pickerEnabled(
            localWorkspacesEnabled: true,
            remoteHostPickerEnabled: false
        ))
        XCTAssertTrue(WorkspaceFeature.pickerEnabled(
            localWorkspacesEnabled: false,
            remoteHostPickerEnabled: true
        ))
        XCTAssertFalse(WorkspaceFeature.pickerEnabled(
            localWorkspacesEnabled: false,
            remoteHostPickerEnabled: false
        ))
    }

    func testLegacyProfilesSettingsDeepLinkOpensWorkspaces() {
        XCTAssertEqual(SettingsTab.compatibleRawValue("profiles"), .workspaces)
        XCTAssertEqual(SettingsTab.compatibleRawValue("workspaces"), .workspaces)
        XCTAssertEqual(SettingsTab.compatibleRawValue("advanced"), .advanced)
    }

    func testWorktreesSettingsTab() {
        XCTAssertEqual(SettingsTab.compatibleRawValue("worktrees"), .worktrees)
        XCTAssertEqual(SettingsTab.worktrees.title, "Worktrees")
        XCTAssertFalse(SettingsTab.hostScopedCases.contains(.worktrees))
    }

    /// Agents & Apps split into Agents and Plugins, and the Unpeel MCP group
    /// (MCP Settings, Sessions use, Browser use) became Agents + Agent access.
    /// Released deep links and snapshot commands keep resolving.
    func testUnifiedSettingsTabsAbsorbTheRetiredOnes() {
        XCTAssertEqual(SettingsTab.compatibleRawValue("agentsApps"), .agents)
        XCTAssertEqual(SettingsTab.compatibleRawValue("presets"), .agents)
        XCTAssertEqual(SettingsTab.compatibleRawValue("mcp"), .agents)
        XCTAssertEqual(SettingsTab.compatibleRawValue("sessions"), .agentAccess)
        XCTAssertEqual(SettingsTab.compatibleRawValue("browser"), .agentAccess)
        XCTAssertEqual(SettingsTab.compatibleRawValue("plugins"), .plugins)
        XCTAssertEqual(SettingsTab.agents.title, "Agents")
        XCTAssertEqual(SettingsTab.plugins.title, "Plugins")
        XCTAssertEqual(SettingsTab.agentAccess.title, "Agent access")
        for tab in [SettingsTab.agents, .plugins, .agentAccess] {
            XCTAssertTrue(SettingsTab.hostScopedCases.contains(tab), "\(tab) follows the scope picker")
        }
        XCTAssertFalse(SettingsTab.visibleCases.contains(.presets))
        XCTAssertFalse(SettingsTab.visibleCases.contains(.computer))
        let visible = SettingsTab.visibleCases
        XCTAssertEqual(visible.firstIndex(of: .agents).map { $0 + 1 }, visible.firstIndex(of: .plugins))
    }
}
