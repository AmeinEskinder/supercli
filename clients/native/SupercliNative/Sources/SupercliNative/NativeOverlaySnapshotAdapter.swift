import Foundation

/// Bounded, allowlisted UserDefaults projection supplied to the canonical
/// Rust Host. The worker parses the same plist dialect as the released
/// default-workspace compatibility loader; no unrelated defaults (or future
/// secrets) can cross the platform callback accidentally.
enum NativeOverlaySnapshotAdapter {
    static let maximumPlistBytes = 320 * 1024

    private static let exactKeys: Set<String> = [
        "supercli.native.appTint",
        "supercli.native.defaultWorkspaceName",
        "supercli.native.projects",
        "supercli.native.projectOrder",
        "supercli.sidebar.pins",
        "supercli.native.sessionTitles",
        "supercli.native.projectFolderColors",
        "supercli.native.archivedSessions",
        "supercli.native.archivedAt",
        "supercli.native.presets",
    ]

    private static let prefixes = [
        "supercli.native.sessionOrder.",
        "supercli.native.pinnedOrder.",
    ]

    static func responseBody(defaults: UserDefaults = AppDefaults.shared) throws -> String {
        let filtered = defaults.dictionaryRepresentation().filter { key, _ in
            exactKeys.contains(key) || prefixes.contains(where: key.hasPrefix)
        }
        guard PropertyListSerialization.propertyList(
            filtered,
            isValidFor: .binary
        ) else {
            throw NSError(
                domain: "SupercliNativeOverlay",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "native overlay contains an invalid value"]
            )
        }
        let plist = try PropertyListSerialization.data(
            fromPropertyList: filtered,
            format: .binary,
            options: 0
        )
        guard plist.count <= maximumPlistBytes else {
            throw NSError(
                domain: "SupercliNativeOverlay",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "native overlay is too large"]
            )
        }
        let data = try JSONSerialization.data(withJSONObject: [
            "defaultsPlistBase64": plist.base64EncodedString(),
        ])
        guard let body = String(data: data, encoding: .utf8) else {
            throw NSError(
                domain: "SupercliNativeOverlay",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "native overlay could not be encoded"]
            )
        }
        return body
    }
}
