import Foundation

enum ShellActivityNotificationVisibility: String, Equatable {
    case focusedVisible
    case visibleUnfocused
    case background
}
enum ShellActivityNotificationKind: String, Equatable {
    case needsInput = "needs_input"
    case failed
    case commandCompleted = "command_completed"
    case processExited = "process_exited"
}

struct ShellActivityNotificationRoute: Equatable, Identifiable {
    let id: String
    let paneID: String
    let tabID: String
    let spaceID: String
    let kind: ShellActivityNotificationKind
    let title: String
    let body: String
    let attention: ShellAttentionState
}

func shellActivityNotificationKey(
    for activity: TerminalActivitySnapshot,
    paneID: String
) -> String {
    [
        paneID,
        activity.source.kind.rawValue,
        activity.status.rawValue,
        activity.freshness.updatedAt,
        shellActivityNotificationPayloadDiscriminator(for: activity),
    ].joined(separator: ":")
}

private func shellActivityNotificationPayloadDiscriminator(
    for activity: TerminalActivitySnapshot
) -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    if let data = try? encoder.encode(activity) {
        return data.base64EncodedString()
    }

    return [
        activity.source.label ?? "",
        activity.priority.rawValue,
        activity.display.sourceFirstLabel,
        activity.freshness.staleAt ?? "",
        activity.freshness.expiresAt ?? "",
    ].joined(separator: "|")
}

func shellActivityAttention(for activity: TerminalActivitySnapshot) -> ShellAttentionState? {
    switch activity.status {
    case .needsInput, .exited:
        return .awaitingUser
    case .failed:
        return .notable
    case .paused, .progress, .running, .bell, .idle, .done, .stale:
        return nil
    }
}

func shellEffectiveAttention(for pane: ShellPane, now: Date? = nil) -> ShellAttentionState {
    let storedAttention = pane.attention
    guard let activity = pane.activity else { return storedAttention }

    if let now,
       !activity.isFresh(at: now),
       let activityAttention = shellActivityAttention(for: activity),
       activityAttention == storedAttention
    {
        return shellPersistentAttention(for: pane)
    }

    if let now, !activity.isFresh(at: now) {
        return storedAttention
    }

    guard let activityAttention = shellActivityAttention(for: activity),
          shellAttentionRank(for: activityAttention) > shellAttentionRank(for: storedAttention)
    else {
        return storedAttention
    }

    return activityAttention
}

private func shellPersistentAttention(for pane: ShellPane) -> ShellAttentionState {
    if pane.alanBinding?.pendingRequest == true {
        return .awaitingUser
    }

    if pane.context?.processState == "exited"
        || pane.context?.surfaceReadiness == "child_exited"
    {
        return .awaitingUser
    }

    if pane.context?.rendererHealth == "failed"
        || pane.context?.rendererPhase == "failed"
        || pane.context?.surfaceReadiness == "renderer_failed"
    {
        return .notable
    }

    return .idle
}

private func shellAttentionRank(for attention: ShellAttentionState) -> Int {
    switch attention {
    case .idle:
        return 0
    case .active:
        return 1
    case .notable:
        return 2
    case .awaitingUser:
        return 3
    }
}

func shellActivityNotificationRoute(
    for activity: TerminalActivitySnapshot,
    pane: ShellPane,
    tab: ShellTab?,
    visibility: ShellActivityNotificationVisibility,
    now: Date? = nil,
    longCommandThresholdMilliseconds: Int = 60_000
) -> ShellActivityNotificationRoute? {
    if let now, !activity.isFresh(at: now) {
        return nil
    }

    guard visibility != .focusedVisible else {
        return nil
    }

    let kind: ShellActivityNotificationKind
    let attention: ShellAttentionState
    switch activity.status {
    case .needsInput where shellIsCodingAgentActivity(activity):
        kind = .needsInput
        attention = .awaitingUser
    case .failed where shellIsCodingAgentActivity(activity):
        kind = .failed
        attention = .notable
    case .done
        where activity.source.kind == .command
            && (activity.command?.durationMilliseconds ?? 0) >= longCommandThresholdMilliseconds,
        .failed
        where activity.source.kind == .command
            && (activity.command?.durationMilliseconds ?? 0) >= longCommandThresholdMilliseconds:
        kind = .commandCompleted
        attention = .notable
    case .exited:
        kind = .processExited
        attention = .awaitingUser
    default:
        return nil
    }

    let subject = shellDisplayTitle(
        rawTitle: tab?.title ?? pane.viewport?.title,
        workingDirectoryName: pane.context?.workingDirectoryName,
        cwd: pane.cwd,
        program: pane.process?.program,
        launchTarget: pane.resolvedLaunchTarget,
        fallback: shellFallbackTitle(for: tab?.kind ?? .terminal)
    )
    return ShellActivityNotificationRoute(
        id: shellActivityNotificationKey(for: activity, paneID: pane.paneID),
        paneID: pane.paneID,
        tabID: pane.tabID,
        spaceID: pane.spaceID,
        kind: kind,
        title: activity.display.sourceFirstLabel,
        body: subject,
        attention: attention
    )
}

private func shellIsCodingAgentActivity(_ activity: TerminalActivitySnapshot) -> Bool {
    switch activity.source.kind {
    case .codex, .claude, .openCode, .alan:
        return true
    case .shell, .progress, .command, .process, .unknown:
        return false
    }
}
