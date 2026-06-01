import Foundation

#if os(macOS) && canImport(AppIntents)
import AppIntents

enum ShellAutomationIntentAvailability {
    static let minimumSupportedMacOS = "macOS 13.0"
    static let fallbackDescription =
        "When App Intents are unavailable, use alan's shell control plane or native shell UI commands."
}

struct ShellAutomationIntentOutcome: Equatable, Sendable {
    let code: ShellAutomationCommandResultCode
    let dialog: String
    let summary: ShellAutomationPaneSummary?
    let acceptedBytes: Int?
    let deliveryCode: String?

    init(
        command: ShellAutomationCommand,
        result: ShellAutomationCommandResult
    ) {
        code = result.code
        summary = result.summary
        acceptedBytes = result.acceptedBytes
        deliveryCode = result.deliveryCode
        dialog = ShellAutomationIntentOutcome.dialog(for: command, result: result)
    }

    func requireSuccessfulIntentResult() throws {
        guard code == .accepted else {
            throw ShellAutomationIntentError(code: code, dialog: dialog)
        }
    }

    private static func dialog(
        for command: ShellAutomationCommand,
        result: ShellAutomationCommandResult
    ) -> String {
        switch result.code {
        case .accepted:
            return acceptedDialog(for: command, summary: result.summary)
        case .queued:
            return deliveryDialog(prefix: "Text delivery queued", result: result)
        case .rejected:
            return "Command rejected."
        case .missingTarget:
            return "Missing shell target."
        case .invalidRequest:
            return "Invalid shell request."
        case .unsupportedContent:
            return "Unsupported shell content."
        case .runtimeUnavailable:
            return "Terminal runtime unavailable."
        case .requiresConfirmation:
            return "Close requires confirmation."
        case .timeout:
            return "Shell command timed out."
        case .lastPane:
            return "Cannot close the last pane."
        case .lastTab:
            return "Cannot close the last tab."
        }
    }

    private static func acceptedDialog(
        for command: ShellAutomationCommand,
        summary: ShellAutomationPaneSummary?
    ) -> String {
        let safeSummary = summary.map { ": \($0.displayText)" } ?? "."
        switch command {
        case .createTab:
            return "Created terminal tab\(safeSummary)"
        case .splitPane:
            return "Split pane\(safeSummary)"
        case .focusPane:
            return "Focused pane\(safeSummary)"
        case .closePane:
            return "Closed pane."
        case .closeTab:
            return "Closed tab."
        case .sendText:
            return deliveryDialog(prefix: "Text delivery accepted", result: nil)
        case .sendKey:
            return deliveryDialog(prefix: "Key delivery accepted", result: nil)
        case .readPaneSummary:
            return "Pane summary\(safeSummary)"
        case .activateAttentionItem:
            return "Opened attention item\(safeSummary)"
        }
    }

    private static func deliveryDialog(
        prefix: String,
        result: ShellAutomationCommandResult?
    ) -> String {
        let byteSuffix = result?.acceptedBytes.map { " (\($0) bytes)" } ?? ""
        let codeSuffix = result?.deliveryCode.map { " [\($0)]" } ?? ""
        return "\(prefix)\(byteSuffix)\(codeSuffix)."
    }
}

struct ShellAutomationIntentError: LocalizedError, Equatable {
    let code: ShellAutomationCommandResultCode
    let dialog: String

    var errorDescription: String? {
        dialog
    }
}

@MainActor
enum ShellAutomationIntentStore {
    private static var commandHandler: ShellAutomationCommandHandling?

    static func install(commandHandler: ShellAutomationCommandHandling?) {
        self.commandHandler = commandHandler
    }

    static func reset() {
        commandHandler = nil
    }

    static func perform(_ command: ShellAutomationCommand) -> ShellAutomationCommandResult {
        guard let commandHandler else {
            return ShellAutomationCommandResult(
                code: .runtimeUnavailable,
                summary: nil,
                errorCode: "intent_handler_unavailable",
                errorMessage: "Shell automation command handler unavailable"
            )
        }
        return commandHandler.performShellAutomationCommand(command)
    }
}

@MainActor
enum ShellAutomationIntentRouter {
    static func createTerminalTab(
        spaceID: String? = nil,
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil
    ) -> ShellAutomationIntentOutcome {
        perform(.createTab(ShellAutomationCreateTabRequest(
            launchTarget: .shell,
            spaceID: spaceID,
            title: title,
            workingDirectory: workingDirectory,
            terminalProfileID: terminalProfileID
        )))
    }

    static func splitPane(
        _ pane: AlanShellPaneEntity,
        direction: ShellPaneSplitDirection,
        terminalProfileID: String? = nil
    ) -> ShellAutomationIntentOutcome {
        perform(.splitPane(ShellAutomationPaneSplitRequest(
            paneID: pane.id,
            placement: direction,
            terminalProfileID: terminalProfileID
        )))
    }

    static func focusPane(_ pane: AlanShellPaneEntity) -> ShellAutomationIntentOutcome {
        perform(.focusPane(paneID: pane.id))
    }

    static func closePane(_ pane: AlanShellPaneEntity) -> ShellAutomationIntentOutcome {
        perform(.closePane(paneID: pane.id))
    }

    static func closeTab(_ tab: AlanShellTabEntity) -> ShellAutomationIntentOutcome {
        perform(.closeTab(tabID: tab.id))
    }

    static func sendText(
        _ text: String,
        to pane: AlanShellPaneEntity
    ) -> ShellAutomationIntentOutcome {
        perform(.sendText(ShellAutomationSendTextRequest(paneID: pane.id, text: text)))
    }

    static func readPaneSummary(for pane: AlanShellPaneEntity) -> ShellAutomationIntentOutcome {
        perform(.readPaneSummary(paneID: pane.id))
    }

    static func openAttentionItem(
        _ attentionItem: AlanShellAttentionItemEntity
    ) -> ShellAutomationIntentOutcome {
        perform(.activateAttentionItem(paneID: attentionItem.paneID))
    }

    private static func perform(_ command: ShellAutomationCommand) -> ShellAutomationIntentOutcome {
        ShellAutomationIntentOutcome(
            command: command,
            result: ShellAutomationIntentStore.perform(command)
        )
    }
}

@available(macOS 13.0, *)
enum AlanShellPaneSplitDirectionOption: String, AppEnum {
    case left
    case right
    case up
    case down

    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Split Direction")
    static var caseDisplayRepresentations: [Self: DisplayRepresentation] = [
        .left: "Left",
        .right: "Right",
        .up: "Up",
        .down: "Down",
    ]

    var shellDirection: ShellPaneSplitDirection {
        switch self {
        case .left:
            return .left
        case .right:
            return .right
        case .up:
            return .up
        case .down:
            return .down
        }
    }
}

@available(macOS 13.0, *)
struct AlanCreateTerminalTabIntent: AppIntent {
    static var title: LocalizedStringResource = "Create Terminal Tab"
    static var description = IntentDescription("Create a terminal tab in alan.")

    @Parameter(title: "Space")
    var space: AlanShellSpaceEntity?

    @Parameter(title: "Title")
    var tabTitle: String?

    @Parameter(title: "Working Directory")
    var workingDirectory: String?

    @Parameter(title: "Terminal Profile ID")
    var terminalProfileID: String?

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.createTerminalTab(
            spaceID: space?.id,
            title: tabTitle,
            workingDirectory: workingDirectory,
            terminalProfileID: terminalProfileID
        )
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
struct AlanSplitPaneIntent: AppIntent {
    static var title: LocalizedStringResource = "Split Shell Pane"
    static var description = IntentDescription("Split an alan shell pane.")

    @Parameter(title: "Pane")
    var pane: AlanShellPaneEntity

    @Parameter(title: "Direction")
    var direction: AlanShellPaneSplitDirectionOption

    @Parameter(title: "Terminal Profile ID")
    var terminalProfileID: String?

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.splitPane(
            pane,
            direction: direction.shellDirection,
            terminalProfileID: terminalProfileID
        )
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
struct AlanFocusPaneIntent: AppIntent {
    static var title: LocalizedStringResource = "Focus Shell Pane"
    static var description = IntentDescription("Focus an alan shell pane.")

    @Parameter(title: "Pane")
    var pane: AlanShellPaneEntity

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.focusPane(pane)
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
struct AlanClosePaneIntent: AppIntent {
    static var title: LocalizedStringResource = "Close Shell Pane"
    static var description = IntentDescription("Close an alan shell pane.")

    @Parameter(title: "Pane")
    var pane: AlanShellPaneEntity

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.closePane(pane)
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
struct AlanCloseTabIntent: AppIntent {
    static var title: LocalizedStringResource = "Close Shell Tab"
    static var description = IntentDescription("Close an alan shell tab.")

    @Parameter(title: "Tab")
    var tab: AlanShellTabEntity

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.closeTab(tab)
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
struct AlanSendTextToPaneIntent: AppIntent {
    static var title: LocalizedStringResource = "Send Text To Shell Pane"
    static var description = IntentDescription("Send text to an alan shell pane.")

    @Parameter(title: "Pane")
    var pane: AlanShellPaneEntity

    @Parameter(title: "Text")
    var text: String

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.sendText(text, to: pane)
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
struct AlanReadPaneSummaryIntent: AppIntent {
    static var title: LocalizedStringResource = "Read Shell Pane Summary"
    static var description = IntentDescription("Read safe metadata for an alan shell pane.")

    @Parameter(title: "Pane")
    var pane: AlanShellPaneEntity

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.readPaneSummary(for: pane)
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
struct AlanOpenAttentionItemIntent: AppIntent {
    static var title: LocalizedStringResource = "Open Shell Attention Item"
    static var description = IntentDescription("Open an alan shell attention item.")

    @Parameter(title: "Attention Item")
    var item: AlanShellAttentionItemEntity

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let outcome = await ShellAutomationIntentRouter.openAttentionItem(item)
        return try shellAutomationIntentResult(outcome)
    }
}

@available(macOS 13.0, *)
private func shellAutomationIntentResult(
    _ outcome: ShellAutomationIntentOutcome
) throws -> some IntentResult & ProvidesDialog {
    try outcome.requireSuccessfulIntentResult()
    return .result(dialog: shellAutomationIntentDialog(outcome.dialog))
}

private func shellAutomationIntentDialog(_ text: String) -> IntentDialog {
    "\(text)"
}
#endif
