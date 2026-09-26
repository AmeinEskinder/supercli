import XCTest
@testable import SupercliNative

final class NativeOverlaySnapshotAdapterTests: XCTestCase {
    func testSnapshotIncludesOnlyTheBoundedOverlayAllowlist() throws {
        let suiteName = "com.supercli.tests.overlay.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        defaults.set("teal", forKey: "supercli.native.appTint")
        defaults.set(["s1", "s2"], forKey: "supercli.native.sessionOrder.p1")
        defaults.set("must-not-cross", forKey: "supercli.native.relayDevToken")
        defaults.set("also-private", forKey: "unrelated.secret")

        let body = try NativeOverlaySnapshotAdapter.responseBody(defaults: defaults)
        let json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(body.utf8)) as? [String: Any]
        )
        XCTAssertEqual(json.count, 1)
        let encoded = try XCTUnwrap(json["defaultsPlistBase64"] as? String)
        let plistData = try XCTUnwrap(Data(base64Encoded: encoded))
        var format = PropertyListSerialization.PropertyListFormat.binary
        let plist = try XCTUnwrap(
            PropertyListSerialization.propertyList(
                from: plistData,
                options: [],
                format: &format
            ) as? [String: Any]
        )

        XCTAssertEqual(plist["supercli.native.appTint"] as? String, "teal")
        XCTAssertEqual(
            plist["supercli.native.sessionOrder.p1"] as? [String],
            ["s1", "s2"]
        )
        XCTAssertNil(plist["supercli.native.relayDevToken"])
        XCTAssertNil(plist["unrelated.secret"])
    }

    func testSnapshotRejectsOversizedAllowedData() throws {
        let suiteName = "com.supercli.tests.overlay.large.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        defaults.set(
            Data(repeating: 0x41, count: NativeOverlaySnapshotAdapter.maximumPlistBytes + 1),
            forKey: "supercli.native.projects"
        )

        XCTAssertThrowsError(
            try NativeOverlaySnapshotAdapter.responseBody(defaults: defaults)
        )
    }
}
