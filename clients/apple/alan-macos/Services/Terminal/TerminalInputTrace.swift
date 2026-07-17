#if os(macOS)
import Foundation

struct AlanTerminalInputTraceConfig {
    let isEnabled: Bool
    let fileURL: URL?
}

final class AlanTerminalInputTrace {
    private let environment: [String: String]
    private let defaults: UserDefaults
    private let fileManager: FileManager
    private let lock = NSLock()
    private let timestampFormatter: ISO8601DateFormatter
    private let configRefreshInterval: TimeInterval = 0.5
    private var cachedConfig: AlanTerminalInputTraceConfig?
    private var lastConfigRefresh = Date.distantPast

    var isEnabled: Bool {
        currentConfig().isEnabled
    }

    init(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        defaults: UserDefaults = .standard,
        fileManager: FileManager = .default
    ) {
        self.environment = environment
        self.defaults = defaults
        self.fileManager = fileManager
        timestampFormatter = ISO8601DateFormatter()
        timestampFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    }

    func log(_ message: @autoclosure () -> String) {
        let config = currentConfig()
        guard config.isEnabled, let fileURL = config.fileURL else { return }

        let line = "\(timestampFormatter.string(from: Date())) \(message())\n"
        guard let data = line.data(using: .utf8) else { return }

        lock.lock()
        defer { lock.unlock() }

        do {
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            if !FileManager.default.fileExists(atPath: fileURL.path) {
                FileManager.default.createFile(atPath: fileURL.path, contents: nil)
            }
            let handle = try FileHandle(forWritingTo: fileURL)
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
        } catch {
            // Tracing is diagnostic-only and must never affect terminal input delivery.
        }
    }

    private func currentConfig() -> AlanTerminalInputTraceConfig {
        let now = Date()

        lock.lock()
        defer { lock.unlock() }

        if let cachedConfig,
           now.timeIntervalSince(lastConfigRefresh) < configRefreshInterval
        {
            return cachedConfig
        }

        defaults.synchronize()
        let config = Self.resolveConfig(
            environment: environment,
            defaults: defaults,
            fileManager: fileManager
        )
        cachedConfig = config
        lastConfigRefresh = now
        return config
    }

    private static func resolveConfig(
        environment: [String: String],
        defaults: UserDefaults,
        fileManager: FileManager
    ) -> AlanTerminalInputTraceConfig {
        AlanTerminalInputTraceConfig(
            isEnabled: isTruthy(environment["ALAN_TERMINAL_INPUT_TRACE"])
                || defaults.bool(forKey: "AlanTerminalInputTraceEnabled"),
            fileURL: resolveFileURL(
                environment: environment,
                defaults: defaults,
                fileManager: fileManager
            )
        )
    }

    private static func resolveFileURL(
        environment: [String: String],
        defaults: UserDefaults,
        fileManager: FileManager
    ) -> URL? {
        if let override = environment["ALAN_TERMINAL_INPUT_TRACE_PATH"],
           !override.isEmpty {
            return URL(fileURLWithPath: expandingHomeDirectory(in: override))
        }
        if let override = defaults.string(forKey: "AlanTerminalInputTracePath"),
           !override.isEmpty {
            return URL(fileURLWithPath: expandingHomeDirectory(in: override))
        }

        guard let libraryURL = fileManager.urls(for: .libraryDirectory, in: .userDomainMask).first else {
            return nil
        }
        return libraryURL
            .appendingPathComponent("Logs", isDirectory: true)
            .appendingPathComponent("Alan", isDirectory: true)
            .appendingPathComponent("terminal-input-trace.log", isDirectory: false)
    }

    private static func expandingHomeDirectory(in path: String) -> String {
        guard path == "~" || path.hasPrefix("~/") else { return path }
        return NSHomeDirectory() + String(path.dropFirst())
    }

    private static func isTruthy(_ value: String?) -> Bool {
        guard let value = value?.lowercased() else { return false }
        return value == "1" || value == "true" || value == "yes" || value == "on"
    }
}
#endif
