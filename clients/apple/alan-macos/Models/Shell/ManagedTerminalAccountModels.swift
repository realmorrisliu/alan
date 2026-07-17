import Foundation

struct ManagedTerminalAccountRequest: Codable, Equatable {
    let accountName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool

    init(
        accountName: String,
        fullName: String? = nil,
        shell: String = "/bin/zsh",
        homeDirectory: String? = nil,
        hideFromLoginWindow: Bool = true
    ) {
        self.accountName = accountName
        self.fullName = fullName
        self.shell = shell
        self.homeDirectory = homeDirectory ?? "/Users/\(accountName)"
        self.hideFromLoginWindow = hideFromLoginWindow
    }

    static func canonicalHomeDirectory(for accountName: String) -> String {
        "/Users/\(accountName)"
    }

    var terminalProfileID: String {
        accountName
    }
}

enum ManagedTerminalAccountValidationError: Equatable {
    case invalidAccountName(String)
    case reservedAccountName(String)
    case invalidShell(String)
    case coreUnavailable(String)
}
