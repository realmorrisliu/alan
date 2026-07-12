import Foundation

extension ShellCoreFFIAdapter {
    func validateManagedTerminalAccountRequest(
        _ request: ManagedTerminalAccountRequest
    ) throws -> [ManagedTerminalAccountValidationError] {
        let response: ShellCoreManagedAccountValidationResponse = try send(
            operation: "managed_terminal_account.validate_request",
            payload: ShellCoreManagedAccountRequestPayload(request)
        )
        return response.errors.map(\.swiftError)
    }
}

private struct ShellCoreManagedAccountValidationResponse: Decodable {
    let errors: [ShellCoreManagedAccountValidationError]
}

private struct ShellCoreManagedAccountRequestPayload: Encodable {
    let accountName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool

    private enum CodingKeys: String, CodingKey {
        case accountName = "account_name"
        case fullName = "full_name"
        case shell
        case homeDirectory = "home_directory"
        case hideFromLoginWindow = "hide_from_login_window"
    }

    init(_ request: ManagedTerminalAccountRequest) {
        accountName = request.accountName
        fullName = request.fullName
        shell = request.shell
        homeDirectory = request.homeDirectory
        hideFromLoginWindow = request.hideFromLoginWindow
    }
}

private enum ShellCoreManagedAccountValidationError: Decodable {
    case invalidAccountName(String)
    case reservedAccountName(String)
    case invalidShell(String)

    private enum CodingKeys: String, CodingKey {
        case type
        case value
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let value = try container.decode(String.self, forKey: .value)
        switch try container.decode(String.self, forKey: .type) {
        case "invalid_account_name":
            self = .invalidAccountName(value)
        case "reserved_account_name":
            self = .reservedAccountName(value)
        case "invalid_shell":
            self = .invalidShell(value)
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "Unsupported managed terminal account validation error"
                )
            )
        }
    }

    var swiftError: ManagedTerminalAccountValidationError {
        switch self {
        case let .invalidAccountName(value):
            return .invalidAccountName(value)
        case let .reservedAccountName(value):
            return .reservedAccountName(value)
        case let .invalidShell(value):
            return .invalidShell(value)
        }
    }
}
