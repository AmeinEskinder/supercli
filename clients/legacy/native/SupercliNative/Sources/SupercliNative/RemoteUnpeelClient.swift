//
//  RemoteSupercliClient.swift
//  SupercliNative
//
//  Mac-as-client v1: connect THIS Supercli to another Supercli's remote server
//  (`supercli-host __remote__`) and open its sessions as local terminal panes
//  running `supercli-host __remote_attach__`. Experimental — gated by
//  SupercliFeatureFlags.remoteSupercliClientEnabled, and the attach CLI itself
//  requires SUPERCLI_REMOTE_ATTACH=1 (injected into the spawned command).
//
//  Credentials come from the other Mac's remote key (its ~/.supercli/remote.json
//  or the JSON status line its server prints): {url, token, fingerprint}.
//  They are persisted to ~/.supercli/remote-peer.json (0600) and referenced by
//  the attach command via --peer-file, so the token never appears in session
//  manifests or shell history.
//

import CryptoKit
import Foundation

struct RemoteSupercliPeer: Codable, Equatable {
    var url: URL
    var token: String
    var fingerprint: String?
    var name: String?

    /// Parse a pasted remote key (remote.json contents / server status line).
    static func parse(_ raw: String) -> RemoteSupercliPeer? {
        guard let data = raw.trimmingCharacters(in: .whitespacesAndNewlines).data(using: .utf8),
              let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let urlString = object["url"] as? String,
              let url = URL(string: urlString),
              let token = object["token"] as? String, !token.isEmpty
        else { return nil }
        return RemoteSupercliPeer(
            url: url,
            token: token,
            fingerprint: object["fingerprint"] as? String,
            name: object["name"] as? String
        )
    }
}

/// One remote session row from `GET /api/sessions` (snake_case, hand-built
/// JSON on the server side — deliberately not the SupercliShared DTOs).
struct RemoteSupercliSession: Identifiable, Equatable {
    let id: String
    let label: String
    let command: String
    let activity: String
}

/// TLS-pinned HTTP client for the peer's self-signed certificate: trusts
/// exactly the certificate whose SHA-256 matches the peer's fingerprint.
final class RemoteSupercliClient: NSObject, URLSessionDelegate, @unchecked Sendable {
    private let peer: RemoteSupercliPeer
    private lazy var session = URLSession(
        configuration: .ephemeral,
        delegate: self,
        delegateQueue: nil
    )

    init(peer: RemoteSupercliPeer) {
        self.peer = peer
    }

    func fetchSessions() async throws -> [RemoteSupercliSession] {
        var request = URLRequest(url: peer.url.appendingPathComponent("api/sessions"))
        request.timeoutInterval = 8
        request.setValue("Bearer \(peer.token)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
            throw RemoteSupercliError.badResponse((response as? HTTPURLResponse)?.statusCode ?? 0)
        }
        guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let rows = object["sessions"] as? [[String: Any]]
        else { return [] }
        return rows.compactMap { row in
            guard let id = row["id"] as? String else { return nil }
            let label = (row["label"] as? String) ?? id
            return RemoteSupercliSession(
                id: id,
                label: label,
                command: (row["command"] as? String) ?? "",
                activity: (row["activity"] as? String) ?? "unknown"
            )
        }
    }

    // MARK: - Certificate pinning

    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let trust = challenge.protectionSpace.serverTrust,
              let certificate = SecTrustCopyCertificateChain(trust).flatMap({ chain in
                  (chain as? [SecCertificate])?.first
              })
        else {
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }
        // No fingerprint: loopback/dev only — accept (matches the CLI).
        guard let expected = peer.fingerprint, !expected.isEmpty else {
            completionHandler(.useCredential, URLCredential(trust: trust))
            return
        }
        let der = SecCertificateCopyData(certificate) as Data
        let actual = SHA256.hash(data: der).map { String(format: "%02x", $0) }.joined()
        if expected.lowercased() == actual {
            completionHandler(.useCredential, URLCredential(trust: trust))
        } else {
            completionHandler(.cancelAuthenticationChallenge, nil)
        }
    }
}

enum RemoteSupercliError: Error, LocalizedError {
    case badResponse(Int)

    var errorDescription: String? {
        switch self {
        case .badResponse(let status):
            return status == 401
                ? "The remote Supercli rejected the token — grab a fresh remote key."
                : "The remote Supercli answered HTTP \(status)."
        }
    }
}

// MARK: - Peer persistence

enum RemoteSupercliPeerStore {
    private static let defaultsKey = "supercli.native.remoteSupercliPeer"

    /// Peer file consumed by `__remote_attach__ --peer-file`, owner-only so
    /// the token stays out of command lines and histories.
    static var peerFileURL: URL {
        LaunchConfig.supercliDir.appendingPathComponent("remote-peer.json")
    }

    static func load() -> RemoteSupercliPeer? {
        guard let data = AppDefaults.shared.data(forKey: defaultsKey) else { return nil }
        return try? JSONDecoder().decode(RemoteSupercliPeer.self, from: data)
    }

    static func save(_ peer: RemoteSupercliPeer) throws {
        let data = try JSONEncoder().encode(peer)
        AppDefaults.shared.set(data, forKey: defaultsKey)
        var object: [String: Any] = [
            "url": peer.url.absoluteString,
            "token": peer.token,
        ]
        if let fingerprint = peer.fingerprint {
            object["fingerprint"] = fingerprint
        }
        let file = try JSONSerialization.data(withJSONObject: object)
        try file.write(to: peerFileURL, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: peerFileURL.path
        )
    }

    static func clear() {
        AppDefaults.shared.removeObject(forKey: defaultsKey)
        try? FileManager.default.removeItem(at: peerFileURL)
    }
}
