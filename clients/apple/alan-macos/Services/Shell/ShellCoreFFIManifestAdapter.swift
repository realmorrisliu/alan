import Foundation

extension ShellCoreFFIAdapter {
    func validateContentWorkspaceManifest(data: Data) throws {
        let _: EmptyManifestPayload = try send(
            operation: "manifest.validate",
            payload: ValidateManifestPayload(manifestJSON: String(decoding: data, as: UTF8.self))
        )
    }

    func defaultContentWorkspaceManifest(
        windowID: String,
        defaultWorkingDirectory: String,
        now: Date
    ) throws -> ShellContentWorkspaceManifest {
        let payload = DefaultManifestPayload(
            windowID: windowID,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: Self.iso8601Formatter.string(from: now)
        )
        let response: ManifestPayload = try send(
            operation: "manifest.default_manifest",
            payload: payload
        )
        return response.manifest
    }

    func pruningExpiredTabs(
        manifest: ShellContentWorkspaceManifest,
        now: Date,
        ttl: TimeInterval
    ) throws -> ShellContentWorkspaceManifest {
        let response: ManifestPayload = try send(
            operation: "manifest.pruning_expired_tabs",
            payload: PruningExpiredTabsPayload(
                manifest: manifest,
                now: Self.iso8601Formatter.string(from: now),
                ttlSeconds: Int64(max(0, ttl.rounded(.down)))
            )
        )
        return response.manifest
    }

    func materializeContentWorkspaceManifest(
        manifest: ShellContentWorkspaceManifest,
        defaultWorkingDirectory: String,
        now: Date
    ) throws -> ShellStateSnapshot {
        let response: MaterializedWorkspaceStatePayload = try send(
            operation: "manifest.materialize",
            payload: MaterializeManifestPayload(
                manifest: manifest,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: Self.iso8601Formatter.string(from: now)
            )
        )
        return try response.state.materializedShellState()
    }

}

private struct ValidateManifestPayload: Encodable {
    let manifestJSON: String

    private enum CodingKeys: String, CodingKey {
        case manifestJSON = "manifest_json"
    }
}

private struct EmptyManifestPayload: Decodable {}

private struct DefaultManifestPayload: Encodable {
    let windowID: String
    let defaultWorkingDirectory: String
    let now: String

    private enum CodingKeys: String, CodingKey {
        case windowID = "window_id"
        case defaultWorkingDirectory = "default_working_directory"
        case now
    }
}

private struct PruningExpiredTabsPayload: Encodable {
    let manifest: ShellContentWorkspaceManifest
    let now: String
    let ttlSeconds: Int64

    private enum CodingKeys: String, CodingKey {
        case manifest
        case now
        case ttlSeconds = "ttl_seconds"
    }
}

private struct MaterializeManifestPayload: Encodable {
    let manifest: ShellContentWorkspaceManifest
    let defaultWorkingDirectory: String
    let now: String

    private enum CodingKeys: String, CodingKey {
        case manifest
        case defaultWorkingDirectory = "default_working_directory"
        case now
    }
}

private struct ManifestPayload: Decodable {
    let manifest: ShellContentWorkspaceManifest
}

private struct MaterializedWorkspaceStatePayload: Decodable {
    let state: ShellCorePortableWorkspaceState
}
