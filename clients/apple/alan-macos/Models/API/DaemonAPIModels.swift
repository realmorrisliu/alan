import Foundation

struct HealthResponse: Decodable {
    let status: String
}

enum GovernanceProfile: String, Codable, CaseIterable, Identifiable {
    case autonomous
    case conservative

    var id: String { rawValue }
}

enum SessionStreamingMode: String, Codable, CaseIterable, Identifiable {
    case auto
    case on
    case off

    var id: String { rawValue }
}

enum PartialStreamRecoveryMode: String, Codable, CaseIterable, Identifiable {
    case continueOnce = "continue_once"
    case off

    var id: String { rawValue }
}

enum ReasoningEffort: String, Codable, CaseIterable, Identifiable {
    case none
    case minimal
    case low
    case medium
    case high
    case xhigh

    var id: String { rawValue }
}

struct GovernanceConfig: Codable {
    let profile: GovernanceProfile
    let policyPath: String?

    private enum CodingKeys: String, CodingKey {
        case profile
        case policyPath = "policy_path"
    }
}

struct CreateSessionResponse: Decodable {
    let sessionID: String
    let websocketURL: String?
    let eventsURL: String?
    let submitURL: String?
    let governance: GovernanceConfig?
    let streamingMode: SessionStreamingMode?
    let partialStreamRecoveryMode: PartialStreamRecoveryMode?
    let reasoningEffort: ReasoningEffort?

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case websocketURL = "websocket_url"
        case eventsURL = "events_url"
        case submitURL = "submit_url"
        case governance
        case streamingMode = "streaming_mode"
        case partialStreamRecoveryMode = "partial_stream_recovery_mode"
        case reasoningEffort = "reasoning_effort"
        case id
        case session
    }

    private enum SessionContainerKeys: String, CodingKey {
        case id
        case sessionID = "session_id"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        if let value = try container.decodeIfPresent(String.self, forKey: .sessionID) {
            sessionID = value
        } else if let value = try container.decodeIfPresent(String.self, forKey: .id) {
            sessionID = value
        } else if container.contains(.session) {
            let nested = try container.nestedContainer(keyedBy: SessionContainerKeys.self, forKey: .session)
            if let value = try nested.decodeIfPresent(String.self, forKey: .sessionID) {
                sessionID = value
            } else if let value = try nested.decodeIfPresent(String.self, forKey: .id) {
                sessionID = value
            } else {
                throw AlanAPIError.missingSessionID
            }
        } else {
            throw AlanAPIError.missingSessionID
        }

        websocketURL = try container.decodeIfPresent(String.self, forKey: .websocketURL)
        eventsURL = try container.decodeIfPresent(String.self, forKey: .eventsURL)
        submitURL = try container.decodeIfPresent(String.self, forKey: .submitURL)
        governance = try container.decodeIfPresent(GovernanceConfig.self, forKey: .governance)
        streamingMode = try container.decodeIfPresent(SessionStreamingMode.self, forKey: .streamingMode)
        partialStreamRecoveryMode = try container.decodeIfPresent(
            PartialStreamRecoveryMode.self,
            forKey: .partialStreamRecoveryMode
        )
        reasoningEffort = try container.decodeIfPresent(ReasoningEffort.self, forKey: .reasoningEffort)
    }
}

struct SessionListResponse: Decodable {
    let sessions: [SessionListItem]
}

struct SessionListItem: Decodable, Identifiable {
    let sessionID: String
    let workspaceID: String
    let active: Bool
    let governance: GovernanceConfig
    let streamingMode: SessionStreamingMode
    let partialStreamRecoveryMode: PartialStreamRecoveryMode
    let reasoningEffort: ReasoningEffort?

    var id: String { sessionID }

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case workspaceID = "workspace_id"
        case active
        case governance
        case streamingMode = "streaming_mode"
        case partialStreamRecoveryMode = "partial_stream_recovery_mode"
        case reasoningEffort = "reasoning_effort"
    }
}

struct SessionHistoryMessage: Decodable {
    let role: String
    let content: String
    let toolName: String?
    let timestamp: String

    private enum CodingKeys: String, CodingKey {
        case role
        case content
        case toolName = "tool_name"
        case timestamp
    }
}

struct SessionReadResponse: Decodable {
    let sessionID: String
    let workspaceID: String
    let active: Bool
    let governance: GovernanceConfig
    let streamingMode: SessionStreamingMode
    let partialStreamRecoveryMode: PartialStreamRecoveryMode
    let reasoningEffort: ReasoningEffort?
    let rolloutPath: String?
    let messages: [SessionHistoryMessage]

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case workspaceID = "workspace_id"
        case active
        case governance
        case streamingMode = "streaming_mode"
        case partialStreamRecoveryMode = "partial_stream_recovery_mode"
        case reasoningEffort = "reasoning_effort"
        case rolloutPath = "rollout_path"
        case messages
    }
}

struct ResumeSessionResponse: Decodable {
    let sessionID: String
    let resumed: Bool

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case resumed
    }
}

struct ForkSessionResponse: Decodable {
    let sessionID: String
    let forkedFromSessionID: String
    let websocketURL: String?
    let eventsURL: String?
    let submitURL: String?
    let reasoningEffort: ReasoningEffort?

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case forkedFromSessionID = "forked_from_session_id"
        case websocketURL = "websocket_url"
        case eventsURL = "events_url"
        case submitURL = "submit_url"
        case reasoningEffort = "reasoning_effort"
    }
}

struct SubmitResponse: Decodable {
    let submissionID: String
    let accepted: Bool

    private enum CodingKeys: String, CodingKey {
        case submissionID = "submission_id"
        case accepted
    }
}

struct ReadEventsResponse: Decodable {
    let sessionID: String
    let gap: Bool
    let oldestEventID: String?
    let latestEventID: String?
    let events: [SessionEventEnvelope]

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case gap
        case oldestEventID = "oldest_event_id"
        case latestEventID = "latest_event_id"
        case events
    }
}

struct ConnectionCatalogResponse: Decodable {
    let providers: [ConnectionProviderDescriptorResponse]
}

struct ConnectionProviderDescriptorResponse: Decodable {
    let providerID: String
    let displayName: String
    let supportsBrowserLogin: Bool
    let supportsDeviceLogin: Bool
    let supportsSecretEntry: Bool
    let supportsLogout: Bool
    let supportsTest: Bool

    private enum CodingKeys: String, CodingKey {
        case providerID = "provider_id"
        case displayName = "display_name"
        case supportsBrowserLogin = "supports_browser_login"
        case supportsDeviceLogin = "supports_device_login"
        case supportsSecretEntry = "supports_secret_entry"
        case supportsLogout = "supports_logout"
        case supportsTest = "supports_test"
    }
}

struct ConnectionProfilesResponse: Decodable {
    let defaultProfile: String?
    let profiles: [ConnectionProfileSummaryResponse]

    private enum CodingKeys: String, CodingKey {
        case defaultProfile = "default_profile"
        case profiles
    }
}

struct ConnectionProfileSummaryResponse: Decodable {
    let profileID: String
    let label: String?
    let provider: String
    let credentialID: String?
    let settings: [String: String]
    let credentialStatus: String
    let isDefault: Bool
    let source: String

    private enum CodingKeys: String, CodingKey {
        case profileID = "profile_id"
        case label
        case provider
        case credentialID = "credential_id"
        case settings
        case credentialStatus = "credential_status"
        case isDefault = "is_default"
        case source
    }
}

struct ConnectionCurrentStateResponse: Decodable {
    let defaultProfile: String?
    let effectiveProfile: String?
    let effectiveSource: String

    private enum CodingKeys: String, CodingKey {
        case defaultProfile = "default_profile"
        case effectiveProfile = "effective_profile"
        case effectiveSource = "effective_source"
    }
}

struct SkillCatalogSnapshotResponse: Decodable {
    let skills: [SkillCatalogSkillResponse]
}

struct SkillCatalogSkillResponse: Decodable {
    let id: String
    let name: String
    let enabled: Bool
    let allowImplicitInvocation: Bool
    let available: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case enabled
        case allowImplicitInvocation = "allow_implicit_invocation"
        case available
    }
}

struct ToolDecisionAudit: Decodable {
    let policySource: String
    let ruleID: String?
    let action: String
    let reason: String?
    let capability: String
    let sandboxBackend: String

    private enum CodingKeys: String, CodingKey {
        case policySource = "policy_source"
        case ruleID = "rule_id"
        case action
        case reason
        case capability
        case sandboxBackend = "sandbox_backend"
    }
}

struct SessionEventEnvelope: Decodable {
    let eventID: String
    let sequence: UInt64?
    let sessionID: String?
    let submissionID: String?
    let turnID: String?
    let itemID: String?
    let timestampMS: UInt64?

    let type: String
    let chunk: String?
    let content: String?
    let isFinal: Bool?

    let toolCallID: String?
    let name: String?
    let resultPreview: String?
    let audit: ToolDecisionAudit?

    let requestID: String?
    let kind: JSONValue?
    let payload: JSONValue?

    let message: String?
    let recoverable: Bool?
    let summary: String?

    let replayFromEventID: String?

    var textChunk: String? {
        chunk ?? content
    }

    var normalizedYieldKind: String? {
        if let raw = kind?.stringValue {
            return raw
        }
        if let object = kind?.objectValue, let custom = object["custom"]?.stringValue {
            return custom
        }
        return nil
    }

    private enum CodingKeys: String, CodingKey {
        case eventID = "event_id"
        case sequence
        case sessionID = "session_id"
        case submissionID = "submission_id"
        case turnID = "turn_id"
        case itemID = "item_id"
        case timestampMS = "timestamp_ms"
        case type
        case chunk
        case content
        case isFinal = "is_final"
        case toolCallID = "id"
        case name
        case resultPreview = "result_preview"
        case audit
        case requestID = "request_id"
        case kind
        case payload
        case message
        case recoverable
        case summary
        case replayFromEventID = "replay_from_event_id"
    }
}

enum JSONValue: Codable, Equatable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: JSONValue])
    case array([JSONValue])
    case null

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()

        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([String: JSONValue].self) {
            self = .object(value)
        } else if let value = try? container.decode([JSONValue].self) {
            self = .array(value)
        } else {
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Unsupported JSON value"
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .string(let value):
            try container.encode(value)
        case .number(let value):
            try container.encode(value)
        case .bool(let value):
            try container.encode(value)
        case .object(let value):
            try container.encode(value)
        case .array(let value):
            try container.encode(value)
        case .null:
            try container.encodeNil()
        }
    }

    var stringValue: String? {
        if case .string(let value) = self {
            return value
        }
        return nil
    }

    var boolValue: Bool? {
        if case .bool(let value) = self {
            return value
        }
        return nil
    }

    var numberValue: Double? {
        if case .number(let value) = self {
            return value
        }
        return nil
    }

    var objectValue: [String: JSONValue]? {
        if case .object(let value) = self {
            return value
        }
        return nil
    }

    var arrayValue: [JSONValue]? {
        if case .array(let value) = self {
            return value
        }
        return nil
    }

    subscript(key: String) -> JSONValue? {
        objectValue?[key]
    }

    static func from(any value: Any) -> JSONValue {
        switch value {
        case let value as String:
            return .string(value)
        case let value as Bool:
            return .bool(value)
        case let value as NSNumber:
            return .number(value.doubleValue)
        case let value as [String: Any]:
            let mapped = value.mapValues { JSONValue.from(any: $0) }
            return .object(mapped)
        case let value as [Any]:
            return .array(value.map(JSONValue.from(any:)))
        default:
            return .null
        }
    }

    func toAny() -> Any {
        switch self {
        case .string(let value):
            return value
        case .number(let value):
            return value
        case .bool(let value):
            return value
        case .object(let value):
            return value.mapValues { $0.toAny() }
        case .array(let value):
            return value.map { $0.toAny() }
        case .null:
            return NSNull()
        }
    }
}

enum AlanContentPart: Encodable {
    case text(String)
    case structured(JSONValue)

    private enum CodingKeys: String, CodingKey {
        case type
        case text
        case data
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .text(let text):
            try container.encode("text", forKey: .type)
            try container.encode(text, forKey: .text)
        case .structured(let data):
            try container.encode("structured", forKey: .type)
            try container.encode(data, forKey: .data)
        }
    }
}

enum AlanOperation: Encodable {
    case turn(parts: [AlanContentPart])
    case input(parts: [AlanContentPart])
    case resume(requestID: String, content: [AlanContentPart])
    case interrupt
    case compactWithOptions(focus: String?)
    case rollback(turns: Int)

    private enum CodingKeys: String, CodingKey {
        case type
        case parts
        case requestID = "request_id"
        case content
        case focus
        case turns
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .turn(let parts):
            try container.encode("turn", forKey: .type)
            try container.encode(parts, forKey: .parts)
        case .input(let parts):
            try container.encode("input", forKey: .type)
            try container.encode(parts, forKey: .parts)
        case .resume(let requestID, let content):
            try container.encode("resume", forKey: .type)
            try container.encode(requestID, forKey: .requestID)
            try container.encode(content, forKey: .content)
        case .interrupt:
            try container.encode("interrupt", forKey: .type)
        case .compactWithOptions(let focus):
            try container.encode("compact_with_options", forKey: .type)
            try container.encodeIfPresent(focus, forKey: .focus)
        case .rollback(let turns):
            try container.encode("rollback", forKey: .type)
            try container.encode(turns, forKey: .turns)
        }
    }
}

enum AlanAPIError: LocalizedError {
    case invalidURL(String)
    case invalidResponse
    case unexpectedStatusCode(Int, String)
    case missingSessionID
    case invalidTextInput
    case invalidJSONPayload

    var errorDescription: String? {
        switch self {
        case .invalidURL(let value):
            return "Invalid server URL: \(value)"
        case .invalidResponse:
            return "Invalid server response"
        case .unexpectedStatusCode(let code, let body):
            return "Server returned \(code): \(body)"
        case .missingSessionID:
            return "Session response did not include a session id"
        case .invalidTextInput:
            return "Message cannot be empty"
        case .invalidJSONPayload:
            return "Invalid JSON payload"
        }
    }
}
