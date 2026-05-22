import SwiftUI

#if os(macOS)
@MainActor
protocol TerminalRuntimeHandle: AnyObject {
    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult
    func teardownTerminalRuntime()
}

@MainActor
final class MockTerminalRuntimeHandle: TerminalRuntimeHandle {
    private(set) var attachedCount = 0
    private(set) var detachedCount = 0
    private(set) var teardownCount = 0
    private(set) var deliveredText: [String] = []
    var deliveryResult: TerminalRuntimeDeliveryResult?

    func attach() {
        attachedCount += 1
    }

    func detach() {
        detachedCount += 1
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        deliveredText.append(text)
        return deliveryResult ?? .accepted(byteCount: text.lengthOfBytes(using: .utf8))
    }

    func teardownTerminalRuntime() {
        teardownCount += 1
    }
}

struct TerminalContentMount: Equatable {
    let contentID: String
    let paneSlotID: String
    let tabID: String
    let spaceID: String

    init(contentID: String, paneSlotID: String, tabID: String, spaceID: String) {
        self.contentID = contentID
        self.paneSlotID = paneSlotID
        self.tabID = tabID
        self.spaceID = spaceID
    }

    init(pane: ShellPane) {
        self.init(
            contentID: pane.terminalContentID,
            paneSlotID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID
        )
    }
}

@MainActor
final class TerminalRuntimeRegistry: ObservableObject {
    typealias MockDeliveryHandler = (String, String) -> TerminalRuntimeDeliveryResult

    private var hostViewsByContentID: [String: AlanTerminalHostNSView] = [:]
    private var snapshotsByContentID: [String: TerminalHostRuntimeSnapshot] = [:]
    private var paneSlotIDByContentID: [String: String] = [:]
    private var contentIDByPaneSlotID: [String: String] = [:]
    private let runtimeService: AlanTerminalRuntimeService
    private let mockDeliveryHandler: MockDeliveryHandler?

    init(
        runtimeService: AlanTerminalRuntimeService? = nil,
        mockDeliveryHandler: MockDeliveryHandler? = nil
    ) {
        self.runtimeService = runtimeService ?? AlanWindowTerminalRuntimeService()
        self.mockDeliveryHandler = mockDeliveryHandler
    }

    func hostView(
        for pane: ShellPane?,
        bootProfile: AlanShellBootProfile?,
        isSelected: Bool,
        activationDelegate: TerminalHostActivationDelegate?,
        onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?,
        onCommandInput: (() -> Void)?,
        onCloseRequest: ((Bool) -> Void)?,
        onRuntimeUpdate: @escaping (TerminalHostRuntimeSnapshot) -> Void,
        onMetadataUpdate: @escaping (TerminalPaneMetadataSnapshot) -> Void
    ) -> AlanTerminalHostNSView {
        hostView(
            forTerminalContent: pane.map(TerminalContentMount.init(pane:)),
            pane: pane,
            bootProfile: bootProfile,
            isSelected: isSelected,
            activationDelegate: activationDelegate,
            onShellAction: onShellAction,
            onCommandInput: onCommandInput,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
    }

    func hostView(
        forTerminalContent mount: TerminalContentMount?,
        pane: ShellPane?,
        bootProfile: AlanShellBootProfile?,
        isSelected: Bool,
        activationDelegate: TerminalHostActivationDelegate?,
        onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?,
        onCommandInput: (() -> Void)?,
        onCloseRequest: ((Bool) -> Void)?,
        onRuntimeUpdate: @escaping (TerminalHostRuntimeSnapshot) -> Void,
        onMetadataUpdate: @escaping (TerminalPaneMetadataSnapshot) -> Void
    ) -> AlanTerminalHostNSView {
        let hostView: AlanTerminalHostNSView
        if let mount {
            recordMount(contentID: mount.contentID, paneSlotID: mount.paneSlotID)
            if let existing = hostViewsByContentID[mount.contentID] {
                hostView = existing
            } else {
                let created = AlanTerminalHostNSView()
                hostViewsByContentID[mount.contentID] = created
                hostView = created
            }
        } else {
            hostView = AlanTerminalHostNSView()
        }

        let surfaceHandle: AlanTerminalSurfaceHandle?
        if let mount {
            surfaceHandle = runtimeService.surfaceHandle(
                forTerminalContentID: mount.contentID,
                mountedAtPaneID: mount.paneSlotID,
                bootProfile: bootProfile
            )
        } else {
            surfaceHandle = nil
        }

        hostView.configure(
            pane: pane,
            terminalContentID: mount?.contentID,
            bootProfile: bootProfile,
            isSelected: isSelected,
            surfaceHandle: surfaceHandle,
            activationDelegate: activationDelegate,
            onShellAction: onShellAction,
            onCommandInput: onCommandInput,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
        return hostView
    }

    func surfaceHandle(
        for pane: ShellPane?,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle? {
        guard let pane else { return nil }
        return surfaceHandle(
            forTerminalContent: TerminalContentMount(pane: pane),
            bootProfile: bootProfile
        )
    }

    func surfaceHandle(
        forTerminalContent mount: TerminalContentMount,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        recordMount(contentID: mount.contentID, paneSlotID: mount.paneSlotID)
        return runtimeService.surfaceHandle(
            forTerminalContentID: mount.contentID,
            mountedAtPaneID: mount.paneSlotID,
            bootProfile: bootProfile
        )
    }

    func updateSnapshot(_ snapshot: TerminalHostRuntimeSnapshot) {
        guard let contentID = snapshot.contentID ?? snapshot.paneID.map(terminalContentID(forPaneID:)) else {
            return
        }
        if let paneID = snapshot.paneID {
            recordMount(contentID: contentID, paneSlotID: paneID)
        }
        snapshotsByContentID[contentID] = snapshot
        runtimeService
            .existingSurfaceHandle(forTerminalContentID: contentID)?
            .updateHostRuntimeSnapshot(snapshot)
    }

    func snapshot(for paneID: String?) -> TerminalHostRuntimeSnapshot {
        guard let paneID else { return .placeholder }
        return snapshot(forTerminalContentID: terminalContentID(mountedAtPaneID: paneID))
    }

    func snapshot(forTerminalContentID contentID: String?) -> TerminalHostRuntimeSnapshot {
        guard let contentID else { return .placeholder }
        return snapshotsByContentID[contentID]
            ?? runtimeSnapshot(from: runtimeService.snapshot(forTerminalContentID: contentID))
    }

    func releaseRuntimes(excluding activePaneIDs: Set<String>) {
        let activeContentIDs = Set(activePaneIDs.map { terminalContentID(mountedAtPaneID: $0) })
        let staleContentIDs = registeredContentIDs.subtracting(activeContentIDs)
        staleContentIDs.forEach { releaseTerminalContent($0) }
    }

    func releaseRuntime(for paneID: String) {
        releaseTerminalContent(terminalContentID(mountedAtPaneID: paneID))
    }

    func releaseAllRuntimes() {
        registeredContentIDs.forEach { releaseTerminalContent($0) }
    }

    func sendText(to paneID: String, text: String) -> TerminalRuntimeDeliveryResult {
        sendText(toTerminalContentID: terminalContentID(mountedAtPaneID: paneID), text: text)
    }

    func sendText(
        toTerminalContentID contentID: String,
        text: String
    ) -> TerminalRuntimeDeliveryResult {
        if let mockDeliveryHandler {
            return mockDeliveryHandler(contentID, text)
        }

        return runtimeService.sendText(toTerminalContentID: contentID, text: text)
    }

    func terminalCommandRuntimeState(for paneID: String) -> ShellTerminalCommandRuntimeState {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.terminalCommandRuntimeState
        }

        let surfaceHandle = runtimeService.existingSurfaceHandle(forTerminalContentID: contentID)
        let selectionEngine = surfaceHandle as? AlanTerminalSelectionEngine
        let searchEngine = surfaceHandle as? AlanTerminalSearchEngine
        let snapshot = snapshotsByContentID[contentID]
        return ShellTerminalCommandRuntimeState(
            paneID: paneID,
            hasSelection: selectionEngine?.hasSelection() ?? false,
            inputReady: surfaceHandle?.isSurfaceReady ?? snapshot?.surfaceState.inputReady ?? false,
            searchAvailable: searchEngine != nil,
            hasReliableSemanticCommands: snapshot?.surfaceState.semanticCommands.hasReliableCommandBoundaries ?? false
        )
    }

    @discardableResult
    func copySelection(for paneID: String) -> Bool {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.copySelection()
        }
        return copySelection(
            for: paneID,
            to: AlanTerminalSystemPasteboardWriter(pasteboard: .general)
        )
    }

    @discardableResult
    func copySelection(for paneID: String, to writer: AlanTerminalPasteboardWriting) -> Bool {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.copySelection(to: writer)
        }
        guard let selectionEngine = runtimeService
            .existingSurfaceHandle(forTerminalContentID: contentID) as? AlanTerminalSelectionEngine,
              let selectedText = selectionEngine.readSelectionText(),
              !selectedText.isEmpty
        else {
            return false
        }
        return writer.writeString(selectedText)
    }

    @discardableResult
    func pasteText(_ text: String, to paneID: String) -> TerminalRuntimeDeliveryResult {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.pasteText(text)
        }
        return sendText(to: paneID, text: text)
    }

    @discardableResult
    func beginFindInteraction(for paneID: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .beginFindInteraction() ?? false
    }

    @discardableResult
    func beginLastCommandOutputSearch(for paneID: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .beginLastCommandOutputSearch() ?? false
    }

    @discardableResult
    func navigateSemanticPrompt(
        for paneID: String,
        direction: AlanTerminalPromptNavigationDirection
    ) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .navigateSemanticPrompt(direction) ?? false
    }

    @discardableResult
    func copyLastCommandOutput(for paneID: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .copyLastCommandOutput() ?? false
    }

    @discardableResult
    func updateFindQuery(for paneID: String, query: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .updateFindQuery(query) ?? false
    }

    func selectNextFindMatch(for paneID: String) {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?.selectNextFindMatch()
    }

    func selectPreviousFindMatch(for paneID: String) {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?.selectPreviousFindMatch()
    }

    func dismissFindInteraction(for paneID: String, refocusTerminal: Bool = true) {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .dismissFindInteraction(refocusTerminal: refocusTerminal)
    }

    func requestFocus(for paneID: String) {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?.focusTerminal()
    }

    var registeredContentIDs: Set<String> {
        Set(hostViewsByContentID.keys)
            .union(snapshotsByContentID.keys)
            .union(runtimeService.registeredContentIDs)
    }

    var registeredPaneIDs: Set<String> {
        Set(paneSlotIDByContentID.values)
            .union(snapshotsByContentID.values.compactMap(\.paneID))
            .union(runtimeService.registeredPaneIDs)
    }

    private func releaseTerminalContent(_ contentID: String) {
        if let hostView = hostViewsByContentID.removeValue(forKey: contentID) {
            hostView.teardownTerminalRuntime()
        }
        runtimeService.finalizeTerminalContent(contentID)
        snapshotsByContentID.removeValue(forKey: contentID)
        if let paneSlotID = paneSlotIDByContentID.removeValue(forKey: contentID),
           contentIDByPaneSlotID[paneSlotID] == contentID
        {
            contentIDByPaneSlotID.removeValue(forKey: paneSlotID)
        }
    }

    private func terminalContentID(forPaneID paneID: String) -> String {
        ShellContentInstance.terminalContentID(forPaneID: paneID)
    }

    private func terminalContentID(mountedAtPaneID paneID: String) -> String {
        contentIDByPaneSlotID[paneID] ?? terminalContentID(forPaneID: paneID)
    }

    private func recordMount(contentID: String, paneSlotID: String) {
        if let previousPaneSlotID = paneSlotIDByContentID[contentID],
           previousPaneSlotID != paneSlotID,
           contentIDByPaneSlotID[previousPaneSlotID] == contentID
        {
            contentIDByPaneSlotID.removeValue(forKey: previousPaneSlotID)
        }
        if let previousContentID = contentIDByPaneSlotID[paneSlotID],
           previousContentID != contentID
        {
            paneSlotIDByContentID.removeValue(forKey: previousContentID)
        }
        paneSlotIDByContentID[contentID] = paneSlotID
        contentIDByPaneSlotID[paneSlotID] = contentID
    }

    private func runtimeSnapshot(
        from surfaceSnapshot: AlanTerminalSurfaceSnapshot?
    ) -> TerminalHostRuntimeSnapshot {
        guard let surfaceSnapshot else { return .placeholder }
        return TerminalHostRuntimeSnapshot(
            stage: .scaffold,
            contentID: surfaceSnapshot.contentID,
            paneID: surfaceSnapshot.paneID,
            tabID: nil,
            logicalSize: .zero,
            backingSize: .zero,
            displayName: nil,
            displayID: nil,
            attachedWindowTitle: nil,
            isFocused: false,
            renderer: surfaceSnapshot.renderer,
            paneMetadata: surfaceSnapshot.metadata,
            surfaceState: .placeholder,
            lastUpdatedAt: surfaceSnapshot.lastUpdatedAt
        )
    }
}
#endif
