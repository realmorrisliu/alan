import Foundation

struct ShellReducerCommandCoordinator {
    func apply(
        state: ShellStateSnapshot,
        operation: ShellCoreReducerOperation
    ) throws -> ShellStateMutationResult {
        try ShellCoreFFIAdapter.shared.applyReducer(state: state, operation: operation)
    }
}
