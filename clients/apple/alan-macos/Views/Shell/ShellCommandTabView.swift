import SwiftUI

#if os(macOS)
private enum ShellCommandInputAction: CaseIterable {
    case newTerminalTab
    case newAlanTab
    case splitRight
    case splitDown
    case splitLeft
    case splitUp
    case focusLeft
    case focusRight
    case focusUp
    case focusDown
    case equalizeSplits
    case zoomPane
    case movePaneLeft
    case movePaneRight
    case movePaneUp
    case movePaneDown
    case closePane
    case closeTab
    case quickTerminalToggle
    case quickTerminalShow
    case quickTerminalHide
    case quickTerminalFocus
    case quickTerminalClose
    case jumpToAttention
    case copyTerminalSelection
    case pasteIntoTerminal
    case searchTerminal
    case previousPrompt
    case nextPrompt
    case copyLastCommandOutput
    case searchLastCommandOutput

    var aliases: Set<String> {
        switch self {
        case .newTerminalTab:
            return ["new tab", "new terminal tab", "open tab", "open terminal tab"]
        case .newAlanTab:
            return ["new alan tab", "open alan tab", "open in alan", "alan tab"]
        case .splitRight:
            return ["split right", "split pane right"]
        case .splitDown:
            return ["split down", "split below", "split pane down"]
        case .splitLeft:
            return ["split left", "split pane left"]
        case .splitUp:
            return ["split up", "split above", "split pane up"]
        case .focusLeft:
            return ["focus left", "focus pane left", "pane left"]
        case .focusRight:
            return ["focus right", "focus pane right", "pane right"]
        case .focusUp:
            return ["focus up", "focus pane up", "pane up"]
        case .focusDown:
            return ["focus down", "focus pane down", "pane down"]
        case .equalizeSplits:
            return ["equalize splits", "balance splits", "reset splits"]
        case .zoomPane:
            return ["zoom pane", "unzoom pane", "toggle pane zoom", "zoom split"]
        case .movePaneLeft:
            return ["move pane left", "move split left"]
        case .movePaneRight:
            return ["move pane right", "move split right"]
        case .movePaneUp:
            return ["move pane up", "move split up"]
        case .movePaneDown:
            return ["move pane down", "move split down"]
        case .closePane:
            return ["close pane", "close focused pane"]
        case .closeTab:
            return ["close tab", "close current tab"]
        case .quickTerminalToggle:
            return ["quick terminal", "toggle quick terminal"]
        case .quickTerminalShow:
            return ["show quick terminal", "open quick terminal"]
        case .quickTerminalHide:
            return ["hide quick terminal", "dismiss quick terminal"]
        case .quickTerminalFocus:
            return ["focus quick terminal"]
        case .quickTerminalClose:
            return ["close quick terminal"]
        case .jumpToAttention:
            return ["jump to attention", "focus attention", "attention"]
        case .copyTerminalSelection:
            return ["copy", "copy selection", "copy terminal selection"]
        case .pasteIntoTerminal:
            return ["paste", "paste into terminal"]
        case .searchTerminal:
            return ["find", "search", "terminal search", "find in terminal"]
        case .previousPrompt:
            return ["previous prompt", "jump previous prompt", "go previous prompt"]
        case .nextPrompt:
            return ["next prompt", "jump next prompt", "go next prompt"]
        case .copyLastCommandOutput:
            return ["copy last output", "copy last command output", "copy command output"]
        case .searchLastCommandOutput:
            return ["search last output", "search last command output", "find last output"]
        }
    }

    var requiresSemanticCommandBoundaries: Bool {
        switch self {
        case .previousPrompt, .nextPrompt, .copyLastCommandOutput, .searchLastCommandOutput:
            return true
        default:
            return false
        }
    }

    var requiresTerminalContent: Bool {
        switch self {
        case .copyTerminalSelection, .pasteIntoTerminal, .searchTerminal,
             .previousPrompt, .nextPrompt, .copyLastCommandOutput, .searchLastCommandOutput:
            return true
        default:
            return false
        }
    }

    static func resolve(
        _ rawValue: String,
        terminalCommandsAvailable: Bool = false,
        semanticCommandsAvailable: Bool = false
    ) -> ShellCommandInputAction? {
        let normalized = normalizedCommand(rawValue)
        guard !normalized.isEmpty else { return nil }
        return allCases.first {
            (!$0.requiresTerminalContent || terminalCommandsAvailable)
                && (!$0.requiresSemanticCommandBoundaries || semanticCommandsAvailable)
                && $0.aliases.contains(normalized)
        }
    }

    private static func normalizedCommand(_ rawValue: String) -> String {
        rawValue
            .lowercased()
            .replacingOccurrences(of: "-", with: " ")
            .replacingOccurrences(of: "_", with: " ")
            .split(whereSeparator: \.isWhitespace)
            .joined(separator: " ")
    }
}

struct ShellCommandTabView: View {
    @ObservedObject var host: ShellHostController
    @Binding var isPresented: Bool
    var isActive = true
    @State private var query = ""
    @State private var unresolvedMessage: String?
    @State private var unresolvedAttemptID = 0
    @FocusState private var isQueryFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            commandInputBar

            if let unresolvedMessage {
                Text(unresolvedMessage)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(ShellPalette.mutedInk)
                    .padding(.leading, 27)
                    .transition(.opacity)
            }
        }
        .frame(width: 560, alignment: .leading)
        .offset(x: unresolvedAttemptID.isMultiple(of: 2) ? 0 : 1.5)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: unresolvedAttemptID)
        .onAppear {
            updateActiveState(isActive)
        }
        .onChange(of: isActive) { _, active in
            updateActiveState(active)
        }
        .onDisappear {
            host.setCommandInputActive(false)
        }
        .onExitCommand {
            dismissAndRestoreFocus()
        }
    }

    private var commandInputBar: some View {
        ZStack {
            GlassEffectContainer(spacing: 10) {
                ShellCommandPaletteGlassSurface()
            }

            commandInputContent
                .padding(.horizontal, 18)
        }
        .frame(width: 560, height: 56, alignment: .leading)
        .shellShadow(ShellShadows.commandPalette)
    }

    private var commandInputContent: some View {
        HStack(alignment: .center, spacing: 12) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.primary)

            TextField(
                "",
                text: $query,
                prompt: Text(commandPrompt)
                    .foregroundStyle(.secondary)
            )
            .textFieldStyle(.plain)
            .font(.system(size: 17, weight: .medium))
            .foregroundStyle(.primary)
            .focused($isQueryFocused)
            .onChange(of: query) { _, _ in
                unresolvedMessage = nil
            }
            .onSubmit {
                submit()
            }
            .onKeyPress(.return) {
                submit()
                return .handled
            }

            Button {
                dismissAndRestoreFocus()
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(.secondary)
                    .frame(width: 26, height: 26)
                    .contentShape(
                        RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                    )
            }
            .buttonStyle(.plain)
            .help("Close command input")
        }
    }

    private func submit() {
        guard let action = ShellCommandInputAction.resolve(
            query,
            terminalCommandsAvailable: host.focusedContentSupportsTerminalCommands,
            semanticCommandsAvailable: host.focusedPaneHasReliableSemanticCommands
        ) else {
            unresolvedMessage = query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "Type a command name."
                : "No matching command."
            unresolvedAttemptID += 1
            isQueryFocused = true
            return
        }

        perform(action)
    }

    private func perform(_ action: ShellCommandInputAction) {
        switch action {
        case .newTerminalTab:
            host.performShellWorkspaceCommand(.newTerminalTab)
        case .newAlanTab:
            host.performShellWorkspaceCommand(.newAlanTab)
        case .splitRight:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.splitRight)
        case .splitDown:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.splitDown)
        case .splitLeft:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.splitLeft)
        case .splitUp:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.splitUp)
        case .focusLeft:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.focusLeft)
        case .focusRight:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.focusRight)
        case .focusUp:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.focusUp)
        case .focusDown:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.focusDown)
        case .equalizeSplits:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.equalizeSplits)
        case .zoomPane:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.togglePaneZoom)
        case .movePaneLeft:
            host.moveSelectedPaneWithinTab(.left)
        case .movePaneRight:
            host.moveSelectedPaneWithinTab(.right)
        case .movePaneUp:
            host.moveSelectedPaneWithinTab(.up)
        case .movePaneDown:
            host.moveSelectedPaneWithinTab(.down)
        case .closePane:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.closePane)
        case .closeTab:
            host.performShellWorkspaceCommand(ShellWorkspaceCommand.closeTab)
        case .quickTerminalToggle:
            host.performShellWorkspaceCommand(.quickTerminalToggle)
        case .quickTerminalShow:
            host.performShellWorkspaceCommand(.quickTerminalShow)
        case .quickTerminalHide:
            host.performShellWorkspaceCommand(.quickTerminalHide)
        case .quickTerminalFocus:
            host.performShellWorkspaceCommand(.quickTerminalFocus)
        case .quickTerminalClose:
            host.performShellWorkspaceCommand(.quickTerminalClose)
        case .jumpToAttention:
            if let firstAttention = host.attentionItems.first {
                host.focusAttentionItem(firstAttention)
            }
        case .copyTerminalSelection:
            host.copyTerminalSelection(source: .commandUI)
        case .pasteIntoTerminal:
            host.pasteIntoTerminalFromPasteboard(source: .commandUI)
        case .searchTerminal:
            host.openTerminalSearch(source: .commandUI)
        case .previousPrompt:
            host.jumpToPreviousPrompt()
        case .nextPrompt:
            host.jumpToNextPrompt()
        case .copyLastCommandOutput:
            host.copyLastCommandOutput()
        case .searchLastCommandOutput:
            host.searchLastCommandOutput()
        }
        dismissAndRestoreFocus()
    }

    private func updateActiveState(_ active: Bool) {
        host.setCommandInputActive(active)
        if active {
            query = ""
            unresolvedMessage = nil
            unresolvedAttemptID = 0
            DispatchQueue.main.async {
                isQueryFocused = true
            }
        } else {
            isQueryFocused = false
            unresolvedMessage = nil
        }
    }

    private func dismissAndRestoreFocus() {
        withAnimation(commandInputAnimation) {
            isPresented = false
        }
        DispatchQueue.main.async {
            host.refocusSelectedTerminalPane()
        }
    }

    private var commandInputAnimation: Animation? {
        reduceMotion ? nil : .easeOut(duration: 0.14)
    }

    private var commandPrompt: String {
        let contentState = host.shellState.contentStateProjection()
        guard let content = contentState.focusedContent else {
            return "Command..."
        }
        return "Command for \(content.title) · \(shellContentTypeHint(for: content.kind))"
    }
}

private struct ShellCommandPaletteGlassSurface: View {
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let shape = Capsule()

        if reduceTransparency {
            fallbackSurface(shape: shape)
        } else {
            Color.clear
                .clipShape(shape)
                .glassEffect(.regular.interactive(), in: shape)
                .glassEffectTransition(.identity)
        }
    }

    private func fallbackSurface(shape: Capsule) -> some View {
        ZStack {
            ShellMaterialBackgroundView(.floatingOverlay)
                .clipShape(shape)

            shape
                .fill(ShellPalette.commandGlassTint.opacity(colorScheme == .light ? 0.18 : 0.10))

            shape
                .fill(
                    LinearGradient(
                        colors: [
                            Color.white.opacity(colorScheme == .light ? 0.44 : 0.12),
                            Color.white.opacity(colorScheme == .light ? 0.06 : 0.02),
                            ShellPalette.sidebarInk.opacity(colorScheme == .light ? 0.035 : 0.10),
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )

            shape
                .strokeBorder(ShellPalette.line.opacity(colorScheme == .light ? 0.28 : 0.30), lineWidth: 0.8)

            shape
                .strokeBorder(Color.white.opacity(colorScheme == .light ? 0.50 : 0.16), lineWidth: 0.65)
                .mask {
                    shape.fill(
                        LinearGradient(
                            colors: [
                                Color.white,
                                Color.white.opacity(0),
                            ],
                            startPoint: .top,
                            endPoint: .center
                        )
                    )
                }
        }
    }
}
#endif
