import Foundation

#if os(macOS)
@MainActor
final class FakeShellAutomationCommandHandler: ShellAutomationCommandHandling {
    private(set) var recordedCommands: [ShellAutomationCommand] = []
    var response: (ShellAutomationCommand) -> ShellAutomationCommandResult

    init(
        response: @escaping (ShellAutomationCommand) -> ShellAutomationCommandResult = { _ in
            ShellAutomationCommandResult(code: .accepted, summary: nil)
        }
    ) {
        self.response = response
    }

    func performShellAutomationCommand(
        _ command: ShellAutomationCommand
    ) -> ShellAutomationCommandResult {
        recordedCommands.append(command)
        return response(command)
    }
}

final class FakeShellAutomationTextRuntime {
    private(set) var deliveredText: [ShellAutomationSendTextRequest] = []
    var result: ShellAutomationCommandResult

    init(
        result: ShellAutomationCommandResult = ShellAutomationCommandResult(
            code: .accepted,
            summary: nil
        )
    ) {
        self.result = result
    }

    func sendText(_ request: ShellAutomationSendTextRequest) -> ShellAutomationCommandResult {
        deliveredText.append(request)
        return result
    }
}
#endif
