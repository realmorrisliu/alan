import Foundation

#if os(macOS)
@MainActor
final class AlanShellControlFilePoller {
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
    private var pollSource: DispatchSourceTimer?
    private var trackedPaneIDs: Set<String> = []
    private var lastBindingPayloadByPaneID: [String: Data] = [:]
    private var bindingURLByPaneID: [String: URL] = [:]

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
    }

    deinit {
        pollSource?.cancel()
    }

    func start() {
        stop()
        let source = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "dev.alan.shell.control.poll"))
        source.schedule(deadline: .now() + .milliseconds(250), repeating: .milliseconds(250), leeway: .milliseconds(100))
        source.setEventHandler { [weak self] in
            Task { @MainActor [weak self] in
                self?.pollCommands()
                self?.pollBindings()
            }
        }
        source.resume()
        pollSource = source
    }

    func stop() {
        pollSource?.cancel()
        pollSource = nil
    }

    func pollCommandsOnce() {
        pollCommands()
    }

    func updateTrackedPaneIDs(_ paneIDs: Set<String>) {
        trackedPaneIDs = paneIDs
        let stalePaneIDs = Set(lastBindingPayloadByPaneID.keys).subtracting(paneIDs)
        for paneID in stalePaneIDs {
            lastBindingPayloadByPaneID.removeValue(forKey: paneID)
            bindingURLByPaneID.removeValue(forKey: paneID)
        }
        let staleCachedPaneIDs = Set(bindingURLByPaneID.keys).subtracting(paneIDs)
        for paneID in staleCachedPaneIDs {
            bindingURLByPaneID.removeValue(forKey: paneID)
        }
    }

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

    private func ensurePollingDirectories() {
        for url in [commandsURL, resultsURL] {
            do {
                try fileManager.createDirectory(at: url, withIntermediateDirectories: true)
            } catch {
                diagnosticHandler("Failed to create shell polling directory \(url.path): \(error.localizedDescription)")
            }
        }
    }

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
}
#endif
