import Foundation

struct ShellPaneTitleBarDetailProjection: Equatable, Identifiable {
    let id: String
    let title: String
    let help: String
}
func shellUserFacingSummary(_ summary: String?) -> String? {
    guard let summary else { return nil }

    let trimmed = summary.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }

    let internalOnlySummaries = [
        "command finished",
        "command succeeded",
        "title updated",
        "input committed",
        "terminal bell",
        "terminal rendering",
        "window attached",
    ]

    let lowercasedSummary = trimmed.lowercased()
    if internalOnlySummaries.contains(lowercasedSummary)
        || lowercasedSummary.hasPrefix("command failed")
    {
        return nil
    }

    return trimmed
}

func shellTerminalStatusSummary(for pane: ShellPane, now: Date? = nil) -> String? {
    if pane.context?.processState == "exited"
        || pane.context?.surfaceReadiness == "child_exited"
    {
        if let exitCode = pane.context?.lastCommandExitCode {
            return "Exited \(exitCode)"
        }
        return "Exited"
    }

    if pane.context?.rendererHealth == "failed"
        || pane.context?.rendererPhase == "failed"
        || pane.context?.surfaceReadiness == "renderer_failed"
    {
        return "Renderer failed"
    }

    if pane.context?.readonly == true {
        return "Read-only"
    }

    if pane.context?.inputReady == false,
       pane.context?.surfaceReadiness == "input_not_ready"
    {
        return "Starting"
    }

    switch shellEffectiveAttention(for: pane, now: now) {
    case .awaitingUser:
        guard let rawSummary = pane.viewport?.summary else { return "Needs attention" }
        return shellUserFacingSummary(rawSummary)
    case .notable:
        guard let rawSummary = pane.viewport?.summary else { return "Terminal bell" }
        return shellUserFacingSummary(rawSummary)
    case .active, .idle:
        return nil
    }
}

func shellVisibleLabel(_ raw: String?) -> String? {
    guard let raw else { return nil }
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, trimmed != "/", trimmed != "-" else { return nil }
    if trimmed.lowercased() == "alan" {
        return "alan"
    }
    return trimmed
}

func shellPathLeaf(_ raw: String?) -> String? {
    guard let visible = shellVisibleLabel(raw) else { return nil }
    if visible == "~" {
        return "Home"
    }

    guard visible.contains("/") else { return nil }
    let components = visible.split(separator: "/").map(String.init)
    return components.last.flatMap(shellVisibleLabel)
}

func shellNormalizedTitle(_ raw: String?) -> String? {
    guard var candidate = shellVisibleLabel(raw) else { return nil }

    let internalOnlyTitles = [
        "title updated",
        "input committed",
        "terminal rendering",
        "window attached",
    ]

    let lowercasedCandidate = candidate.lowercased()
    if internalOnlyTitles.contains(lowercasedCandidate)
        || lowercasedCandidate.hasPrefix("pane_")
    {
        return nil
    }

    for suffix in [" - fish", " - zsh", " - bash", " - sh"] {
        if candidate.lowercased().hasSuffix(suffix) {
            candidate.removeLast(suffix.count)
            break
        }
    }

    candidate = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let visible = shellVisibleLabel(candidate) else { return nil }

    if let leaf = shellPathLeaf(visible) {
        return leaf
    }

    return visible
}

func shellDisplayTitle(
    rawTitle: String?,
    workingDirectoryName: String?,
    cwd: String?,
    program: String?,
    launchTarget: ShellLaunchTarget,
    fallback: String? = nil
) -> String {
    if let workingDirectoryName = shellVisibleLabel(workingDirectoryName) {
        return workingDirectoryName
    }

    if let cwdLeaf = shellPathLeaf(cwd) {
        return cwdLeaf
    }

    if let normalizedTitle = shellNormalizedTitle(rawTitle) {
        return normalizedTitle
    }

    if let fallback = shellVisibleLabel(fallback) {
        return fallback
    }

    if let program = shellVisibleLabel(program) {
        return program
    }

    return "Terminal"
}

func shellPaneTitleBarTitle(for pane: ShellPane) -> String {
    if let normalizedTitle = shellNormalizedTitle(pane.viewport?.title) {
        return normalizedTitle
    }

    if let cwdLeaf = shellPathLeaf(pane.cwd) {
        return cwdLeaf
    }

    if let workingDirectory = shellVisibleLabel(pane.context?.workingDirectoryName) {
        return workingDirectory
    }

    if let program = shellVisibleLabel(pane.process?.program) {
        return program
    }

    return "Terminal"
}

func shellContentTypeHint(for kind: ShellContentKind) -> String {
    switch kind {
    case .terminal:
        return "Terminal"
    case .markdown:
        return "Document"
    case .settings:
        return "Settings"
    case .agent:
        return "Agent"
    }
}

func shellPaneActivityAccessoryLabel(for pane: ShellPane, now: Date? = nil) -> String? {
    guard let activity = pane.activity else { return nil }
    if let now, !activity.isFresh(at: now) {
        return nil
    }

    switch activity.status {
    case .idle, .stale:
        return nil
    case .done:
        return activity.source.kind == .command ? nil : activity.display.sourceFirstLabel
    case .needsInput, .failed, .paused, .progress, .running, .bell, .exited:
        return activity.display.sourceFirstLabel
    }
}

func shellPaneStatusAccessoryLabel(for pane: ShellPane, now: Date? = nil) -> String? {
    guard let status = shellTerminalStatusSummary(for: pane, now: now) else { return nil }
    if shellEffectiveAttention(for: pane, now: now) == .notable,
       status == "Terminal bell"
    {
        return nil
    }
    return status
}

func shellPaneTitleBarDetailProjection(
    for pane: ShellPane,
    title: String,
    now: Date? = nil
) -> [ShellPaneTitleBarDetailProjection] {
    var items: [ShellPaneTitleBarDetailProjection] = []

    if let activityLabel = shellPaneActivityAccessoryLabel(for: pane, now: now) {
        items.append(
            ShellPaneTitleBarDetailProjection(
                id: "activity",
                title: activityLabel,
                help: activityLabel
            )
        )
    }

    if let status = shellPaneStatusAccessoryLabel(for: pane, now: now) {
        items.append(
            ShellPaneTitleBarDetailProjection(
                id: "status",
                title: status,
                help: status
            )
        )
    }

    if let context = shellPaneContextAccessoryProjection(for: pane, title: title) {
        items.append(context)
    }

    if let branch = shellPaneBranchAccessoryProjection(for: pane, title: title) {
        items.append(branch)
    }

    if let process = shellPaneProcessAccessoryProjection(for: pane, title: title) {
        items.append(process)
    }

    if let alan = shellPaneAlanAccessoryProjection(for: pane) {
        items.append(alan)
    }

    return items
}

private func shellPaneContextAccessoryProjection(
    for pane: ShellPane,
    title: String
) -> ShellPaneTitleBarDetailProjection? {
    let repositoryLabel = shellPathLeaf(pane.context?.repositoryRoot)
    let cwdLabel = shellPathLeaf(pane.cwd)
        ?? shellVisibleLabel(pane.context?.workingDirectoryName)
    guard let label = repositoryLabel ?? cwdLabel,
          !shellLabelsMatch(label, title)
    else {
        return nil
    }

    return ShellPaneTitleBarDetailProjection(
        id: repositoryLabel == nil ? "cwd" : "worktree",
        title: label,
        help: repositoryLabel == nil ? "Directory \(label)" : "Worktree \(label)"
    )
}

private func shellPaneBranchAccessoryProjection(
    for pane: ShellPane,
    title: String
) -> ShellPaneTitleBarDetailProjection? {
    guard let branch = shellVisibleLabel(pane.context?.gitBranch),
          !shellLabelsMatch(branch, title)
    else {
        return nil
    }

    return ShellPaneTitleBarDetailProjection(
        id: "branch",
        title: branch,
        help: "Git branch \(branch)"
    )
}

private func shellPaneProcessAccessoryProjection(
    for pane: ShellPane,
    title: String
) -> ShellPaneTitleBarDetailProjection? {
    guard let program = shellVisibleLabel(pane.process?.program),
          !shellLabelsMatch(program, title),
          !shellProcessDuplicatesAgentOrAlan(program, pane: pane)
    else {
        return nil
    }

    return ShellPaneTitleBarDetailProjection(
        id: "process",
        title: program,
        help: "Process \(program)"
    )
}

private func shellPaneAlanAccessoryProjection(for pane: ShellPane) -> ShellPaneTitleBarDetailProjection? {
    guard let binding = pane.alanBinding,
          pane.activity?.source.kind != .alan
    else {
        return nil
    }

    let title = binding.pendingRequest ? "Input" : shellVisibleLabel(binding.machineState)
    guard let title else { return nil }
    return ShellPaneTitleBarDetailProjection(
        id: "alan",
        title: title,
        help: "Alan Machine \(binding.machineState)"
    )
}

private func shellProcessDuplicatesAgentOrAlan(_ program: String, pane: ShellPane) -> Bool {
    let lowercasedProgram = program.lowercased()
    if pane.alanBinding != nil {
        return lowercasedProgram.contains("alan")
    }

    guard let activity = pane.activity else { return false }
    switch activity.source.kind {
    case .codex:
        return lowercasedProgram.contains("codex")
    case .claude:
        return lowercasedProgram.contains("claude")
    case .openCode:
        return lowercasedProgram.contains("opencode") || lowercasedProgram.contains("open-code")
    case .alan:
        return lowercasedProgram.contains("alan")
    case .shell, .progress, .command, .process, .unknown:
        return false
    }
}

func shellLabelsMatch(_ lhs: String, _ rhs: String) -> Bool {
    lhs.trimmingCharacters(in: .whitespacesAndNewlines)
        .caseInsensitiveCompare(rhs.trimmingCharacters(in: .whitespacesAndNewlines)) == .orderedSame
}


func shellFallbackTitle(for kind: ShellTabKind) -> String {
    switch kind {
    case .terminal:
        return "Terminal"
    case .scratch:
        return "Scratch"
    case .log:
        return "Logs"
    }
}
