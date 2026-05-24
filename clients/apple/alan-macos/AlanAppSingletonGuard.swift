import Foundation

#if os(macOS)
import AppKit
import Darwin

enum AlanAppSingletonGuardError: Error, Equatable {
    case missingBundleIdentifier
    case openLockFailed(path: String, errno: Int32)
    case lockFailed(path: String, errno: Int32)
}

final class AlanAppSingletonGuard {
    enum Acquisition {
        case acquired(AlanAppSingletonGuard)
        case alreadyRunning(ownerPID: Int32?)
    }

    private var fileDescriptor: CInt?
    let lockURL: URL
    let ownerPID: Int32

    private init(fileDescriptor: CInt, lockURL: URL, ownerPID: Int32) {
        self.fileDescriptor = fileDescriptor
        self.lockURL = lockURL
        self.ownerPID = ownerPID
    }

    deinit {
        release()
    }

    static func acquire(
        applicationSupportDirectory: URL? = nil,
        bundleIdentifier: String? = Bundle.main.bundleIdentifier,
        fileManager: FileManager = .default,
        processIdentifier: Int32 = ProcessInfo.processInfo.processIdentifier
    ) throws -> Acquisition {
        guard let bundleIdentifier, !bundleIdentifier.isEmpty else {
            throw AlanAppSingletonGuardError.missingBundleIdentifier
        }

        let lockDirectory = try singletonLockDirectory(
            applicationSupportDirectory: applicationSupportDirectory,
            channel: AlanInstallChannel.current(bundleIdentifier: bundleIdentifier),
            fileManager: fileManager
        )
        let lockURL = lockDirectory.appendingPathComponent(
            "\(sanitizedBundleIdentifier(bundleIdentifier)).lock",
            isDirectory: false
        )
        let descriptor = open(lockURL.path, O_RDWR | O_CREAT | O_CLOEXEC, S_IRUSR | S_IWUSR)
        guard descriptor >= 0 else {
            throw AlanAppSingletonGuardError.openLockFailed(path: lockURL.path, errno: errno)
        }

        if flock(descriptor, LOCK_EX | LOCK_NB) == 0 {
            try writeOwnerPID(processIdentifier, to: descriptor)
            return .acquired(
                AlanAppSingletonGuard(
                    fileDescriptor: descriptor,
                    lockURL: lockURL,
                    ownerPID: processIdentifier
                )
            )
        }

        let lockErrno = errno
        let existingOwnerPID = readOwnerPID(from: descriptor)
        close(descriptor)

        if lockErrno == EWOULDBLOCK || lockErrno == EAGAIN {
            return .alreadyRunning(ownerPID: existingOwnerPID)
        }

        throw AlanAppSingletonGuardError.lockFailed(path: lockURL.path, errno: lockErrno)
    }

    static func activateExistingInstance(
        bundleIdentifier: String? = Bundle.main.bundleIdentifier,
        currentProcessIdentifier: Int32 = ProcessInfo.processInfo.processIdentifier
    ) {
        guard let bundleIdentifier else { return }
        let runningApps = NSRunningApplication.runningApplications(withBundleIdentifier: bundleIdentifier)
        let existingApp = runningApps.first { app in
            app.processIdentifier != currentProcessIdentifier
        }
        existingApp?.activate(options: [.activateAllWindows])
    }

    func release() {
        guard let descriptor = fileDescriptor else { return }
        flock(descriptor, LOCK_UN)
        close(descriptor)
        fileDescriptor = nil
    }

    private static func singletonLockDirectory(
        applicationSupportDirectory: URL?,
        channel: AlanInstallChannel,
        fileManager: FileManager
    ) throws -> URL {
        let appSupportURL =
            applicationSupportDirectory
            ?? alanMacApplicationSupportDirectory(fileManager: fileManager)
        let lockDirectory = appSupportURL
            .appendingPathComponent(channel.applicationSupportDirectoryName, isDirectory: true)
            .appendingPathComponent("SingletonLocks", isDirectory: true)
        try fileManager.createDirectory(at: lockDirectory, withIntermediateDirectories: true)
        return lockDirectory
    }

    private static func sanitizedBundleIdentifier(_ bundleIdentifier: String) -> String {
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "._-"))
        return String(
            bundleIdentifier.unicodeScalars.map { scalar in
                allowed.contains(scalar) ? Character(scalar) : "_"
            }
        )
    }

    private static func writeOwnerPID(_ processIdentifier: Int32, to descriptor: CInt) throws {
        let payload = "\(processIdentifier)\n"
        ftruncate(descriptor, 0)
        lseek(descriptor, 0, SEEK_SET)
        _ = payload.withCString { pointer in
            Darwin.write(descriptor, pointer, strlen(pointer))
        }
        fsync(descriptor)
    }

    private static func readOwnerPID(from descriptor: CInt) -> Int32? {
        lseek(descriptor, 0, SEEK_SET)
        var buffer = [UInt8](repeating: 0, count: 64)
        let count = Darwin.read(descriptor, &buffer, buffer.count)
        guard count > 0 else { return nil }
        let data = Data(buffer.prefix(Int(count)))
        guard let text = String(data: data, encoding: .utf8) else { return nil }
        return Int32(text.trimmingCharacters(in: .whitespacesAndNewlines))
    }
}
#endif
