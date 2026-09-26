import Foundation
import XCTest
@testable import SupercliNative

final class ResumeCommandTests: XCTestCase {
    func testDecodesHostOwnedRelaunchPlan() throws {
        let data = try JSONSerialization.data(withJSONObject: [
            "command": "agent resume opaque-id",
            "failure_markers": ["missing conversation", "opaque-id"],
        ])
        XCTAssertEqual(
            ResumeCommand.decodeRelaunchPlan(data),
            ResumeCommand.RelaunchPlan(
                command: "agent resume opaque-id",
                failureMarkers: ["missing conversation", "opaque-id"]
            )
        )
    }

    func testOlderHostResponseDefaultsToNoFailureMarkers() throws {
        let data = try JSONSerialization.data(withJSONObject: ["command": "agent --continue"])
        XCTAssertEqual(
            ResumeCommand.decodeRelaunchPlan(data),
            ResumeCommand.RelaunchPlan(command: "agent --continue", failureMarkers: [])
        )
    }

    func testInvalidOrEmptyHostPlanFailsClosed() throws {
        XCTAssertNil(ResumeCommand.decodeRelaunchPlan(Data("{}".utf8)))
        XCTAssertNil(ResumeCommand.decodeRelaunchPlan(Data(#"{"command":""}"#.utf8)))
        XCTAssertNil(ResumeCommand.decodeRelaunchPlan(Data("not-json".utf8)))
    }

    func testManagedStorageCleanupAcceptsOnlyStrictSupercliDescendants() {
        let root = URL(fileURLWithPath: "/tmp/supercli-runtime-test", isDirectory: true)
        XCTAssertEqual(
            SupercliStore.validatedManagedStoragePath(
                "/tmp/supercli-runtime-test/runtime-storage/session-1",
                supercliDir: root
            ),
            "/tmp/supercli-runtime-test/runtime-storage/session-1"
        )
        XCTAssertNil(SupercliStore.validatedManagedStoragePath(root.path, supercliDir: root))
        XCTAssertNil(SupercliStore.validatedManagedStoragePath(
            "/tmp/supercli-runtime-test-escape/session-1",
            supercliDir: root
        ))
        XCTAssertNil(SupercliStore.validatedManagedStoragePath(
            "/tmp/supercli-runtime-test/../outside",
            supercliDir: root
        ))
    }
}
