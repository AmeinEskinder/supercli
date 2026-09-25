import XCTest
@testable import SupercliNative

final class NativeRelayBridgeTests: XCTestCase {
    func testCallbackWatchdogIncludesConnectBudgetOnlyForUnboundRequests() {
        XCTAssertEqual(
            NativeRelayBridge.callbackWaitMilliseconds(
                requestTimeoutMilliseconds: 10_000,
                mayEstablishConnection: true
            ),
            35_000
        )
        XCTAssertEqual(
            NativeRelayBridge.callbackWaitMilliseconds(
                requestTimeoutMilliseconds: 35_000,
                mayEstablishConnection: false
            ),
            40_000
        )
    }

    func testCallbackWatchdogSaturatesWithoutWrapping() {
        XCTAssertEqual(
            NativeRelayBridge.callbackWaitMilliseconds(
                requestTimeoutMilliseconds: .max,
                mayEstablishConnection: true
            ),
            Int.max
        )
    }
}
