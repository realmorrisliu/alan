import Foundation

#if os(macOS)
final class AlanShellControlFilePoller: @unchecked Sendable {
    private let windowID: String
    private let fileManager: FileManager
    private let channel: AlanInstallChannel
    private let commandsURL: URL
    private let resultsURL: URL
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder
    private let commandHandler: @MainActor (AlanShellControlCommand) -> AlanShellControlResponse
    private let bindingProjectionHandler: @MainActor (String, ShellAlanBinding?) -> Void
    private let diagnosticHandler: @MainActor (String) -> Void
    private let pollQueue: DispatchQueue
    private var pollSource: DispatchSourceTimer?
    private var trackedPaneIDs: Set<String> = []
    private var lastBindingPayloadByPaneID: [String: Data] = [:]
    private var bindingURLByPaneID: [String: URL] = [:]
    private var commandFilesInFlight: Set<URL> = []

    init(
        windowID: String,
        fileManager: FileManager,
        channel: AlanInstallChannel = .current(),
        commandsURL: URL,
        resultsURL: URL,
        encoder: JSONEncoder,
        decoder: JSONDecoder,
        commandHandler: @escaping @MainActor (AlanShellControlCommand) -> AlanShellControlResponse,
        bindingProjectionHandler: @escaping @MainActor (String, ShellAlanBinding?) -> Void,
        diagnosticHandler: @escaping @MainActor (String) -> Void
    ) {
        self.windowID = windowID
        self.fileManager = fileManager
        self.channel = channel
        self.commandsURL = commandsURL
        self.resultsURL = resultsURL
        self.encoder = encoder
        self.decoder = decoder
        self.commandHandler = commandHandler
        self.bindingProjectionHandler = bindingProjectionHandler
        self.diagnosticHandler = diagnosticHandler
        self.pollQueue = DispatchQueue(label: "dev.alan.shell.control.poll", qos: .utility)
    }

    deinit {
        pollSource?.cancel()
    }

    func start() {
        stop()
        let source = DispatchSource.makeTimerSource(queue: pollQueue)
        source.schedule(deadline: .now() + .milliseconds(250), repeating: .milliseconds(250), leeway: .milliseconds(100))
        source.setEventHandler { [weak self] in
            self?.pollCommandsOnPollQueue()
            self?.pollBindingsOnPollQueue()
        }
        source.resume()
        pollSource = source
    }

    func stop() {
        pollSource?.cancel()
        pollSource = nil
    }

    @MainActor
    func pollCommandsOnce() {
        pollCommands()
    }

    func updateTrackedPaneIDs(_ paneIDs: Set<String>) {
        pollQueue.async { [weak self] in
            guard let self else { return }
            self.trackedPaneIDs = paneIDs
            let stalePaneIDs = Set(self.lastBindingPayloadByPaneID.keys).subtracting(paneIDs)
            for paneID in stalePaneIDs {
                self.lastBindingPayloadByPaneID.removeValue(forKey: paneID)
                self.bindingURLByPaneID.removeValue(forKey: paneID)
            }
            let staleCachedPaneIDs = Set(self.bindingURLByPaneID.keys).subtracting(paneIDs)
            for paneID in staleCachedPaneIDs {
                self.bindingURLByPaneID.removeValue(forKey: paneID)
            }
        }
    }

    @MainActor
    private func pollCommands() {
        ensurePollingDirectories()

        let commandFiles: [URL]
        do {
            commandFiles = try fileManager.contentsOfDirectory(
                at: commandsURL,
                includingPropertiesForKeys: [.creationDateKey, .contentModificationDateKey],
                options: [.skipsHiddenFiles]
            )
            .filter { $0.pathExtension == "json" }
            .sorted(by: compareCommandFiles)
        } catch {
            diagnosticHandler("Failed to read shell command directory: \(error.localizedDescription)")
            return
        }

        for fileURL in commandFiles {
            handleCommandFile(at: fileURL)
        }
    }

    @MainActor
    private func ensurePollingDirectories() {
        for url in [commandsURL, resultsURL] {
            do {
                try fileManager.createDirectory(at: url, withIntermediateDirectories: true)
            } catch {
                diagnosticHandler("Failed to create shell polling directory \(url.path): \(error.localizedDescription)")
            }
        }
    }

    @MainActor
    private func handleCommandFile(at fileURL: URL) {
        guard let data = try? Data(contentsOf: fileURL),
              let command = try? decoder.decode(AlanShellControlCommand.self, from: data)
        else {
            diagnosticHandler("Ignored unreadable shell command file \(fileURL.lastPathComponent).")
            do {
                try fileManager.removeItem(at: fileURL)
            } catch {
                diagnosticHandler("Failed to remove unreadable shell command file \(fileURL.lastPathComponent): \(error.localizedDescription)")
            }
            return
        }

        let response = commandHandler(command)
        let responseURL = resultsURL.appendingPathComponent("\(command.requestID).json")

        do {
            let responseData = try encoder.encode(response)
            try responseData.write(to: responseURL, options: .atomic)
        } catch {
            diagnosticHandler("Failed to write shell command result \(responseURL.lastPathComponent): \(error.localizedDescription)")
        }

        do {
            try fileManager.removeItem(at: fileURL)
        } catch {
            diagnosticHandler("Failed to remove processed shell command file \(fileURL.lastPathComponent): \(error.localizedDescription)")
        }
    }

    @MainActor
    private func compareCommandFiles(_ lhs: URL, _ rhs: URL) -> Bool {
        let lhsValues = try? lhs.resourceValues(forKeys: [.creationDateKey, .contentModificationDateKey])
        let rhsValues = try? rhs.resourceValues(forKeys: [.creationDateKey, .contentModificationDateKey])
        let lhsDate = lhsValues?.creationDate ?? lhsValues?.contentModificationDate ?? .distantPast
        let rhsDate = rhsValues?.creationDate ?? rhsValues?.contentModificationDate ?? .distantPast

        if lhsDate != rhsDate {
            return lhsDate < rhsDate
        }

        return lhs.lastPathComponent < rhs.lastPathComponent
    }

    private func pollCommandsOnPollQueue() {
        ensurePollingDirectoriesOnPollQueue()

        let commandFiles: [URL]
        do {
            commandFiles = try fileManager.contentsOfDirectory(
                at: commandsURL,
                includingPropertiesForKeys: [.creationDateKey, .contentModificationDateKey],
                options: [.skipsHiddenFiles]
            )
            .filter { $0.pathExtension == "json" }
            .sorted(by: compareCommandFilesOnPollQueue)
        } catch {
            recordDiagnosticFromPollQueue("Failed to read shell command directory: \(error.localizedDescription)")
            return
        }

        for fileURL in commandFiles where !commandFilesInFlight.contains(fileURL) {
            handleCommandFileOnPollQueue(at: fileURL)
        }
    }

    private func ensurePollingDirectoriesOnPollQueue() {
        for url in [commandsURL, resultsURL] {
            do {
                try fileManager.createDirectory(at: url, withIntermediateDirectories: true)
            } catch {
                recordDiagnosticFromPollQueue("Failed to create shell polling directory \(url.path): \(error.localizedDescription)")
            }
        }
    }

    private func handleCommandFileOnPollQueue(at fileURL: URL) {
        guard let data = try? Data(contentsOf: fileURL),
              let command = try? decoder.decode(AlanShellControlCommand.self, from: data)
        else {
            recordDiagnosticFromPollQueue("Ignored unreadable shell command file \(fileURL.lastPathComponent).")
            do {
                try fileManager.removeItem(at: fileURL)
            } catch {
                recordDiagnosticFromPollQueue("Failed to remove unreadable shell command file \(fileURL.lastPathComponent): \(error.localizedDescription)")
            }
            return
        }

        commandFilesInFlight.insert(fileURL)
        Task { @MainActor [weak self] in
            guard let self else { return }
            let response = self.commandHandler(command)
            let responseData = try? self.encoder.encode(response)
            self.pollQueue.async { [weak self] in
                self?.writeCommandResponseOnPollQueue(
                    responseData,
                    requestID: command.requestID,
                    fileURL: fileURL
                )
            }
        }
    }

    private func writeCommandResponseOnPollQueue(
        _ responseData: Data?,
        requestID: String,
        fileURL: URL
    ) {
        defer { commandFilesInFlight.remove(fileURL) }
        let responseURL = resultsURL.appendingPathComponent("\(requestID).json")

        do {
            guard let responseData else {
                throw CocoaError(.fileWriteUnknown)
            }
            try responseData.write(to: responseURL, options: .atomic)
        } catch {
            recordDiagnosticFromPollQueue("Failed to write shell command result \(responseURL.lastPathComponent): \(error.localizedDescription)")
        }

        do {
            try fileManager.removeItem(at: fileURL)
        } catch {
            recordDiagnosticFromPollQueue("Failed to remove processed shell command file \(fileURL.lastPathComponent): \(error.localizedDescription)")
        }
    }

    private func compareCommandFilesOnPollQueue(_ lhs: URL, _ rhs: URL) -> Bool {
        let lhsValues = try? lhs.resourceValues(forKeys: [.creationDateKey, .contentModificationDateKey])
        let rhsValues = try? rhs.resourceValues(forKeys: [.creationDateKey, .contentModificationDateKey])
        let lhsDate = lhsValues?.creationDate ?? lhsValues?.contentModificationDate ?? .distantPast
        let rhsDate = rhsValues?.creationDate ?? rhsValues?.contentModificationDate ?? .distantPast

        if lhsDate != rhsDate {
            return lhsDate < rhsDate
        }

        return lhs.lastPathComponent < rhs.lastPathComponent
    }

    @MainActor
    private func pollBindings() {
        for paneID in trackedPaneIDs.sorted() {
            let bindingURL = cachedBindingURL(for: paneID)

            guard fileManager.fileExists(atPath: bindingURL.path) else {
                if lastBindingPayloadByPaneID.removeValue(forKey: paneID) != nil {
                    bindingProjectionHandler(paneID, nil)
                }
                continue
            }

            guard let data = try? Data(contentsOf: bindingURL) else {
                diagnosticHandler("Failed to read alan binding file for \(paneID).")
                continue
            }

            if lastBindingPayloadByPaneID[paneID] == data {
                continue
            }

            guard let projection = try? decoder.decode(AlanShellBindingProjection.self, from: data) else {
                lastBindingPayloadByPaneID[paneID] = data
                diagnosticHandler("Ignored invalid alan binding file for \(paneID).")
                continue
            }

            lastBindingPayloadByPaneID[paneID] = data
            bindingProjectionHandler(paneID, projection.shellBinding)
        }
    }

    private func pollBindingsOnPollQueue() {
        for paneID in trackedPaneIDs.sorted() {
            let bindingURL = cachedBindingURL(for: paneID)

            guard fileManager.fileExists(atPath: bindingURL.path) else {
                if lastBindingPayloadByPaneID.removeValue(forKey: paneID) != nil {
                    projectBindingFromPollQueue(paneID: paneID, shellBinding: nil)
                }
                continue
            }

            guard let data = try? Data(contentsOf: bindingURL) else {
                recordDiagnosticFromPollQueue("Failed to read alan binding file for \(paneID).")
                continue
            }

            if lastBindingPayloadByPaneID[paneID] == data {
                continue
            }

            guard let projection = try? decoder.decode(AlanShellBindingProjection.self, from: data) else {
                lastBindingPayloadByPaneID[paneID] = data
                recordDiagnosticFromPollQueue("Ignored invalid alan binding file for \(paneID).")
                continue
            }

            lastBindingPayloadByPaneID[paneID] = data
            projectBindingFromPollQueue(paneID: paneID, shellBinding: projection.shellBinding)
        }
    }

    private func cachedBindingURL(for paneID: String) -> URL {
        if let url = bindingURLByPaneID[paneID] {
            return url
        }
        let url = alanShellBindingFileURL(
            windowID: windowID,
            paneID: paneID,
            fileManager: fileManager,
            channel: channel
        )
        bindingURLByPaneID[paneID] = url
        return url
    }

    private func recordDiagnosticFromPollQueue(_ message: String) {
        Task { @MainActor [diagnosticHandler] in
            diagnosticHandler(message)
        }
    }

    private func projectBindingFromPollQueue(paneID: String, shellBinding: ShellAlanBinding?) {
        Task { @MainActor [bindingProjectionHandler] in
            bindingProjectionHandler(paneID, shellBinding)
        }
    }
}
#endif
