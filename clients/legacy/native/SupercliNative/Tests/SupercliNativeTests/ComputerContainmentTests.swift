import XCTest
@testable import SupercliNative

final class ComputerContainmentTests: XCTestCase {
    func testComputerUseDefaultsOff() {
        XCTAssertFalse(AppFeature.computerUse.defaultOn)
    }

    func testComputerUseIsRetiredInDevelopmentBuildsToo() {
        XCTAssertFalse(SupercliFeatureFlags.computerUseAvailable(infoDictionary: nil))
        XCTAssertFalse(SupercliFeatureFlags.computerUseAvailable(infoDictionary: [:]))
        XCTAssertFalse(SupercliFeatureFlags.computerUseAvailable(
            infoDictionary: ["SupercliDevelopmentBuild": false]
        ))
        XCTAssertFalse(SupercliFeatureFlags.computerUseAvailable(
            infoDictionary: ["SupercliDevelopmentBuild": "true"]
        ))
        XCTAssertFalse(SupercliFeatureFlags.computerUseAvailable(
            infoDictionary: ["SupercliDevelopmentBuild": true]
        ))
    }

    func testProductionAvailabilityExcludesOnlyComputerUse() {
        XCTAssertFalse(SupercliFeatureFlags.isAvailable(
            .computerUse, developmentBuild: false
        ))
        XCTAssertFalse(SupercliFeatureFlags.isAvailable(
            .computerUse, developmentBuild: true
        ))

        for feature in AppFeature.all where feature != .computerUse {
            XCTAssertTrue(
                SupercliFeatureFlags.isAvailable(feature, developmentBuild: false),
                "production unexpectedly hid \(feature.key)"
            )
        }
    }

    /// 2026-09-08: only Browser use is still experimental; the others are
    /// shipped Features rows. Graduating or demoting one is a deliberate
    /// registry edit, so pin the split here.
    func testOnlyBrowserUseRemainsExperimental() {
        XCTAssertEqual(
            SupercliFeatureFlags.availableExperimentalFeatures.map(\.key),
            [AppFeature.browserMcp.key]
        )
        XCTAssertEqual(
            SupercliFeatureFlags.availableShippedFeatures.map(\.key),
            [
                AppFeature.remoteWorkspaces.key, AppFeature.worktrees.key,
                AppFeature.sessionsMcp.key, AppFeature.workspaces.key,
            ]
        )
        XCTAssertEqual(
            SupercliFeatureFlags.availableFeatures,
            SupercliFeatureFlags.availableShippedFeatures
                + SupercliFeatureFlags.availableExperimentalFeatures
        )
    }

    func testRetirementAlsoAppliesToOlderHostsAdvertisingComputerUse() {
        XCTAssertFalse(SupercliFeatureFlags.computerUseControllable(hostAdvertisesAvailability: true))
        XCTAssertFalse(SupercliFeatureFlags.computerUseControllable(hostAdvertisesAvailability: false))
        XCTAssertFalse(SupercliFeatureFlags.computerUseControllable(hostAdvertisesAvailability: nil))
    }

    func testComputerTabIsHiddenForEveryHost() {
        XCTAssertFalse(SettingsTab.visibleCases(computerUseControllable: true).contains(.computer))
        // Release build + this Mac's local scope → hidden, exactly as today.
        // (`visibleCases` without the Host flag is the local-scope path; the
        // test runner is not a development bundle.)
        XCTAssertEqual(
            SettingsTab.visibleCases(computerUseControllable: false).contains(.computer),
            SupercliFeatureFlags.isAvailable(.computerUse)
        )
        XCTAssertEqual(
            SettingsTab.visibleCases.contains(.computer),
            SettingsTab.visibleCases(computerUseControllable: false).contains(.computer)
        )
    }

    func testComputerUseExperimentWriteKeepsSiblingGatesAndUnknownKeys() throws {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("supercli-cu-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("app-state.json")
        let seed: [String: Any] = [
            "projects": [], "presets": [],
            "experimental_features": ["sessions_mcp": true, "future_gate": "keep"],
            "a_key_from_a_future_version": ["nested": [1, 2, 3]],
        ]
        try JSONSerialization.data(withJSONObject: seed).write(to: file)

        XCTAssertFalse(SupercliStore.writeComputerUseExperiment(true, appStateFile: file))
        let after = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any]
        )
        let features = try XCTUnwrap(after["experimental_features"] as? [String: Any])
        XCTAssertNil(features["computer_use"])
        XCTAssertEqual(features["sessions_mcp"] as? Bool, true)
        XCTAssertEqual(features["future_gate"] as? String, "keep")
        XCTAssertNotNil(after["a_key_from_a_future_version"])

        XCTAssertTrue(SupercliStore.writeComputerUseExperiment(false, appStateFile: file))
        let off = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any]
        )
        XCTAssertEqual((off["experimental_features"] as? [String: Any])?["computer_use"] as? Bool, false)
    }
}
