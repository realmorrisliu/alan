import Foundation

#if os(macOS)
enum ShellAutomationCommand: Equatable {
    case createTab(ShellAutomationCreateTabRequest)
    case splitPane(ShellAutomationPaneSplitRequest)
    case focusPane(paneID: String)
    case closePane(paneID: String)
    case closeTab(tabID: String)
    case sendText(ShellAutomationSendTextRequest)
    case readPaneSummary(paneID: String)
    case activateAttentionItem(paneID: String)
}

struct ShellAutomationCreateTabRequest: Equatable {
    let launchTarget: ShellLaunchTarget
    let spaceID: String?
    let title: String?
    let workingDirectory: String?
}

struct ShellAutomationPaneSplitRequest: Equatable {
    let paneID: String
    let placement: ShellPaneSplitDirection
}

struct ShellAutomationSendTextRequest: Equatable {
    let paneID: String
    let terminalContentID: String?
    let text: String

    init(paneID: String, terminalContentID: String? = nil, text: String) {
        self.paneID = paneID
        self.terminalContentID = terminalContentID
        self.text = text
    }
}

enum ShellAutomationCommandResultCode: String, Codable, Equatable {
    case accepted
    case queued
    case rejected
    case missingTarget = "missing_target"
    case invalidRequest = "invalid_request"
    case unsupportedContent = "unsupported_content"
    case runtimeUnavailable = "runtime_unavailable"
    case requiresConfirmation = "requires_confirmation"
    case timeout
    case lastPane = "last_pane"
    case lastTab = "last_tab"
}

struct ShellAutomationCommandResult: Equatable {
    let code: ShellAutomationCommandResultCode
    let summary: ShellAutomationPaneSummary?
    let spaceID: String?
    let tabID: String?
    let paneID: String?
    let acceptedBytes: Int?
    let deliveryCode: String?
    let runtimePhase: String?
    let errorCode: String?
    let errorMessage: String?

    init(
        code: ShellAutomationCommandResultCode,
        summary: ShellAutomationPaneSummary?,
        spaceID: String? = nil,
        tabID: String? = nil,
        paneID: String? = nil,
        acceptedBytes: Int? = nil,
        deliveryCode: String? = nil,
        runtimePhase: String? = nil,
        errorCode: String? = nil,
        errorMessage: String? = nil
    ) {
        self.code = code
        self.summary = summary
        self.spaceID = spaceID
        self.tabID = tabID
        self.paneID = paneID
        self.acceptedBytes = acceptedBytes
        self.deliveryCode = deliveryCode
        self.runtimePhase = runtimePhase
        self.errorCode = errorCode
        self.errorMessage = errorMessage
    }

    var applied: Bool {
        code == .accepted
    }
}

struct ShellAutomationPaneSummary: Equatable {
    let windowID: String
    let spaceID: String
    let spaceTitle: String
    let tabID: String
    let tabTitle: String
    let paneID: String
    let paneTitle: String
    let workingDirectory: String?
    let processProgram: String?
    let processState: String?
    let attention: ShellAttentionState

    var displayText: String {
        [
            paneTitle,
            spaceTitle,
            tabTitle,
            workingDirectory,
            processProgram,
            processState,
            attention.rawValue,
        ]
        .compactMap { value in
            let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed?.isEmpty == false ? trimmed : nil
        }
        .joined(separator: " - ")
    }
}

@MainActor
protocol ShellAutomationCommandHandling: AnyObject {
    func performShellAutomationCommand(
        _ command: ShellAutomationCommand
    ) -> ShellAutomationCommandResult
}

extension ShellStateSnapshot {
    func automationPaneSummary(paneID: String) -> ShellAutomationPaneSummary? {
        guard let pane = pane(paneID: paneID),
              let tab = tab(tabID: pane.tabID),
              let space = space(spaceID: pane.spaceID)
        else {
            return nil
        }

        return ShellAutomationPaneSummary(
            windowID: windowID,
            spaceID: pane.spaceID,
            spaceTitle: firstNonEmptyDisplayTitle([space.title], fallback: "Space"),
            tabID: pane.tabID,
            tabTitle: firstNonEmptyDisplayTitle([tab.title], fallback: "Tab"),
            paneID: pane.paneID,
            paneTitle: firstNonEmptyDisplayTitle(
                [pane.viewport?.title, pane.context?.displayName, pane.process?.program],
                fallback: "Pane"
            ),
            workingDirectory: pane.cwd,
            processProgram: pane.process?.program,
            processState: pane.context?.processState,
            attention: pane.attention
        )
    }

    private func firstNonEmptyDisplayTitle(_ candidates: [String?], fallback: String) -> String {
        for candidate in candidates {
            guard let trimmed = candidate?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !trimmed.isEmpty
            else {
                continue
            }
            return trimmed
        }
        return fallback
    }
}
#endif
