import XCTest
@testable import SupercliNative

final class HostServiceAgentTests: XCTestCase {
    private var agentsDirectory: URL!

    override func setUpWithError() throws {
        agentsDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("supercli-agent-\(UUID().uuidString)", isDirectory: true)
            .appendingPathComponent("LaunchAgents", isDirectory: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: agentsDirectory.deletingLastPathComponent())
    }

    /// Records every launchctl call and answers from a script keyed by the
    /// subcommand; unscripted calls succeed.
    private final class FakeLaunchctl: @unchecked Sendable {
        var calls: [[String]] = []
        var answers: [String: HostServiceAgent.CommandResult] = [:]
        private let lock = NSLock()

        func run(_ arguments: [String]) -> HostServiceAgent.CommandResult {
            lock.lock()
            defer { lock.unlock() }
            calls.append(arguments)
            return answers[arguments.first ?? ""] ?? .init(status: 0, output: "")
        }

        var subcommands: [String] { calls.compactMap(\.first) }
    }

    private func ensure(
        _ tool: FakeLaunchctl,
        hostBinary: String = "/Applications/Supercli.app/Contents/MacOS/supercli-host",
        label: String = HostServiceAgent.releaseLabel
    ) -> HostServiceAgent.Outcome {
        HostServiceAgent.ensureRunning(
            label: label,
            hostBinary: hostBinary,
            agentsDirectory: agentsDirectory,
            uid: 501,
            launchctl: { tool.run($0) }
        )
    }

    func testLabelsKeepDevBuildsOffTheReleaseUnit() {
        XCTAssertEqual(HostServiceAgent.label(developmentBuild: false), "com.supercli.native.serve")
        XCTAssertEqual(HostServiceAgent.label(developmentBuild: true), "com.supercli.native.dev.serve")
        XCTAssertNotEqual(HostServiceAgent.releaseLabel, "com.supercli.serve", "the CLI's unit label is reserved")
    }

    func testDirectLaunchOverrideIsExplicitOptIn() {
        XCTAssertFalse(HostServiceAgent.usesDirectLaunch(environment: [:]))
        XCTAssertFalse(HostServiceAgent.usesDirectLaunch(environment: ["SUPERCLI_NATIVE_SERVICE_LAUNCHER": "launchd"]))
        XCTAssertTrue(HostServiceAgent.usesDirectLaunch(environment: ["SUPERCLI_NATIVE_SERVICE_LAUNCHER": " Direct\n"]))
    }

    func testRenderedUnitRunsTheMachineServiceWithoutKeepAlive() throws {
        let data = try HostServiceAgent.renderPlist(
            label: "com.supercli.native.serve",
            hostBinary: "/Applications/Supercli.app/Contents/MacOS/supercli-host"
        )
        let unit = try XCTUnwrap(
            PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any]
        )
        XCTAssertEqual(unit["Label"] as? String, "com.supercli.native.serve")
        XCTAssertEqual(
            unit["ProgramArguments"] as? [String],
            ["/Applications/Supercli.app/Contents/MacOS/supercli-host", "__serve__"]
        )
        XCTAssertEqual(unit["RunAtLoad"] as? Bool, true)
        // The machine lease makes a second service exit at once; KeepAlive
        // would have launchd respawn that loser forever.
        XCTAssertEqual(unit["KeepAlive"] as? Bool, false)
        XCTAssertEqual(unit["ProcessType"] as? String, "Interactive")
        XCTAssertEqual(unit["AssociatedBundleIdentifiers"] as? [String], ["com.supercli.native"])
        XCTAssertNil(unit["EnvironmentVariables"], "the machine service resolves its own homes")
    }

    func testFirstRunWritesTheUnitThenBootstrapsAndKickstarts() throws {
        let tool = FakeLaunchctl()
        XCTAssertEqual(ensure(tool), .running(label: HostServiceAgent.releaseLabel, rewrote: true))
        let plist = HostServiceAgent.plistURL(label: HostServiceAgent.releaseLabel, agentsDirectory: agentsDirectory)
        XCTAssertTrue(FileManager.default.fileExists(atPath: plist.path))
        XCTAssertEqual(tool.calls, [
            ["bootout", "gui/501/com.supercli.native.serve"],
            ["bootstrap", "gui/501", plist.path],
            ["kickstart", "gui/501/com.supercli.native.serve"],
        ])
    }

    func testUnchangedUnitIsNeverReloaded() {
        let tool = FakeLaunchctl()
        _ = ensure(tool)
        tool.calls = []
        // Every launch after the first: the job is already loaded, so
        // bootstrap reports an error that `print` disproves.
        tool.answers["bootstrap"] = .init(status: 5, output: "Bootstrap failed: 5: Input/output error")
        XCTAssertEqual(ensure(tool), .running(label: HostServiceAgent.releaseLabel, rewrote: false))
        XCTAssertEqual(tool.subcommands, ["bootstrap", "print", "kickstart"])
    }

    func testMovedBundleRewritesAndReloadsTheUnit() {
        let tool = FakeLaunchctl()
        _ = ensure(tool, hostBinary: "/Applications/Supercli.app/Contents/MacOS/supercli-host")
        tool.calls = []
        XCTAssertEqual(
            ensure(tool, hostBinary: "/Users/me/Applications/Supercli.app/Contents/MacOS/supercli-host"),
            .running(label: HostServiceAgent.releaseLabel, rewrote: true)
        )
        XCTAssertEqual(tool.subcommands, ["bootout", "bootstrap", "kickstart"])
    }

    func testBootstrapFailureWithoutALoadedJobFallsBack() {
        let tool = FakeLaunchctl()
        tool.answers["bootstrap"] = .init(status: 37, output: "Bootstrap failed: 37: Operation already in progress")
        tool.answers["print"] = .init(status: 113, output: "Could not find service")
        guard case .failed(let reason) = ensure(tool) else {
            return XCTFail("expected a failure so the app forks the service instead")
        }
        XCTAssertTrue(reason.contains("bootstrap"), reason)
        XCTAssertFalse(tool.subcommands.contains("kickstart"))
    }

    func testKickstartFailureFallsBack() {
        let tool = FakeLaunchctl()
        tool.answers["kickstart"] = .init(status: 1, output: "Could not kickstart service")
        guard case .failed(let reason) = ensure(tool) else {
            return XCTFail("expected a failure so the app forks the service instead")
        }
        XCTAssertTrue(reason.contains("kickstart"), reason)
    }

    func testDevelopmentLabelWritesItsOwnFile() {
        let tool = FakeLaunchctl()
        XCTAssertEqual(
            ensure(tool, label: HostServiceAgent.developmentLabel),
            .running(label: HostServiceAgent.developmentLabel, rewrote: true)
        )
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: agentsDirectory.appendingPathComponent("com.supercli.native.dev.serve.plist").path
        ))
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: agentsDirectory.appendingPathComponent("com.supercli.native.serve.plist").path
        ))
    }
}
