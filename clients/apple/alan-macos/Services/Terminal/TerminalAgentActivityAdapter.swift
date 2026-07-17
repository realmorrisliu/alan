import Foundation

enum TerminalAgentActivityAdapter {
    private static let iso8601Formatter = ISO8601DateFormatter()

    static func activity(
        from event: TerminalAgentActivityEvent,
        now: Date = Date()
    ) -> TerminalActivitySnapshot? {
        guard let sourceKind = sourceKind(for: event.agentKind) else { return nil }
        guard let status = mappedStatus(for: event.status) else { return nil }

        let updatedAtDate = event.updatedAt.flatMap(iso8601Formatter.date(from:)) ?? now
        let updatedAt = iso8601Formatter.string(from: updatedAtDate)
        let sourceLabel = sourceLabel(for: sourceKind)
        let workingDirectory = sanitizedLabel(event.workingDirectory, maxLength: 240)
        let projectLabel = sanitizedLabel(event.projectLabel, maxLength: 48)
            ?? pathLeaf(from: workingDirectory)

        return TerminalActivitySnapshot(
            source: TerminalActivitySource(kind: sourceKind, label: sourceLabel),
            status: status.status,
            priority: status.priority,
            progress: nil,
            command: nil,
            agent: TerminalActivityAgentMetadata(
                kind: sourceKind,
                safeSessionLabel: sanitizedSessionLabel(event.sessionLabel),
                projectLabel: projectLabel,
                workingDirectory: workingDirectory
            ),
            display: TerminalActivityDisplay(
                sourceLabel: sourceLabel,
                stateLabel: status.stateLabel,
                detailLabel: sanitizedDetail(event.detail),
                paneHint: nil
            ),
            freshness: freshness(for: status.status, updatedAt: updatedAtDate, updatedAtString: updatedAt)
        )
    }

    private struct MappedStatus {
        let status: TerminalActivityStatus
        let priority: TerminalActivityPriority
        let stateLabel: String
    }

    private static func sourceKind(for raw: String) -> TerminalActivitySourceKind? {
        let token = normalizedToken(raw)
        guard !token.isEmpty else { return nil }

        switch token {
        case "codex", "openaicodex":
            return .codex
        default:
            return .unknown
        }
    }

    private static func sourceLabel(for sourceKind: TerminalActivitySourceKind) -> String {
        switch sourceKind {
        case .codex:
            return "Codex"
        default:
            return "Agent"
        }
    }

    private static func mappedStatus(for raw: String) -> MappedStatus? {
        switch normalizedToken(raw) {
        case "running", "inprogress", "working", "thinking", "streaming", "toolrunning":
            return MappedStatus(status: .running, priority: .active, stateLabel: "Running")
        case "needsinput", "inputrequired", "waitingforinput", "requiresinput",
             "approvalrequired", "requiresapproval", "blocked":
            return MappedStatus(status: .needsInput, priority: .awaitingUser, stateLabel: "Input needed")
        case "completed", "complete", "done", "success", "succeeded", "idle":
            return MappedStatus(status: .done, priority: .passive, stateLabel: "Done")
        case "failed", "failure", "error", "errored", "cancelled", "canceled":
            return MappedStatus(status: .failed, priority: .notable, stateLabel: "Error")
        case "paused":
            return MappedStatus(status: .paused, priority: .active, stateLabel: "Paused")
        default:
            return nil
        }
    }

    private static func freshness(
        for status: TerminalActivityStatus,
        updatedAt: Date,
        updatedAtString: String
    ) -> TerminalActivityFreshness {
        switch status {
        case .running:
            return TerminalActivityFreshness(
                updatedAt: updatedAtString,
                staleAt: iso8601Formatter.string(from: updatedAt.addingTimeInterval(90)),
                expiresAt: nil
            )
        case .done:
            return TerminalActivityFreshness(
                updatedAt: updatedAtString,
                staleAt: nil,
                expiresAt: iso8601Formatter.string(from: updatedAt.addingTimeInterval(8))
            )
        case .needsInput, .failed:
            return TerminalActivityFreshness(updatedAt: updatedAtString, staleAt: nil, expiresAt: nil)
        case .paused:
            return TerminalActivityFreshness(
                updatedAt: updatedAtString,
                staleAt: iso8601Formatter.string(from: updatedAt.addingTimeInterval(90)),
                expiresAt: nil
            )
        case .progress, .bell, .exited, .idle, .stale:
            return TerminalActivityFreshness(updatedAt: updatedAtString, staleAt: nil, expiresAt: nil)
        }
    }

    private static func sanitizedSessionLabel(_ raw: String?) -> String? {
        guard let label = sanitizedLabel(raw, maxLength: 32) else { return nil }
        let lowercased = label.lowercased()
        guard !lowercased.contains("session"),
              !lowercased.hasPrefix("sess"),
              !looksLikeRawIdentifier(label)
        else {
            return nil
        }
        return label
    }

    private static func sanitizedDetail(_ raw: String?) -> String? {
        guard let detail = sanitizedLabel(raw, maxLength: 80) else { return nil }
        let lowercased = detail.lowercased()
        guard !detail.hasPrefix("{"),
              !detail.hasPrefix("["),
              !lowercased.contains("event"),
              !lowercased.contains("session_id")
        else {
            return nil
        }
        return detail
    }

    private static func sanitizedLabel(_ raw: String?, maxLength: Int) -> String? {
        guard let raw else { return nil }
        let cleaned = raw.unicodeScalars
            .map { scalar in
                CharacterSet.controlCharacters.contains(scalar) ? " " : String(scalar)
            }
            .joined()
        let collapsed = cleaned
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        guard !collapsed.isEmpty else { return nil }

        let clipped = String(collapsed.prefix(maxLength))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return clipped.isEmpty ? nil : clipped
    }

    private static func pathLeaf(from raw: String?) -> String? {
        guard let raw, raw.contains("/") else { return nil }
        let leaf = URL(fileURLWithPath: raw).lastPathComponent
        return sanitizedLabel(leaf, maxLength: 48)
    }

    private static func looksLikeRawIdentifier(_ label: String) -> Bool {
        let alphanumericCount = label.filter { $0.isLetter || $0.isNumber }.count
        guard alphanumericCount >= 20 else { return false }
        let hexCount = label.filter(\.isHexDigit).count
        return Double(hexCount) / Double(alphanumericCount) > 0.7
    }

    private static func normalizedToken(_ raw: String) -> String {
        raw.lowercased().filter { $0.isLetter || $0.isNumber }
    }
}
