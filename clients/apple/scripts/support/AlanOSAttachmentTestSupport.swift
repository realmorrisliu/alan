import Foundation

enum AlanInstallChannel: Equatable {
    case stable
    case dev

    static func current() -> Self { .dev }

    var installChannelID: String {
        switch self {
        case .stable: "stable"
        case .dev: "dev"
        }
    }

    var bundleIdentifier: String {
        switch self {
        case .stable: "app.alanworks.macos"
        case .dev: "app.alanworks.macos.dev"
        }
    }

    var cliToolName: String {
        switch self {
        case .stable: "alan"
        case .dev: "alan-dev"
        }
    }
}

struct AlanOSProcessReference: Codable, Equatable, Hashable {
    let bootID: String
    let pid: UInt64

    private enum CodingKeys: String, CodingKey {
        case bootID = "boot_id"
        case pid
    }
}

struct AlanAgentStreamOffsets: Codable, Equatable {
    var output: UInt64
    var requests: UInt64
    var actions: UInt64
    var ui: UInt64

    static let zero = Self(output: 0, requests: 0, actions: 0, ui: 0)
}

struct AlanAgentContentPresentation: Codable, Equatable {
    var followsOutput: Bool

    static let `default` = Self(followsOutput: true)

    private enum CodingKeys: String, CodingKey {
        case followsOutput = "follows_output"
    }
}

struct AlanAgentAttachment: Codable, Equatable {
    let process: AlanOSProcessReference
    var offsets: AlanAgentStreamOffsets
    var presentation: AlanAgentContentPresentation
}
