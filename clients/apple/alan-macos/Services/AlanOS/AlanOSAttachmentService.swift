#if os(macOS)
import Darwin
import AppKit
import Combine
import Foundation
import Security

enum AlanOSAttachmentError: LocalizedError, Equatable {
    case invalidStatus(String)
    case hostUnavailable(String)
    case transport(String)
    case protocolFailure(String)
    case processUnavailable(String)
    case priorBoot
    case retentionGap(stream: String, requested: UInt64, available: UInt64)

    var errorDescription: String? {
        switch self {
        case .invalidStatus(let detail), .hostUnavailable(let detail), .transport(let detail),
             .protocolFailure(let detail), .processUnavailable(let detail):
            return detail
        case .priorBoot:
            return "This Agent Process belongs to an earlier Alan OS boot."
        case .retentionGap(let stream, let requested, let available):
            return "\(stream) cannot resume at offset \(requested); its current available length is \(available)."
        }
    }
}

struct AlanOSHostStatus: Codable, Equatable {
    let version: UInt16
    let channelID: String
    let bootID: String
    let pid: UInt32
    let readiness: String
    let socket: String

    private enum CodingKeys: String, CodingKey {
        case version
        case channelID = "channel_id"
        case bootID = "boot_id"
        case pid
        case readiness
        case socket
    }
}

struct AlanAPQID: Codable, Equatable {
    let kind: String
    let version: UInt32
    let path: UInt64
}

struct AlanAPStat: Codable, Equatable {
    let name: String
    let qid: AlanAPQID
    let length: UInt64
    let writable: Bool
}

struct AlanOSValidatedProcess: Equatable {
    let reference: AlanOSProcessReference
    let procQID: AlanAPQID
    let status: String
}

struct AlanAgentStreamChunk: Equatable {
    let stream: String
    let requestedOffset: UInt64
    let nextOffset: UInt64
    let data: Data
}

struct AlanAgentPendingRequest: Equatable {
    let id: String
    let kind: String
    let prompt: String
    let options: String
}

/// Deduplicates intentional overlap reads while keeping the caller-owned next offset explicit.
struct AlanAgentStreamAccumulator: Equatable {
    private(set) var nextOffset: UInt64

    init(nextOffset: UInt64) {
        self.nextOffset = nextOffset
    }

    mutating func accept(_ chunk: AlanAgentStreamChunk) throws -> Data {
        guard chunk.requestedOffset <= nextOffset else {
            throw AlanOSAttachmentError.retentionGap(
                stream: chunk.stream,
                requested: nextOffset,
                available: chunk.requestedOffset
            )
        }
        let overlap = nextOffset - chunk.requestedOffset
        guard overlap <= UInt64(chunk.data.count) else {
            return Data()
        }
        let fresh = chunk.data.dropFirst(Int(overlap))
        nextOffset = max(nextOffset, chunk.nextOffset)
        return Data(fresh)
    }

    mutating func recover(at availableOffset: UInt64) {
        nextOffset = availableOffset
    }
}

private struct AlanAPWireFailure: Error {
    let code: String
}

/// One app-level Shell Process attachment. Every fid and offset remains caller-owned.
actor AlanOSAttachmentSession {
    nonisolated let status: AlanOSHostStatus

    private var descriptor: Int32
    private var nextFid: UInt64 = 1
    private var nextTag: UInt64 = 1
    private var receiveBuffer = Data()

    private init(status: AlanOSHostStatus, descriptor: Int32) {
        self.status = status
        self.descriptor = descriptor
    }

    deinit {
        if descriptor >= 0 {
            Darwin.close(descriptor)
        }
    }

    static func attach(
        channel: AlanInstallChannel = .current(),
        fileManager: FileManager = .default
    ) async throws -> AlanOSAttachmentSession {
        let paths = AlanOSHostEndpointPaths(channel: channel, fileManager: fileManager)
        var status = try? paths.readStatus()
        if status?.readiness != "ready" {
            try paths.requestPlatformHostStart()
            status = try await paths.waitUntilReady()
        }
        guard let status else {
            throw AlanOSAttachmentError.hostUnavailable("Alan OS Host did not publish readiness.")
        }
        return try await connect(status: status, channel: channel)
    }

    static func connect(
        status: AlanOSHostStatus,
        channel: AlanInstallChannel
    ) async throws -> AlanOSAttachmentSession {
        let descriptor = try connectUnixSocket(path: status.socket)
        do {
            try sendAttachRequest(descriptor: descriptor)
            let session = AlanOSAttachmentSession(status: status, descriptor: descriptor)
            try await session.validateNamespace(channel: channel)
            return session
        } catch {
            Darwin.close(descriptor)
            throw error
        }
    }

    func detach() {
        guard descriptor >= 0 else { return }
        Darwin.shutdown(descriptor, SHUT_RDWR)
        Darwin.close(descriptor)
        descriptor = -1
        receiveBuffer.removeAll(keepingCapacity: false)
    }

    func cat(_ path: String) throws -> Data {
        try read(path, offset: 0, count: 1 << 20)
    }

    func list(_ path: String) throws -> [String] {
        let data = try cat(path)
        guard let text = String(data: data, encoding: .utf8) else {
            throw AlanOSAttachmentError.protocolFailure("Alan OS returned non-UTF-8 directory data for \(path).")
        }
        return text.split(whereSeparator: \.isNewline).map(String.init)
    }

    func stat(_ path: String) throws -> AlanAPStat {
        let fid = try walk(path)
        defer { try? clunk(fid) }
        let response = try call(op: "stat", fields: ["fid": fid])
        guard response["op"] as? String == "stat",
              let raw = response["stat"] as? [String: Any]
        else {
            throw AlanOSAttachmentError.protocolFailure("Invalid aP stat response for \(path).")
        }
        let data = try JSONSerialization.data(withJSONObject: raw)
        return try JSONDecoder().decode(AlanAPStat.self, from: data)
    }

    func validate(_ reference: AlanOSProcessReference) throws -> AlanOSValidatedProcess {
        guard reference.bootID == status.bootID else {
            throw AlanOSAttachmentError.priorBoot
        }
        let procPath = "/proc/\(reference.pid)"
        do {
            let procStat = try stat(procPath)
            let stateData = try cat("\(procPath)/status")
            let state = String(decoding: stateData, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return AlanOSValidatedProcess(reference: reference, procQID: procStat.qid, status: state)
        } catch let failure as AlanAPWireFailure where failure.code == "not_found" {
            throw AlanOSAttachmentError.processUnavailable("Alan OS Process \(reference.pid) is unavailable.")
        }
    }

    func readAgentStream(
        reference: AlanOSProcessReference,
        relativePath: String,
        offset: UInt64,
        overlap: UInt64 = 0,
        count: UInt32 = 64 * 1024
    ) throws -> AlanAgentStreamChunk {
        _ = try validate(reference)
        let path = "/agent/\(reference.pid)/\(relativePath)"
        let metadata = try stat(path)
        guard offset <= metadata.length else {
            throw AlanOSAttachmentError.retentionGap(
                stream: relativePath,
                requested: offset,
                available: metadata.length
            )
        }
        if offset == metadata.length {
            return AlanAgentStreamChunk(
                stream: relativePath,
                requestedOffset: offset,
                nextOffset: offset,
                data: Data()
            )
        }
        let requestedOffset = offset > overlap ? offset - overlap : 0
        let data = try read(path, offset: requestedOffset, count: count)
        return AlanAgentStreamChunk(
            stream: relativePath,
            requestedOffset: requestedOffset,
            nextOffset: requestedOffset + UInt64(data.count),
            data: data
        )
    }

    /// Rehydrates a bounded window ending at a caller-owned offset without moving that offset.
    func readAgentStreamWindow(
        reference: AlanOSProcessReference,
        relativePath: String,
        endingAt offset: UInt64,
        count: UInt32 = 64 * 1024
    ) throws -> Data {
        _ = try validate(reference)
        let path = "/agent/\(reference.pid)/\(relativePath)"
        let metadata = try stat(path)
        guard offset <= metadata.length else {
            throw AlanOSAttachmentError.retentionGap(
                stream: relativePath,
                requested: offset,
                available: metadata.length
            )
        }
        let start = offset > UInt64(count) ? offset - UInt64(count) : 0
        guard start < offset else { return Data() }
        return try read(path, offset: start, count: UInt32(offset - start))
    }

    func latestPendingRequest(
        reference: AlanOSProcessReference
    ) throws -> AlanAgentPendingRequest? {
        _ = try validate(reference)
        let base = "/agent/\(reference.pid)/requests"
        let requestIDs = try list(base)
            .filter { $0 != "clone" && $0 != "events" }
            .sorted(by: alanAgentFileIDPrecedes)
            .reversed()
        for requestID in requestIDs {
            let requestPath = "\(base)/\(requestID)"
            guard try readText("\(requestPath)/status") == "pending" else { continue }
            return AlanAgentPendingRequest(
                id: requestID,
                kind: try readText("\(requestPath)/kind"),
                prompt: try readText("\(requestPath)/prompt"),
                options: (try? readText("\(requestPath)/options")) ?? ""
            )
        }
        return nil
    }

    func rootAgentAttachment() throws -> AlanAgentAttachment {
        let text = try readText("/mnt/service-manager/units/root-agent/pid")
        guard let pid = UInt64(text), pid > 0 else {
            throw AlanOSAttachmentError.protocolFailure("Root Agent Process has no valid PID.")
        }
        let reference = AlanOSProcessReference(bootID: status.bootID, pid: pid)
        _ = try validate(reference)
        return AlanAgentAttachment(
            process: reference,
            offsets: .zero,
            presentation: .default
        )
    }

    func writeAgentInput(reference: AlanOSProcessReference, data: Data) throws {
        _ = try validate(reference)
        try writeDocument("/agent/\(reference.pid)/io/input", data: data)
    }

    func writeFile(_ path: String, data: Data) throws {
        try writeDocument(path, data: data)
    }

    func respond(reference: AlanOSProcessReference, requestID: String, data: Data) throws {
        _ = try validate(reference)
        try writeDocument("/agent/\(reference.pid)/requests/\(requestID)/response", data: data)
    }

    func controlMachine(reference: AlanOSProcessReference, command: String) throws {
        _ = try validate(reference)
        try writeDocument("/agent/\(reference.pid)/machine/ctl", data: Data(command.utf8))
    }

    func interrupt(reference: AlanOSProcessReference) throws {
        _ = try validate(reference)
        try writeDocument("/agent/\(reference.pid)/machine/ctl", data: Data("interrupt".utf8))
    }

    func stop(reference: AlanOSProcessReference) throws {
        _ = try validate(reference)
        try writeDocument("/proc/\(reference.pid)/ctl", data: Data("cancel".utf8))
    }

    private func validateNamespace(channel: AlanInstallChannel) throws {
        guard status.version == 1,
              status.channelID == channel.installChannelID,
              status.readiness == "ready"
        else {
            throw AlanOSAttachmentError.invalidStatus("Alan OS Host status does not match this app channel.")
        }
        let publishedBootID = String(decoding: try cat("/proc/host/boot_id"), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard publishedBootID == status.bootID else {
            throw AlanOSAttachmentError.invalidStatus("Alan OS Host status and namespace boot identity differ.")
        }
        guard try cat("/proc/host/state") == Data("ready\n".utf8) else {
            throw AlanOSAttachmentError.hostUnavailable("Alan OS namespace is not ready.")
        }
        _ = try list("/agent/root")
    }

    private func read(_ path: String, offset: UInt64, count: UInt32) throws -> Data {
        let fid = try walk(path)
        defer { try? clunk(fid) }
        _ = try call(op: "open", fields: ["fid": fid, "mode": "read"])
        let response = try call(op: "read", fields: ["fid": fid, "offset": offset, "count": count])
        guard response["op"] as? String == "read",
              let bytes = response["data"] as? [UInt8]
        else {
            throw AlanOSAttachmentError.protocolFailure("Invalid aP read response for \(path).")
        }
        return Data(bytes)
    }

    private func readText(_ path: String) throws -> String {
        String(decoding: try cat(path), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func writeDocument(_ path: String, data: Data) throws {
        let fid = try walk(path)
        var shouldClunk = true
        defer {
            if shouldClunk { try? clunk(fid) }
        }
        _ = try call(op: "open", fields: ["fid": fid, "mode": "write"])
        _ = try call(op: "write", fields: ["fid": fid, "offset": UInt64(0), "data": [UInt8](data)])
        try clunk(fid)
        shouldClunk = false
    }

    private func walk(_ path: String) throws -> UInt64 {
        let names = path.split(separator: "/").map(String.init)
        let fid = nextFid
        nextFid += 1
        _ = try call(op: "walk", fields: ["fid": UInt64(0), "newfid": fid, "names": names])
        return fid
    }

    private func clunk(_ fid: UInt64) throws {
        _ = try call(op: "clunk", fields: ["fid": fid])
    }

    private func call(op: String, fields: [String: Any]) throws -> [String: Any] {
        guard descriptor >= 0 else {
            throw AlanOSAttachmentError.transport("Alan OS attachment is detached.")
        }
        let tag = nextTag
        nextTag += 1
        var request = fields
        request["op"] = op
        let frame: [String: Any] = ["tag": tag, "request": request]
        var data = try JSONSerialization.data(withJSONObject: frame)
        data.append(0x0A)
        try sendAll(descriptor: descriptor, data: data)
        let responseData = try readLine()
        guard let envelope = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              (envelope["tag"] as? NSNumber)?.uint64Value == tag,
              let status = envelope["status"] as? String
        else {
            throw AlanOSAttachmentError.protocolFailure("Malformed or mismatched aP response.")
        }
        if status == "error" {
            throw AlanAPWireFailure(code: envelope["code"] as? String ?? "io")
        }
        guard status == "ok", let response = envelope["response"] as? [String: Any] else {
            throw AlanOSAttachmentError.protocolFailure("Malformed successful aP response.")
        }
        return response
    }

    private func readLine() throws -> Data {
        while true {
            if let newline = receiveBuffer.firstIndex(of: 0x0A) {
                let line = receiveBuffer.prefix(upTo: newline)
                receiveBuffer.removeSubrange(receiveBuffer.startIndex...newline)
                return Data(line)
            }
            var bytes = [UInt8](repeating: 0, count: 8192)
            let count = Darwin.read(descriptor, &bytes, bytes.count)
            guard count > 0 else {
                throw AlanOSAttachmentError.transport("Alan OS closed the aP connection.")
            }
            receiveBuffer.append(contentsOf: bytes.prefix(count))
            guard receiveBuffer.count <= 1 << 20 else {
                throw AlanOSAttachmentError.protocolFailure("aP response exceeded the wire limit.")
            }
        }
    }
}

@MainActor
final class AlanOSAttachmentController: ObservableObject {
    enum State: Equatable {
        case detached
        case attaching
        case ready(bootID: String)
        case unavailable(String)
    }

    static let shared = AlanOSAttachmentController()

    @Published private(set) var state: State = .detached
    private(set) var session: AlanOSAttachmentSession?
    private var attachTask: Task<Void, Never>?

    func attach() {
        guard attachTask == nil, session == nil else { return }
        state = .attaching
        attachTask = Task { [weak self] in
            do {
                let session = try await AlanOSAttachmentSession.attach()
                guard !Task.isCancelled else {
                    await session.detach()
                    return
                }
                self?.session = session
                self?.state = .ready(bootID: session.status.bootID)
            } catch {
                self?.state = .unavailable(error.localizedDescription)
            }
            self?.attachTask = nil
        }
    }

    func detach() {
        attachTask?.cancel()
        attachTask = nil
        let active = session
        session = nil
        state = .detached
        Task { await active?.detach() }
    }
}

struct AlanOSHostEndpointPaths {
    let channel: AlanInstallChannel
    let runtimeRoot: URL
    let productRoot: URL
    let root: URL
    let socket: URL
    let status: URL

    init(
        channel: AlanInstallChannel,
        fileManager: FileManager,
        runtimeRoot override: URL? = nil
    ) {
        self.channel = channel
        runtimeRoot = override ?? fileManager.temporaryDirectory
            .appendingPathComponent("alan-os-\(getuid())", isDirectory: true)
        productRoot = runtimeRoot
            .appendingPathComponent("Alan OS", isDirectory: true)
        root = productRoot.appendingPathComponent(channel.installChannelID, isDirectory: true)
        socket = root.appendingPathComponent("namespace.ap.sock", isDirectory: false)
        status = root.appendingPathComponent("host.json", isDirectory: false)
    }

    func readStatus() throws -> AlanOSHostStatus {
        try validatePrivateDirectory(runtimeRoot)
        try validatePrivateDirectory(productRoot)
        try validatePrivateDirectory(root)
        let value = try JSONDecoder().decode(
            AlanOSHostStatus.self,
            from: try readOwnedPrivateFile(status)
        )
        guard value.channelID == channel.installChannelID,
              value.version == 1,
              value.pid > 0,
              UUID(uuidString: value.bootID) != nil,
              value.readiness == "ready" || value.readiness == "stopping",
              URL(fileURLWithPath: value.socket).standardizedFileURL == socket.standardizedFileURL
        else {
            throw AlanOSAttachmentError.invalidStatus("Alan OS Host endpoint does not match this app channel.")
        }
        try validateOwnedPrivateSocket(socket)
        return value
    }

    func waitUntilReady() async throws -> AlanOSHostStatus {
        let deadline = ContinuousClock.now + .seconds(10)
        var lastError = "Host status is unavailable."
        while ContinuousClock.now < deadline {
            do {
                let value = try readStatus()
                if value.readiness == "ready" { return value }
            } catch {
                lastError = error.localizedDescription
            }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw AlanOSAttachmentError.hostUnavailable("Alan OS Host did not become ready: \(lastError)")
    }

    func requestPlatformHostStart(
        bundle: Bundle = .main,
        process: Process = Process()
    ) throws {
        let name = channel == .dev ? "alan-os-host-dev" : "alan-os-host"
        guard let executable = bundle.resourceURL?
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(name),
              FileManager.default.isExecutableFile(atPath: executable.path)
        else {
            throw AlanOSAttachmentError.hostUnavailable("The signed app does not contain \(name).")
        }
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        process.arguments = [
            "submit", "-l", "\(channel.bundleIdentifier).os-host",
            "-p", executable.path, "-o", "/dev/null", "-e", "/dev/null", "--",
            executable.path,
        ]
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw AlanOSAttachmentError.hostUnavailable("launchd rejected the Alan OS Host start request.")
        }
    }
}

private func validatePrivateDirectory(_ url: URL) throws {
    var metadata = Darwin.stat()
    guard lstat(url.path, &metadata) == 0,
          metadata.st_uid == getuid(),
          (metadata.st_mode & S_IFMT) == S_IFDIR,
          metadata.st_mode & 0o077 == 0
    else {
        throw AlanOSAttachmentError.invalidStatus(
            "Alan OS runtime directory is missing, redirected, foreign-owned, or not private."
        )
    }
}

private func validateOwnedPrivateSocket(_ url: URL) throws {
    var metadata = Darwin.stat()
    guard lstat(url.path, &metadata) == 0,
          metadata.st_uid == getuid(),
          (metadata.st_mode & S_IFMT) == S_IFSOCK,
          metadata.st_mode & 0o077 == 0
    else {
        throw AlanOSAttachmentError.invalidStatus(
            "Alan OS attachment socket is missing, redirected, foreign-owned, or not private."
        )
    }
}

private func readOwnedPrivateFile(_ url: URL) throws -> Data {
    let descriptor = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW)
    guard descriptor >= 0 else {
        throw AlanOSAttachmentError.invalidStatus("Alan OS Host status is unavailable or redirected.")
    }
    defer { Darwin.close(descriptor) }
    var metadata = Darwin.stat()
    guard fstat(descriptor, &metadata) == 0,
          metadata.st_uid == getuid(),
          (metadata.st_mode & S_IFMT) == S_IFREG,
          metadata.st_mode & 0o077 == 0
    else {
        throw AlanOSAttachmentError.invalidStatus(
            "Alan OS Host status is foreign-owned or not private."
        )
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
    return try handle.readToEnd() ?? Data()
}

private func connectUnixSocket(path: String) throws -> Int32 {
    let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard descriptor >= 0 else {
        throw AlanOSAttachmentError.transport("Could not create the Alan OS Unix socket.")
    }
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let capacity = MemoryLayout.size(ofValue: address.sun_path)
    guard path.utf8.count < capacity else {
        Darwin.close(descriptor)
        throw AlanOSAttachmentError.transport("Alan OS Unix socket path is too long.")
    }
    withUnsafeMutablePointer(to: &address.sun_path) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: capacity) { destination in
            path.withCString { source in
                _ = strncpy(destination, source, capacity - 1)
            }
        }
    }
    let result = withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
            Darwin.connect(descriptor, socketAddress, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard result == 0 else {
        let message = String(cString: strerror(errno))
        Darwin.close(descriptor)
        throw AlanOSAttachmentError.transport("Could not connect to Alan OS Host: \(message)")
    }
    var peerUID: uid_t = 0
    var peerGID: gid_t = 0
    guard getpeereid(descriptor, &peerUID, &peerGID) == 0, peerUID == getuid() else {
        Darwin.close(descriptor)
        throw AlanOSAttachmentError.transport("Alan OS Host peer identity did not match this user.")
    }
    return descriptor
}

private func alanAgentFileIDPrecedes(_ lhs: String, _ rhs: String) -> Bool {
    let leftNumber = UInt64(lhs.drop(while: { !$0.isNumber }))
    let rightNumber = UInt64(rhs.drop(while: { !$0.isNumber }))
    if let leftNumber, let rightNumber, leftNumber != rightNumber {
        return leftNumber < rightNumber
    }
    return lhs < rhs
}

private func sendAttachRequest(descriptor: Int32) throws {
    let payload = Data(#"{"op":"attach"}"#.utf8)
    var length = UInt32(payload.count).bigEndian
    var frame = Data(bytes: &length, count: MemoryLayout<UInt32>.size)
    frame.append(payload)
    try sendAll(descriptor: descriptor, data: frame)
}

private func sendAll(descriptor: Int32, data: Data) throws {
    try data.withUnsafeBytes { raw in
        guard let base = raw.baseAddress else { return }
        var sent = 0
        while sent < raw.count {
            let count = Darwin.write(descriptor, base.advanced(by: sent), raw.count - sent)
            guard count > 0 else {
                throw AlanOSAttachmentError.transport("Alan OS aP write failed.")
            }
            sent += count
        }
    }
}

private struct AlanHostMountNativeRequest: Decodable {
    let id: String
    let label: String
    let namespacePath: String
    let access: String
    let reason: String
    let requestingPID: UInt64

    private enum CodingKeys: String, CodingKey {
        case id, label, access, reason
        case namespacePath = "namespace_path"
        case requestingPID = "requesting_pid"
    }
}

private struct AlanConnectionNativeRequest: Decodable {
    let id: String
    let profileID: String
    let action: String

    private enum CodingKeys: String, CodingKey {
        case id, action
        case profileID = "profile_id"
    }
}

private struct AlanConnectionMetadata: Decodable {
    struct Profile: Decodable {
        let credentialID: String?

        private enum CodingKeys: String, CodingKey {
            case credentialID = "credential_id"
        }
    }

    let profiles: [String: Profile]
}

/// CLI-created requests are completed by that CLI process, not by the renderer adapter.
func alanOSNativeAdapterOwnsConnectionRequest(_ requestID: String) -> Bool {
    !requestID.hasPrefix("cli-")
}

/// Observes service-owned request files and supplies only native capabilities.
/// It never owns mount grants, Connection profiles, defaults, or Process lifecycle.
@MainActor
final class AlanOSNativeCapabilityAdapter {
    static let shared = AlanOSNativeCapabilityAdapter()

    private var observationTask: Task<Void, Never>?
    private var inFlight = Set<String>()
    private var dismissedMountRequests = Set<String>()
    private var securityScopedDirectories: [String: URL] = [:]

    func start(attachment: AlanOSAttachmentController) {
        guard observationTask == nil else { return }
        observationTask = Task { [weak self, weak attachment] in
            while !Task.isCancelled {
                if let session = attachment?.session {
                    await self?.poll(session: session)
                }
                try? await Task.sleep(for: .milliseconds(500))
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        inFlight.removeAll()
        dismissedMountRequests.removeAll()
        for url in securityScopedDirectories.values {
            url.stopAccessingSecurityScopedResource()
        }
        securityScopedDirectories.removeAll()
    }

    private func poll(session: AlanOSAttachmentSession) async {
        await pollHostMounts(session: session)
        await pollConnections(session: session)
    }

    private func pollHostMounts(session: AlanOSAttachmentSession) async {
        guard let entries = try? await session.list("/mnt/host-mount/requests") else { return }
        var requests: [AlanHostMountNativeRequest] = []
        for requestID in entries where requestID != "clone" && requestID != "events" {
            let base = "/mnt/host-mount/requests/\(requestID)"
            guard let statusData = try? await session.cat("\(base)/status"),
                  String(decoding: statusData, as: UTF8.self)
                    .trimmingCharacters(in: .whitespacesAndNewlines) == "pending",
                  let requestData = try? await session.cat("\(base)/request"),
                  let request = try? JSONDecoder().decode(
                    AlanHostMountNativeRequest.self,
                    from: requestData
                  ),
                  request.id == requestID
            else { continue }
            requests.append(request)
        }
        requests.sort { alanAgentFileIDPrecedes($0.id, $1.id) }
        let pendingIDs = Set(requests.map(\.id))
        dismissedMountRequests.formIntersection(pendingIDs)
        guard let request = requests.first(where: {
            !inFlight.contains("mount:\($0.id)") && !dismissedMountRequests.contains($0.id)
        }) else { return }
        let key = "mount:\(request.id)"
        inFlight.insert(key)
        defer { inFlight.remove(key) }

        let panel = NSOpenPanel()
        panel.title = request.label
        panel.message = request.reason
        panel.prompt = request.access == "read_write" ? "Allow Read & Write" : "Allow Read"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let directory = panel.url else {
            dismissedMountRequests.insert(request.id)
            return
        }
        let scoped = directory.startAccessingSecurityScopedResource()
        do {
            try AlanOSHostCommandClient.approveHostMount(
                status: session.status,
                request: request,
                directory: directory
            )
            if scoped { securityScopedDirectories[request.id] = directory }
        } catch {
            if scoped { directory.stopAccessingSecurityScopedResource() }
            presentNativeError(title: "Directory access failed", error: error)
        }
    }

    private func pollConnections(session: AlanOSAttachmentSession) async {
        guard let data = try? await session.cat("/mnt/connections/native-requests"),
              let requests = try? JSONDecoder().decode(
                [String: AlanConnectionNativeRequest].self,
                from: data
              ),
              let request = requests.values
                .sorted(by: { $0.id < $1.id })
                .first(where: {
                    alanOSNativeAdapterOwnsConnectionRequest($0.id)
                        && !inFlight.contains("connection:\($0.id)")
                })
        else { return }
        let key = "connection:\(request.id)"
        inFlight.insert(key)
        defer { inFlight.remove(key) }

        do {
            switch request.action {
            case "secret_entry":
                try await handleSecretEntry(request, session: session)
            case "browser_login", "device_login":
                try await runEmbeddedConnectionAdapter(request)
            case "logout":
                try await handleLogout(request, session: session)
            default:
                try await respondConnection(
                    requestID: request.id,
                    opaqueReference: nil,
                    status: "unavailable",
                    session: session
                )
            }
        } catch {
            try? await respondConnection(
                requestID: request.id,
                opaqueReference: nil,
                status: "failed",
                session: session
            )
            presentNativeError(title: "Connection action failed", error: error)
        }
    }

    private func handleSecretEntry(
        _ request: AlanConnectionNativeRequest,
        session: AlanOSAttachmentSession
    ) async throws {
        let metadataData = try await session.cat("/mnt/connections/metadata")
        let metadata = try JSONDecoder().decode(AlanConnectionMetadata.self, from: metadataData)
        guard let credentialID = metadata.profiles[request.profileID]?.credentialID else {
            throw AlanOSAttachmentError.protocolFailure("Connection profile has no credential reference.")
        }
        let field = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 320, height: 24))
        field.placeholderString = "API key"
        let alert = NSAlert()
        alert.messageText = "Set secret for \(request.profileID)"
        alert.informativeText = "The secret is stored in macOS Keychain. Alan OS receives only an opaque reference."
        alert.accessoryView = field
        alert.addButton(withTitle: "Store in Keychain")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn, !field.stringValue.isEmpty else {
            try await respondConnection(
                requestID: request.id,
                opaqueReference: nil,
                status: "unavailable",
                session: session
            )
            return
        }

        let channel = AlanInstallChannel.current().installChannelID
        let service = "app.alanworks.macos.\(channel).connections"
        try AlanKeychainCredentialStore.save(
            secret: field.stringValue,
            service: service,
            account: credentialID
        )
        try await respondConnection(
            requestID: request.id,
            opaqueReference: "host-keychain:\(channel):\(credentialID)",
            status: "ready",
            session: session
        )
    }

    private func runEmbeddedConnectionAdapter(_ request: AlanConnectionNativeRequest) async throws {
        let channel = AlanInstallChannel.current()
        guard let executable = Bundle.main.resourceURL?
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(channel.cliToolName),
              FileManager.default.isExecutableFile(atPath: executable.path)
        else {
            throw AlanOSAttachmentError.hostUnavailable("The signed app does not contain \(channel.cliToolName).")
        }
        let process = Process()
        process.executableURL = executable
        switch request.action {
        case "browser_login":
            process.arguments = ["connection", "login", request.profileID, "browser"]
        case "device_login":
            process.arguments = ["connection", "login", request.profileID, "device"]
        case "logout":
            process.arguments = ["connection", "logout", request.profileID]
        default:
            throw AlanOSAttachmentError.protocolFailure("Unsupported native Connection action.")
        }
        var environment = ProcessInfo.processInfo.environment
        environment["ALAN_NATIVE_CONNECTION_REQUEST_ID"] = request.id
        environment["ALAN_INSTALL_CHANNEL"] = channel.installChannelID
        process.environment = environment
        let output = Pipe()
        process.standardOutput = output
        process.standardError = output
        try process.run()
        let data = await Task.detached {
            let data = output.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            return data
        }.value
        guard process.terminationStatus == 0 else {
            let detail = String(decoding: data, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw AlanOSAttachmentError.hostUnavailable(
                detail.isEmpty ? "Native Connection adapter failed." : detail
            )
        }
    }

    private func handleLogout(
        _ request: AlanConnectionNativeRequest,
        session: AlanOSAttachmentSession
    ) async throws {
        let metadataData = try await session.cat("/mnt/connections/metadata")
        let metadata = try JSONDecoder().decode(AlanConnectionMetadata.self, from: metadataData)
        if let credentialID = metadata.profiles[request.profileID]?.credentialID {
            let channel = AlanInstallChannel.current().installChannelID
            let removed = try AlanKeychainCredentialStore.delete(
                service: "app.alanworks.macos.\(channel).connections",
                account: credentialID
            )
            if removed {
                try await respondConnection(
                    requestID: request.id,
                    opaqueReference: nil,
                    status: "logged_out",
                    session: session
                )
                return
            }
        }
        try await runEmbeddedConnectionAdapter(request)
    }

    private func respondConnection(
        requestID: String,
        opaqueReference: String?,
        status: String,
        session: AlanOSAttachmentSession
    ) async throws {
        let body: [String: Any?] = [
            "request_id": requestID,
            "opaque_credential_ref": opaqueReference,
            "status": status,
        ]
        let normalized = body.reduce(into: [String: Any]()) { result, pair in
            result[pair.key] = pair.value ?? NSNull()
        }
        try await session.writeFile(
            "/mnt/connections/native-responses",
            data: JSONSerialization.data(withJSONObject: normalized)
        )
    }

    private func presentNativeError(title: String, error: Error) {
        let alert = NSAlert(error: error)
        alert.messageText = title
        alert.runModal()
    }
}

private enum AlanKeychainCredentialStore {
    static func save(secret: String, service: String, account: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
        var item = query
        item[kSecValueData as String] = Data(secret.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw AlanOSAttachmentError.hostUnavailable("macOS Keychain rejected the credential (\(status)).")
        }
    }

    static func delete(service: String, account: String) throws -> Bool {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(query as CFDictionary)
        if status == errSecSuccess { return true }
        if status == errSecItemNotFound { return false }
        throw AlanOSAttachmentError.hostUnavailable(
            "macOS Keychain rejected credential deletion (\(status))."
        )
    }
}

private enum AlanOSHostCommandClient {
    static func approveHostMount(
        status: AlanOSHostStatus,
        request: AlanHostMountNativeRequest,
        directory: URL
    ) throws {
        let descriptor = try connectUnixSocket(path: status.socket)
        defer { Darwin.close(descriptor) }
        let command: [String: Any] = [
            "op": "approve_host_mount",
            "request_id": request.id,
            "host_path": directory.path,
        ]
        let payload = try JSONSerialization.data(withJSONObject: command)
        var length = UInt32(payload.count).bigEndian
        var frame = Data(bytes: &length, count: MemoryLayout<UInt32>.size)
        frame.append(payload)
        try sendAll(descriptor: descriptor, data: frame)
        let header = try readExact(descriptor: descriptor, count: MemoryLayout<UInt32>.size)
        var rawResponseLength: UInt32 = 0
        _ = withUnsafeMutableBytes(of: &rawResponseLength) { header.copyBytes(to: $0) }
        let responseLength = UInt32(bigEndian: rawResponseLength)
        guard responseLength <= 64 * 1024 else {
            throw AlanOSAttachmentError.protocolFailure("Host Mount response exceeded its bound.")
        }
        let responseData = try readExact(descriptor: descriptor, count: Int(responseLength))
        guard let response = try JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              response["boot_id"] as? String == status.bootID
        else {
            throw AlanOSAttachmentError.protocolFailure("Host Mount response has the wrong boot identity.")
        }
        if let error = response["error"] as? String {
            throw AlanOSAttachmentError.hostUnavailable(error)
        }
        guard response["host_path"] == nil,
              let grant = response["grant"] as? [String: Any],
              grant["host_path"] == nil,
              grant["id"] as? String == request.id,
              grant["namespace_path"] as? String == request.namespacePath,
              grant["access"] as? String == request.access,
              grant["active"] as? Bool == true
        else {
            throw AlanOSAttachmentError.protocolFailure("Host Mount response was unbounded or mismatched.")
        }
    }
}

private func readExact(descriptor: Int32, count: Int) throws -> Data {
    var result = Data(count: count)
    try result.withUnsafeMutableBytes { raw in
        guard let base = raw.baseAddress else { return }
        var received = 0
        while received < count {
            let amount = Darwin.read(descriptor, base.advanced(by: received), count - received)
            guard amount > 0 else {
                throw AlanOSAttachmentError.transport("Alan OS Host closed a native command response.")
            }
            received += amount
        }
    }
    return result
}
#endif
