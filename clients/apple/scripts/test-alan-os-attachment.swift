import Darwin
import Foundation

private enum TestFailure: Error, CustomStringConvertible {
    case message(String)

    var description: String {
        switch self {
        case .message(let message): message
        }
    }
}

private func require(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    if !condition() { throw TestFailure.message(message) }
}

private func temporaryDirectory() throws -> URL {
    let url = URL(fileURLWithPath: "/tmp", isDirectory: true)
        .appendingPathComponent("aos-\(getpid())-\(UUID().uuidString.prefix(8))", isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path)
    return url
}

private func makeListener(at path: String) throws -> Int32 {
    let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard descriptor >= 0 else { throw TestFailure.message("socket failed") }
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let capacity = MemoryLayout.size(ofValue: address.sun_path)
    guard path.utf8.count < capacity else {
        Darwin.close(descriptor)
        throw TestFailure.message("test socket path is too long")
    }
    withUnsafeMutablePointer(to: &address.sun_path) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: capacity) { destination in
            path.withCString { source in _ = strncpy(destination, source, capacity - 1) }
        }
    }
    let result = withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { socketAddress in
            Darwin.bind(descriptor, socketAddress, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard result == 0, Darwin.listen(descriptor, 1) == 0 else {
        let detail = String(cString: strerror(errno))
        Darwin.close(descriptor)
        throw TestFailure.message("bind/listen failed: \(detail)")
    }
    guard chmod(path, 0o600) == 0 else {
        Darwin.close(descriptor)
        throw TestFailure.message("chmod socket failed")
    }
    return descriptor
}

private func readExact(_ descriptor: Int32, count: Int) throws -> Data {
    var result = Data(count: count)
    try result.withUnsafeMutableBytes { raw in
        guard let base = raw.baseAddress else { return }
        var received = 0
        while received < count {
            let amount = Darwin.read(descriptor, base.advanced(by: received), count - received)
            guard amount > 0 else { throw TestFailure.message("unexpected socket EOF") }
            received += amount
        }
    }
    return result
}

private func readLine(_ descriptor: Int32) throws -> Data? {
    var data = Data()
    var byte: UInt8 = 0
    while true {
        let amount = Darwin.read(descriptor, &byte, 1)
        if amount == 0 { return data.isEmpty ? nil : data }
        guard amount > 0 else { throw TestFailure.message("socket read failed") }
        if byte == 0x0A { return data }
        data.append(byte)
    }
}

private func sendAll(_ descriptor: Int32, data: Data) throws {
    try data.withUnsafeBytes { raw in
        guard let base = raw.baseAddress else { return }
        var sent = 0
        while sent < raw.count {
            let amount = Darwin.write(descriptor, base.advanced(by: sent), raw.count - sent)
            guard amount > 0 else { throw TestFailure.message("socket write failed") }
            sent += amount
        }
    }
}

private func uint32BigEndian(_ data: Data) -> UInt32 {
    var value: UInt32 = 0
    _ = withUnsafeMutableBytes(of: &value) { destination in data.copyBytes(to: destination) }
    return UInt32(bigEndian: value)
}

private func serveMockNamespace(listener: Int32, bootID: String) async throws {
    try await Task.detached {
        let descriptor = Darwin.accept(listener, nil, nil)
        guard descriptor >= 0 else { throw TestFailure.message("accept failed") }
        defer { Darwin.close(descriptor) }

        let header = try readExact(descriptor, count: MemoryLayout<UInt32>.size)
        let payload = try readExact(descriptor, count: Int(uint32BigEndian(header)))
        let attach = try JSONSerialization.jsonObject(with: payload) as? [String: String]
        try require(attach?["op"] == "attach", "client must send the bounded attach request first")

        var paths: [UInt64: String] = [:]
        let files: [String: Data] = [
            "/proc/host/boot_id": Data("\(bootID)\n".utf8),
            "/proc/host/state": Data("ready\n".utf8),
            "/agent/root": Data("io\nmachine\nrequests\nactions\n".utf8),
        ]
        while let line = try readLine(descriptor) {
            guard let envelope = try JSONSerialization.jsonObject(with: line) as? [String: Any],
                  let tag = envelope["tag"],
                  let request = envelope["request"] as? [String: Any],
                  let op = request["op"] as? String
            else { throw TestFailure.message("malformed aP request") }

            let response: [String: Any]
            switch op {
            case "walk":
                let fid = (request["newfid"] as? NSNumber)?.uint64Value ?? 0
                let names = request["names"] as? [String] ?? []
                paths[fid] = "/" + names.joined(separator: "/")
                response = ["op": "walk", "qids": []]
            case "open":
                response = ["op": "open"]
            case "read":
                let fid = (request["fid"] as? NSNumber)?.uint64Value ?? 0
                let offset = (request["offset"] as? NSNumber)?.uint64Value ?? 0
                let count = (request["count"] as? NSNumber)?.uint64Value ?? 0
                let value = files[paths[fid] ?? ""] ?? Data()
                let start = min(Int(offset), value.count)
                let end = min(start + Int(count), value.count)
                response = ["op": "read", "data": [UInt8](value[start..<end])]
            case "clunk":
                let fid = (request["fid"] as? NSNumber)?.uint64Value ?? 0
                paths.removeValue(forKey: fid)
                response = ["op": "clunk"]
            default:
                throw TestFailure.message("unexpected aP request \(op)")
            }
            var encoded = try JSONSerialization.data(withJSONObject: [
                "tag": tag,
                "status": "ok",
                "response": response,
            ])
            encoded.append(0x0A)
            try sendAll(descriptor, data: encoded)
        }
    }.value
}

private func testStreamOverlapAndGap() throws {
    var accumulator = AlanAgentStreamAccumulator(nextOffset: 5)
    let fresh = try accumulator.accept(
        AlanAgentStreamChunk(
            stream: "io/output",
            requestedOffset: 3,
            nextOffset: 8,
            data: Data("34567".utf8)
        )
    )
    try require(String(decoding: fresh, as: UTF8.self) == "567", "overlap must be deduplicated")
    try require(accumulator.nextOffset == 8, "caller offset must advance to the returned edge")

    do {
        _ = try accumulator.accept(
            AlanAgentStreamChunk(
                stream: "io/output",
                requestedOffset: 10,
                nextOffset: 11,
                data: Data("x".utf8)
            )
        )
        throw TestFailure.message("a discontinuous stream chunk must fail visibly")
    } catch AlanOSAttachmentError.retentionGap(let stream, let requested, let available) {
        try require(stream == "io/output" && requested == 8 && available == 10, "gap details must be stable")
    }
}

private func testNativeConnectionRequestOwnership() throws {
    try require(
        !alanOSNativeAdapterOwnsConnectionRequest("cli-1234"),
        "renderer must not claim CLI-owned native requests"
    )
    try require(
        alanOSNativeAdapterOwnsConnectionRequest("agent-1234"),
        "renderer must handle non-CLI native requests"
    )
}

private func testProtectedStatusDiscovery() throws {
    let fileManager = FileManager.default
    let temporary = try temporaryDirectory()
    defer { try? fileManager.removeItem(at: temporary) }
    let runtimeRoot = temporary.appendingPathComponent("alan-os-\(getuid())", isDirectory: true)
    let paths = AlanOSHostEndpointPaths(
        channel: .dev,
        fileManager: fileManager,
        runtimeRoot: runtimeRoot
    )
    try fileManager.createDirectory(at: paths.root, withIntermediateDirectories: true)
    for directory in [runtimeRoot, paths.productRoot, paths.root] {
        try fileManager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
    }
    let listener = try makeListener(at: paths.socket.path)
    defer { Darwin.close(listener) }
    let status = AlanOSHostStatus(
        version: 1,
        channelID: "dev",
        bootID: UUID().uuidString.lowercased(),
        pid: UInt32(getpid()),
        readiness: "ready",
        socket: paths.socket.path
    )
    try JSONEncoder().encode(status).write(to: paths.status)
    try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: paths.status.path)
    let discovered = try paths.readStatus()
    try require(discovered == status, "protected status must resolve the matching endpoint")

    let target = paths.root.appendingPathComponent("redirected-status.json")
    try fileManager.moveItem(at: paths.status, to: target)
    try fileManager.createSymbolicLink(at: paths.status, withDestinationURL: target)
    do {
        _ = try paths.readStatus()
        throw TestFailure.message("status discovery must reject symlink redirection")
    } catch is AlanOSAttachmentError {
        // Expected.
    }
}

private func testAPAttachAndNamespaceValidation() async throws {
    let fileManager = FileManager.default
    let temporary = try temporaryDirectory()
    defer { try? fileManager.removeItem(at: temporary) }
    let socket = temporary.appendingPathComponent("namespace.ap.sock")
    let listener = try makeListener(at: socket.path)
    let bootID = UUID().uuidString.lowercased()
    let status = AlanOSHostStatus(
        version: 1,
        channelID: "dev",
        bootID: bootID,
        pid: UInt32(getpid()),
        readiness: "ready",
        socket: socket.path
    )

    async let server: Void = serveMockNamespace(listener: listener, bootID: bootID)
    let session = try await AlanOSAttachmentSession.connect(status: status, channel: .dev)
    let published = try await session.cat("/proc/host/boot_id")
    try require(String(decoding: published, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines) == bootID, "aP client must read the attached namespace")
    await session.detach()
    try await server
    Darwin.close(listener)
}

@main
private enum AlanOSAttachmentTests {
    static func main() async throws {
        try testStreamOverlapAndGap()
        try testNativeConnectionRequestOwnership()
        try testProtectedStatusDiscovery()
        try await testAPAttachAndNamespaceValidation()
        print("Alan OS attachment tests passed.")
    }
}
