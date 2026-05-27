import Foundation

private struct SubmitBody: Encodable {
    let op: AlanOperation
}

private struct CreateSessionBody {
    let governanceProfile: GovernanceProfile
    let reasoningEffort: ReasoningEffort?
    let streamingMode: SessionStreamingMode?
    let partialStreamRecoveryMode: PartialStreamRecoveryMode?

    func toJSONObject() -> [String: Any] {
        var body: [String: Any] = [
            "governance": [
                "profile": governanceProfile.rawValue,
            ],
        ]
        if let streamingMode {
            body["streaming_mode"] = streamingMode.rawValue
        }
        if let reasoningEffort {
            body["reasoning_effort"] = reasoningEffort.rawValue
        }
        if let partialStreamRecoveryMode {
            body["partial_stream_recovery_mode"] = partialStreamRecoveryMode.rawValue
        }
        return body
    }
}

struct AlanAPIClient {
    private let baseURL: URL
    private let session: URLSession
    private let decoder = JSONDecoder()
    private let encoder = JSONEncoder()

    init(baseURLString: String, session: URLSession = .shared) throws {
        let trimmed = baseURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: trimmed) else {
            throw AlanAPIError.invalidURL(baseURLString)
        }

        self.baseURL = url
        self.session = session
    }

    func checkHealth() async throws -> HealthResponse {
        let requestURL = endpointURL(pathComponents: ["health"])
        let data = try await request(url: requestURL)

        if let decoded = try? decoder.decode(HealthResponse.self, from: data) {
            return decoded
        }

        let body = String(data: data, encoding: .utf8) ?? "OK"
        return HealthResponse(status: body)
    }

    func listSessions() async throws -> [SessionListItem] {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "sessions"])
        let data = try await request(url: requestURL)
        return try decoder.decode(SessionListResponse.self, from: data).sessions
    }

    func createSession(
        governanceProfile: GovernanceProfile,
        reasoningEffort: ReasoningEffort? = nil,
        streamingMode: SessionStreamingMode? = nil,
        partialStreamRecoveryMode: PartialStreamRecoveryMode? = nil
    ) async throws -> CreateSessionResponse {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "sessions"])
        let body = CreateSessionBody(
            governanceProfile: governanceProfile,
            reasoningEffort: reasoningEffort,
            streamingMode: streamingMode,
            partialStreamRecoveryMode: partialStreamRecoveryMode
        )
        let data = try JSONSerialization.data(withJSONObject: body.toJSONObject())
        let responseData = try await request(
            url: requestURL,
            method: "POST",
            contentType: "application/json",
            body: data
        )
        return try decoder.decode(CreateSessionResponse.self, from: responseData)
    }

    func readSession(sessionID: String) async throws -> SessionReadResponse {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "sessions", sessionID, "read"])
        let data = try await request(url: requestURL)
        return try decoder.decode(SessionReadResponse.self, from: data)
    }

    func resumeSession(sessionID: String) async throws -> ResumeSessionResponse {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "sessions", sessionID, "resume"])
        let data = try await request(
            url: requestURL,
            method: "POST",
            contentType: "application/json",
            body: Data("{}".utf8)
        )
        return try decoder.decode(ResumeSessionResponse.self, from: data)
    }

    func forkSession(sessionID: String) async throws -> ForkSessionResponse {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "sessions", sessionID, "fork"])
        let data = try await request(
            url: requestURL,
            method: "POST",
            contentType: "application/json",
            body: Data("{}".utf8)
        )
        return try decoder.decode(ForkSessionResponse.self, from: data)
    }

    func deleteSession(sessionID: String) async throws {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "sessions", sessionID])
        _ = try await request(url: requestURL, method: "DELETE")
    }

    func submitOperation(sessionID: String, operation: AlanOperation) async throws -> SubmitResponse {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "sessions", sessionID, "submit"])
        let body = try encoder.encode(SubmitBody(op: operation))
        let data = try await request(
            url: requestURL,
            method: "POST",
            contentType: "application/json",
            body: body
        )
        return try decoder.decode(SubmitResponse.self, from: data)
    }

    func sendTurn(sessionID: String, text: String) async throws -> SubmitResponse {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw AlanAPIError.invalidTextInput
        }
        return try await submitOperation(
            sessionID: sessionID,
            operation: .turn(parts: [.text(trimmed)])
        )
    }

    func sendInput(sessionID: String, text: String) async throws -> SubmitResponse {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw AlanAPIError.invalidTextInput
        }
        return try await submitOperation(
            sessionID: sessionID,
            operation: .input(parts: [.text(trimmed)])
        )
    }

    func resume(sessionID: String, requestID: String, payload: JSONValue) async throws -> SubmitResponse {
        try await submitOperation(
            sessionID: sessionID,
            operation: .resume(requestID: requestID, content: [.structured(payload)])
        )
    }

    func interrupt(sessionID: String) async throws -> SubmitResponse {
        try await submitOperation(sessionID: sessionID, operation: .interrupt)
    }

    func compact(sessionID: String, focus: String? = nil) async throws -> SubmitResponse {
        try await submitOperation(
            sessionID: sessionID,
            operation: .compactWithOptions(focus: focus)
        )
    }

    func rollback(sessionID: String, turns: Int) async throws -> SubmitResponse {
        try await submitOperation(sessionID: sessionID, operation: .rollback(turns: turns))
    }

    func readEvents(
        sessionID: String,
        afterEventID: String?,
        limit: Int = 200
    ) async throws -> ReadEventsResponse {
        var components = URLComponents(
            url: endpointURL(pathComponents: ["api", "v1", "sessions", sessionID, "events", "read"]),
            resolvingAgainstBaseURL: false
        )

        var queryItems = [URLQueryItem(name: "limit", value: String(limit))]
        if let afterEventID, !afterEventID.isEmpty {
            queryItems.append(URLQueryItem(name: "after_event_id", value: afterEventID))
        }
        components?.queryItems = queryItems

        guard let requestURL = components?.url else {
            throw AlanAPIError.invalidURL(baseURL.absoluteString)
        }

        let data = try await request(url: requestURL)
        return try decoder.decode(ReadEventsResponse.self, from: data)
    }

    func connectionCatalog() async throws -> ConnectionCatalogResponse {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "connections", "catalog"])
        let data = try await request(url: requestURL)
        return try decoder.decode(ConnectionCatalogResponse.self, from: data)
    }

    func listConnectionProfiles() async throws -> ConnectionProfilesResponse {
        let requestURL = endpointURL(pathComponents: ["api", "v1", "connections"])
        let data = try await request(url: requestURL)
        return try decoder.decode(ConnectionProfilesResponse.self, from: data)
    }

    func currentConnection(workspaceDir: String? = nil) async throws -> ConnectionCurrentStateResponse {
        var components = URLComponents(
            url: endpointURL(pathComponents: ["api", "v1", "connections", "current"]),
            resolvingAgainstBaseURL: false
        )
        if let workspaceDir,
           !workspaceDir.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            components?.queryItems = [URLQueryItem(name: "workspace_dir", value: workspaceDir)]
        }
        guard let requestURL = components?.url else {
            throw AlanAPIError.invalidURL(baseURL.absoluteString)
        }
        let data = try await request(url: requestURL)
        return try decoder.decode(ConnectionCurrentStateResponse.self, from: data)
    }

    func skillCatalog(
        workspaceDir: String? = nil,
        agentName: String? = nil
    ) async throws -> SkillCatalogSnapshotResponse {
        var components = URLComponents(
            url: endpointURL(pathComponents: ["api", "v1", "skills", "catalog"]),
            resolvingAgainstBaseURL: false
        )
        var queryItems: [URLQueryItem] = []
        if let workspaceDir,
           !workspaceDir.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            queryItems.append(URLQueryItem(name: "workspace_dir", value: workspaceDir))
        }
        if let agentName,
           !agentName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            queryItems.append(URLQueryItem(name: "agent_name", value: agentName))
        }
        components?.queryItems = queryItems.isEmpty ? nil : queryItems
        guard let requestURL = components?.url else {
            throw AlanAPIError.invalidURL(baseURL.absoluteString)
        }
        let data = try await request(url: requestURL)
        return try decoder.decode(SkillCatalogSnapshotResponse.self, from: data)
    }

    private func endpointURL(pathComponents: [String]) -> URL {
        pathComponents.reduce(baseURL) { partial, component in
            partial.appendingPathComponent(component)
        }
    }

    private func request(
        url: URL,
        method: String = "GET",
        contentType: String? = nil,
        body: Data? = nil
    ) async throws -> Data {
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.httpBody = body
        if let contentType {
            request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        }

        let (data, response) = try await session.data(for: request)
        try validate(response: response, data: data)
        return data
    }

    private func validate(response: URLResponse, data: Data) throws {
        guard let http = response as? HTTPURLResponse else {
            throw AlanAPIError.invalidResponse
        }

        guard (200..<300).contains(http.statusCode) else {
            let body = String(data: data, encoding: .utf8) ?? "<no body>"
            throw AlanAPIError.unexpectedStatusCode(http.statusCode, body)
        }
    }
}
