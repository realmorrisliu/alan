import Foundation

struct ShellSidebarTabProjection: Equatable {
    let title: String
    let secondaryLine: String
    let activity: TerminalActivitySnapshot?
    let progress: TerminalActivityProgress?
    let accessibilityActivityLabel: String?
}

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

struct ShellPaneTitleBarDetailProjection: Equatable, Identifiable {
    let id: String
    let title: String
    let help: String
}

struct ShellTabPaneSummary: Equatable {
    let topology: ShellSidebarPaneTopology
    let focusedPaneID: String?

    init(
        paneTree: ShellPaneTreeNode,
        visiblePaneIDs: [String],
        focusedPaneID: String?
    ) {
        topology = ShellSidebarPaneTopology.classify(
            paneTree: paneTree,
            visiblePaneIDs: visiblePaneIDs
        )
        self.focusedPaneID = visiblePaneIDs.contains(focusedPaneID ?? "") ? focusedPaneID : nil
    }

    var paneIDs: [String] {
        topology.paneIDs
    }

    var paneCount: Int {
        topology.paneCount
    }

    var isComplex: Bool {
        topology.isComplex
    }

    var accessibilityTopologyLabel: String {
        topology.userFacingDescription
    }

    func nextPaneID(after currentPaneID: String?) -> String? {
        guard !paneIDs.isEmpty else { return nil }
        guard let currentPaneID,
              let currentIndex = paneIDs.firstIndex(of: currentPaneID)
        else {
            return paneIDs.first
        }

        return paneIDs[(currentIndex + 1) % paneIDs.count]
    }
}

struct ShellSidebarPaneTopology: Equatable {
    let kind: ShellSidebarPaneTopologyKind
    let paneIDs: [String]

    var paneCount: Int {
        paneIDs.count
    }

    var isComplex: Bool {
        if case .complex = kind {
            return true
        }
        return false
    }

    var userFacingDescription: String {
        switch kind {
        case .single:
            return "Single pane"
        case .columns(let count):
            return "\(count)-column split"
        case .rows(let count):
            return "\(count)-row split"
        case .mainLeftWithRightStack:
            return "Main pane left with right stack"
        case .mainRightWithLeftStack:
            return "Main pane right with left stack"
        case .mainTopWithBottomSplit:
            return "Main pane top with bottom split"
        case .mainBottomWithTopSplit:
            return "Main pane bottom with top split"
        case .grid2x2:
            return "2 by 2 grid split"
        case .complex(let count):
            return "Complex split, \(count) panes"
        }
    }

    static func classify(
        paneTree: ShellPaneTreeNode,
        visiblePaneIDs: [String]
    ) -> ShellSidebarPaneTopology {
        let orderedVisiblePaneIDs = paneTree.paneIDs.filter { visiblePaneIDs.contains($0) }
        guard !orderedVisiblePaneIDs.isEmpty,
              let normalizedTree = ShellSidebarPaneTopologyNode(
                  paneTree: paneTree,
                  visiblePaneIDs: Set(orderedVisiblePaneIDs)
              )
        else {
            return ShellSidebarPaneTopology(kind: .complex(count: 0), paneIDs: [])
        }

        let paneIDs = normalizedTree.paneIDs
        guard paneIDs.count > 1 else {
            return ShellSidebarPaneTopology(kind: .single, paneIDs: paneIDs)
        }

        if let splitDirection = normalizedTree.splitDirection,
           let flattened = normalizedTree.flattenedPaneIDs(along: splitDirection),
           flattened.count == paneIDs.count,
           (2...4).contains(flattened.count)
        {
            let kind: ShellSidebarPaneTopologyKind = splitDirection == .vertical
                ? .columns(count: flattened.count)
                : .rows(count: flattened.count)
            return ShellSidebarPaneTopology(kind: kind, paneIDs: flattened)
        }

        if let mainStackKind = Self.mainStackKind(for: normalizedTree) {
            return ShellSidebarPaneTopology(kind: mainStackKind, paneIDs: paneIDs)
        }

        if let gridKind = Self.gridKind(for: normalizedTree) {
            return ShellSidebarPaneTopology(kind: gridKind, paneIDs: paneIDs)
        }

        return ShellSidebarPaneTopology(kind: .complex(count: paneIDs.count), paneIDs: paneIDs)
    }

    private static func mainStackKind(
        for node: ShellSidebarPaneTopologyNode
    ) -> ShellSidebarPaneTopologyKind? {
        guard case .split(let direction, let children) = node,
              children.count == 2
        else {
            return nil
        }

        let first = children[0]
        let second = children[1]

        switch direction {
        case .vertical:
            if first.leafPaneID != nil,
               second.flattenedPaneIDs(along: .horizontal)?.count == 2
            {
                return .mainLeftWithRightStack
            }
            if first.flattenedPaneIDs(along: .horizontal)?.count == 2,
               second.leafPaneID != nil
            {
                return .mainRightWithLeftStack
            }
        case .horizontal:
            if first.leafPaneID != nil,
               second.flattenedPaneIDs(along: .vertical)?.count == 2
            {
                return .mainTopWithBottomSplit
            }
            if first.flattenedPaneIDs(along: .vertical)?.count == 2,
               second.leafPaneID != nil
            {
                return .mainBottomWithTopSplit
            }
        }

        return nil
    }

    private static func gridKind(
        for node: ShellSidebarPaneTopologyNode
    ) -> ShellSidebarPaneTopologyKind? {
        guard case .split(let direction, let children) = node,
              children.count == 2
        else {
            return nil
        }

        let childDirection: ShellSplitDirection = direction == .vertical ? .horizontal : .vertical
        guard children.allSatisfy({ $0.flattenedPaneIDs(along: childDirection)?.count == 2 }) else {
            return nil
        }

        return .grid2x2(rootDirection: direction)
    }
}

enum ShellSidebarPaneTopologyKind: Equatable {
    case single
    case columns(count: Int)
    case rows(count: Int)
    case mainLeftWithRightStack
    case mainRightWithLeftStack
    case mainTopWithBottomSplit
    case mainBottomWithTopSplit
    case grid2x2(rootDirection: ShellSplitDirection)
    case complex(count: Int)
}

private enum ShellSidebarPaneTopologyNode: Equatable {
    case pane(String)
    case split(direction: ShellSplitDirection, children: [ShellSidebarPaneTopologyNode])

    init?(
        paneTree: ShellPaneTreeNode,
        visiblePaneIDs: Set<String>
    ) {
        switch paneTree.kind {
        case .pane:
            guard let paneID = paneTree.paneID,
                  visiblePaneIDs.contains(paneID)
            else {
                return nil
            }
            self = .pane(paneID)
        case .split:
            let visibleChildren = (paneTree.children ?? []).compactMap {
                ShellSidebarPaneTopologyNode(paneTree: $0, visiblePaneIDs: visiblePaneIDs)
            }
            guard visibleChildren.count > 1,
                  let direction = paneTree.direction
            else {
                guard let onlyChild = visibleChildren.first else { return nil }
                self = onlyChild
                return
            }
            self = .split(direction: direction, children: visibleChildren)
        }
    }

    var paneIDs: [String] {
        switch self {
        case .pane(let paneID):
            return [paneID]
        case .split(_, let children):
            return children.flatMap(\.paneIDs)
        }
    }

    var splitDirection: ShellSplitDirection? {
        if case .split(let direction, _) = self {
            return direction
        }
        return nil
    }

    var leafPaneID: String? {
        if case .pane(let paneID) = self {
            return paneID
        }
        return nil
    }

    func flattenedPaneIDs(along direction: ShellSplitDirection) -> [String]? {
        switch self {
        case .pane(let paneID):
            return [paneID]
        case .split(let splitDirection, let children):
            guard splitDirection == direction else { return nil }
            var paneIDs: [String] = []
            for child in children {
                guard let childPaneIDs = child.flattenedPaneIDs(along: direction) else {
                    return nil
                }
                paneIDs.append(contentsOf: childPaneIDs)
            }
            return paneIDs
        }
    }
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

    let title = binding.pendingYield ? "Input" : shellVisibleLabel(binding.runStatus)
    guard let title else { return nil }
    return ShellPaneTitleBarDetailProjection(
        id: "alan",
        title: title,
        help: "alan \(binding.runStatus)"
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

private func shellLabelsMatch(_ lhs: String, _ rhs: String) -> Bool {
    lhs.trimmingCharacters(in: .whitespacesAndNewlines)
        .caseInsensitiveCompare(rhs.trimmingCharacters(in: .whitespacesAndNewlines)) == .orderedSame
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
    if pane.alanBinding?.pendingYield == true {
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

func shellSidebarTabProjection(
    for tab: ShellTab,
    panes allPanes: [ShellPane],
    contentState: ShellContentStateSnapshot? = nil,
    focusedPaneID: String?,
    focusedTabID: String?,
    now: Date? = nil
) -> ShellSidebarTabProjection {
    let panes = shellOrderedPanes(for: tab, panes: allPanes)
    let primaryPane = shellPrimaryPane(in: panes, focusedPaneID: focusedPaneID)
    let primaryContent = shellPrimaryContent(in: tab, contentState: contentState, focusedPaneID: focusedPaneID)
    let title = shellSidebarTabTitle(for: tab, primaryPane: primaryPane, primaryContent: primaryContent)
    let isOwningTabFocused = focusedTabID == tab.tabID

    let activityCandidates = panes.enumerated().compactMap { index, pane -> TerminalActivitySnapshot? in
        if let contentState,
           contentState.contentMounted(in: pane.paneID)?.kind != .terminal
        {
            return nil
        }
        guard let activity = pane.activity,
              activity.isSidebarWorthy(at: now, owningTabFocused: isOwningTabFocused)
        else { return nil }

        let hint: String?
        if panes.count > 1,
           pane.paneID != primaryPane?.paneID
        {
            hint = "Pane \(index + 1)"
        } else {
            hint = nil
        }
        return activity.withPaneHint(hint)
    }

    if let activity = TerminalActivitySnapshot.primarySidebarActivity(activityCandidates, now: now) {
        return ShellSidebarTabProjection(
            title: title,
            secondaryLine: activity.display.sourceFirstLabel,
            activity: activity,
            progress: activity.progress,
            accessibilityActivityLabel: activity.display.sourceFirstLabel
        )
    }

    let fallback = shellSidebarContentLine(for: primaryContent)
        ?? primaryPane.flatMap { shellTerminalStatusSummary(for: $0, now: now) }
        ?? primaryPane.flatMap { shellSidebarContextLine(for: $0, title: title) }
        ?? shellFallbackTitle(for: tab.kind)
    return ShellSidebarTabProjection(
        title: title,
        secondaryLine: fallback,
        activity: nil,
        progress: nil,
        accessibilityActivityLabel: nil
    )
}

func shellOrderedPanes(for tab: ShellTab, panes allPanes: [ShellPane]) -> [ShellPane] {
    let byID = Dictionary(uniqueKeysWithValues: allPanes.map { ($0.paneID, $0) })
    let ordered = tab.paneTree.paneIDs.compactMap { byID[$0] }
    if !ordered.isEmpty {
        return ordered
    }
    return allPanes.filter { $0.tabID == tab.tabID }
}

private func shellPrimaryPane(in panes: [ShellPane], focusedPaneID: String?) -> ShellPane? {
    if let focusedPaneID,
       let focused = panes.first(where: { $0.paneID == focusedPaneID })
    {
        return focused
    }
    return panes.first
}

private func shellPrimaryContent(
    in tab: ShellTab,
    contentState: ShellContentStateSnapshot?,
    focusedPaneID: String?
) -> ShellContentInstance? {
    guard let contentState else { return nil }
    if let focusedPaneID,
       tab.contains(paneID: focusedPaneID),
       let focusedContent = contentState.contentMounted(in: focusedPaneID)
    {
        return focusedContent
    }

    return tab.paneTree.paneIDs.lazy.compactMap {
        contentState.contentMounted(in: $0)
    }.first
}

private func shellSidebarTabTitle(
    for tab: ShellTab,
    primaryPane: ShellPane?,
    primaryContent: ShellContentInstance?
) -> String {
    if let primaryContent,
       primaryContent.kind != .terminal
    {
        return primaryContent.title
    }

    return shellDisplayTitle(
        rawTitle: tab.title ?? primaryPane?.viewport?.title,
        workingDirectoryName: primaryPane?.context?.workingDirectoryName,
        cwd: primaryPane?.cwd,
        program: primaryPane?.process?.program,
        launchTarget: primaryPane?.resolvedLaunchTarget ?? .shell,
        fallback: shellFallbackTitle(for: tab.kind)
    )
}

private func shellSidebarContentLine(for content: ShellContentInstance?) -> String? {
    guard let content,
          content.kind != .terminal
    else {
        return nil
    }

    return shellContentTypeHint(for: content.kind)
}

private func shellSidebarContextLine(for pane: ShellPane, title: String) -> String? {
    let contextLabel = shellPathLeaf(pane.context?.repositoryRoot)
        ?? shellVisibleLabel(pane.context?.workingDirectoryName)
        ?? shellPathLeaf(pane.cwd)

    if let branch = shellVisibleLabel(pane.context?.gitBranch) {
        if let contextLabel, contextLabel != title {
            return "\(contextLabel) · \(branch)"
        }
        return branch
    }

    if let contextLabel {
        if contextLabel == title,
           let program = shellVisibleLabel(pane.process?.program)
        {
            return program
        }
        return contextLabel
    }

    return shellVisibleLabel(pane.process?.program)
}

private func shellFallbackTitle(for kind: ShellTabKind) -> String {
    switch kind {
    case .terminal:
        return "Terminal"
    case .scratch:
        return "Scratch"
    case .log:
        return "Logs"
    }
}
