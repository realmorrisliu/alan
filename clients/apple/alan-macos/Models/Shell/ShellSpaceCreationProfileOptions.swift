import Combine
import Foundation

#if os(macOS)
struct ShellSpaceCreationProfileOption: Identifiable, Equatable, Sendable {
    let id: String
    let name: String
    let systemName: String
    let isEnabled: Bool
    let guidance: String?

    init(
        id: String,
        name: String,
        systemName: String = "terminal",
        isEnabled: Bool = true,
        guidance: String? = nil
    ) {
        self.id = id
        self.name = name
        self.systemName = systemName
        self.isEnabled = isEnabled
        self.guidance = guidance
    }
}

@MainActor
final class ShellSpaceCreationProfileOptionStore: ObservableObject {
    @Published private(set) var options: [ShellSpaceCreationProfileOption] = []
    private var refreshGeneration = 0

    func refresh() {
        refreshGeneration += 1
        let generation = refreshGeneration
        Task { [weak self] in
            let options = await ShellSpaceCreationProfileOptionLoader.load()
            guard let self, self.refreshGeneration == generation else { return }
            self.options = options
        }
    }
}

private enum ShellSpaceCreationProfileOptionLoader {
    static func load() async -> [ShellSpaceCreationProfileOption] {
        await Task.detached(priority: .utility) {
            profileOptions()
        }.value
    }

    private static func profileOptions() -> [ShellSpaceCreationProfileOption] {
        let terminalProfiles = TerminalProfileSettingsSummary.current()
        let managedAccounts = ManagedTerminalAccountSettingsSummary.current(
            terminalProfiles: terminalProfiles,
            helperClient: AlanPrivilegedHelperAppClient(channel: .current())
        )
        let selectableIDs = Set(
            TerminalProfileSpaceIdentityFilter.selectableProfiles(
                terminalProfiles: terminalProfiles,
                managedTerminalAccounts: managedAccounts
            ).map(\.id)
        )
        return terminalProfiles.profiles
            .filter { $0.id != TerminalProfileDefinition.loginShellFallback.id }
            .map { profile in
                let isEnabled = selectableIDs.contains(profile.id)
                return ShellSpaceCreationProfileOption(
                    id: profile.id,
                    name: shellTerminalProfileMenuTitle(profile),
                    systemName: shellTerminalProfileMenuSymbol(for: profile),
                    isEnabled: isEnabled,
                    guidance: isEnabled
                        ? nil
                        : TerminalProfileSpaceIdentityFilter.repairGuidance(
                            profileID: profile.id,
                            terminalProfiles: terminalProfiles,
                            managedTerminalAccounts: managedAccounts
                        )
                )
            }
    }

    private static func shellTerminalProfileMenuTitle(_ profile: TerminalProfileDefinition) -> String {
        "\(profile.title) · \(profile.launch.kind.rawValue)"
    }

    private static func shellTerminalProfileMenuSymbol(for profile: TerminalProfileDefinition) -> String {
        switch profile.launch {
        case .loginShell:
            return "terminal"
        case .sudoUser:
            return "person.crop.circle"
        case .sudoRoot:
            return "exclamationmark.triangle"
        case .managedUser:
            return "checkmark.seal"
        case .customCommand:
            return "hammer"
        }
    }
}
#endif
