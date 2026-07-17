import Foundation

enum ManagedTerminalAccountIdentifierValidator {
    static func validate(_ request: ManagedTerminalAccountRequest) -> [ManagedTerminalAccountValidationError] {
        do {
            return try ShellCoreManagedTerminalAccountAdapter()
                .validateManagedTerminalAccountRequest(request)
        } catch {
            return [.coreUnavailable(String(describing: error))]
        }
    }
}
