import SwiftUI

#if os(macOS)
import AppKit
import UniformTypeIdentifiers

struct AlanMacShellCommands: Commands {
    @ObservedObject var host: ShellHostController
    @ObservedObject var updateController: AlanMacUpdateController
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(after: .appInfo) {
            Button("Check for Updates...") {
                updateController.checkForUpdates()
            }
        }

        CommandGroup(replacing: .newItem) {
            Button("Show alan window") {
                openWindow(id: "main")
                AlanMacPrimaryWindowPresenter.focusExistingWindowSoon()
            }
            .keyboardShortcut("n", modifiers: .command)
        }

        CommandGroup(replacing: .pasteboard) {
            Button("Cut") {
                sendResponderAction(#selector(NSText.cut(_:)))
            }
            .keyboardShortcut("x", modifiers: .command)

            Button("Copy") {
                if !host.copyTerminalSelection(source: .menuBar) {
                    sendResponderAction(#selector(NSText.copy(_:)))
                }
            }
            .keyboardShortcut("c", modifiers: .command)

            Button("Paste") {
                if !host.pasteIntoTerminalFromPasteboard(source: .menuBar) {
                    sendResponderAction(#selector(NSText.paste(_:)))
                }
            }
            .keyboardShortcut("v", modifiers: .command)

            Button("Select All") {
                sendResponderAction(#selector(NSText.selectAll(_:)))
            }
            .keyboardShortcut("a", modifiers: .command)
        }

        CommandMenu("Tools") {
            Button("Install Command Line Tools...") {
                installCommandLineTools()
            }
        }

        CommandGroup(replacing: .appSettings) {
            Button("Settings...") {
                openSettingsTab()
            }
            .keyboardShortcut(",", modifiers: .command)
        }

        CommandMenu("Shell") {
            Button(host.shellActionTitle(.newTerminalTab)) {
                host.performShellAction(.newTerminalTab)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.newTerminalTab))

            Button(host.shellActionTitle(.newAlanTab)) {
                host.performShellAction(.newAlanTab)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.newAlanTab))

            Button("Open Markdown...") {
                openMarkdownFile()
            }

            Button("Open Settings") {
                openSettingsTab()
            }

            Button("Ask alan...") {
                host.requestCommandInput()
            }
            .keyboardShortcut("p", modifiers: .command)

            Button("Find") {
                host.openTerminalSearch(source: .menuBar)
            }
            .keyboardShortcut("f", modifiers: .command)
            .disabled(!host.canOpenTerminalSearch(source: .menuBar))

            Button(host.shellActionTitle(.quickTerminalToggle)) {
                host.performShellAction(.quickTerminalToggle)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.quickTerminalToggle))

            Divider()

            Button(host.shellActionTitle(.paneSplitRight)) {
                host.performShellAction(.paneSplitRight)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneSplitRight))
            .disabled(!host.shellActionAvailability(.paneSplitRight).isAvailable)

            Button(host.shellActionTitle(.paneSplitDown)) {
                host.performShellAction(.paneSplitDown)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneSplitDown))
            .disabled(!host.shellActionAvailability(.paneSplitDown).isAvailable)

            Button(host.shellActionTitle(.paneSplitLeft)) {
                host.performShellAction(.paneSplitLeft)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneSplitLeft))
            .disabled(!host.shellActionAvailability(.paneSplitLeft).isAvailable)

            Button(host.shellActionTitle(.paneSplitUp)) {
                host.performShellAction(.paneSplitUp)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneSplitUp))
            .disabled(!host.shellActionAvailability(.paneSplitUp).isAvailable)

            Button(host.shellActionTitle(.paneEqualizeSplits)) {
                host.performShellAction(.paneEqualizeSplits)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneEqualizeSplits))

            Button(host.shellActionTitle(.paneZoomToggle)) {
                host.performShellAction(.paneZoomToggle)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneZoomToggle))
            .disabled(!host.shellActionAvailability(.paneZoomToggle).isAvailable)

            Divider()

            Button(host.shellActionTitle(.paneMoveLeft)) {
                host.performShellAction(.paneMoveLeft)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneMoveLeft))
            .disabled(!host.shellActionAvailability(.paneMoveLeft).isAvailable)

            Button(host.shellActionTitle(.paneMoveRight)) {
                host.performShellAction(.paneMoveRight)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneMoveRight))
            .disabled(!host.shellActionAvailability(.paneMoveRight).isAvailable)

            Button(host.shellActionTitle(.paneMoveUp)) {
                host.performShellAction(.paneMoveUp)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneMoveUp))
            .disabled(!host.shellActionAvailability(.paneMoveUp).isAvailable)

            Button(host.shellActionTitle(.paneMoveDown)) {
                host.performShellAction(.paneMoveDown)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneMoveDown))
            .disabled(!host.shellActionAvailability(.paneMoveDown).isAvailable)

            Divider()

            Button(host.shellActionTitle(.paneFocusLeft)) {
                host.performShellAction(.paneFocusLeft)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneFocusLeft))
            .disabled(!host.shellActionAvailability(.paneFocusLeft).isAvailable)

            Button(host.shellActionTitle(.paneFocusRight)) {
                host.performShellAction(.paneFocusRight)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneFocusRight))
            .disabled(!host.shellActionAvailability(.paneFocusRight).isAvailable)

            Button(host.shellActionTitle(.paneFocusUp)) {
                host.performShellAction(.paneFocusUp)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneFocusUp))
            .disabled(!host.shellActionAvailability(.paneFocusUp).isAvailable)

            Button(host.shellActionTitle(.paneFocusDown)) {
                host.performShellAction(.paneFocusDown)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneFocusDown))
            .disabled(!host.shellActionAvailability(.paneFocusDown).isAvailable)

            Divider()

            Button(host.shellActionTitle(.tabSelectPrevious)) {
                host.performShellAction(.tabSelectPrevious)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.tabSelectPrevious))
            .disabled(!host.shellActionAvailability(.tabSelectPrevious).isAvailable)

            Button(host.shellActionTitle(.tabSelectNext)) {
                host.performShellAction(.tabSelectNext)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.tabSelectNext))
            .disabled(!host.shellActionAvailability(.tabSelectNext).isAvailable)

            Button(host.shellActionTitle(.tabMoveLeft)) {
                host.performShellAction(.tabMoveLeft)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.tabMoveLeft))
            .disabled(!host.shellActionAvailability(.tabMoveLeft).isAvailable)

            Button(host.shellActionTitle(.tabMoveRight)) {
                host.performShellAction(.tabMoveRight)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.tabMoveRight))
            .disabled(!host.shellActionAvailability(.tabMoveRight).isAvailable)

            Divider()

            Button(host.shellActionTitle(.paneClose)) {
                host.performShellAction(.paneClose)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.paneClose))
            .disabled(!host.shellActionAvailability(.paneClose).isAvailable)

            Button(host.shellActionTitle(.tabClose)) {
                host.performShellAction(.tabClose)
            }
            .shellActionKeyboardShortcut(host.shellActionShortcut(.tabClose))
            .disabled(!host.shellActionAvailability(.tabClose).isAvailable)
        }
    }

    private func sendResponderAction(_ selector: Selector) {
        NSApp.sendAction(selector, to: nil, from: nil)
    }

    private func openMarkdownFile() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = markdownContentTypes

        guard panel.runModal() == .OK,
              let fileURL = panel.url
        else { return }

        _ = host.openMarkdownTab(fileURL: fileURL)
    }

    private func openSettingsTab() {
        openWindow(id: "main")
        _ = host.openSettingsTab()
        AlanMacPrimaryWindowPresenter.focusExistingWindowSoon()
    }

    private var markdownContentTypes: [UTType] {
        ["md", "markdown", "mdown"].compactMap { UTType(filenameExtension: $0) }
    }

    private func installCommandLineTools() {
        do {
            let records = try AlanCommandLineToolInstaller.install()
            let installed = records.filter { $0.status == .installed }
            let skipped = records.compactMap { record -> String? in
                guard case .skipped(let reason) = record.status else {
                    return nil
                }
                return "\(record.tool): \(reason)\n\(record.targetPath)"
            }

            let alert = NSAlert()
            alert.messageText = skipped.isEmpty
                ? "Command line tools installed"
                : "Command line tools partially installed"
            alert.informativeText = (
                installed.map { "\($0.tool) -> \($0.targetPath)" } +
                skipped.map { "Skipped \($0)" }
            ).joined(separator: "\n\n")
            alert.alertStyle = skipped.isEmpty ? .informational : .warning
            alert.addButton(withTitle: "OK")
            alert.runModal()
        } catch {
            let targetDirectory = AlanCommandLineToolInstaller.defaultInstallDirectory.path
            let alert = NSAlert()
            alert.messageText = "Command line tools were not installed"
            alert.informativeText = """
            alan tried to create command links in \(targetDirectory).

            \(error.localizedDescription)

            Choose a writable PATH directory or install with Homebrew cask.
            """
            alert.alertStyle = .warning
            alert.addButton(withTitle: "OK")
            alert.runModal()
        }
    }
}

extension View {
    @ViewBuilder
    func shellActionKeyboardShortcut(_ shortcut: ShellActionShortcut?) -> some View {
        if let shortcut,
           let keyEquivalent = shortcut.swiftUIKeyEquivalent
        {
            keyboardShortcut(keyEquivalent, modifiers: shortcut.swiftUIModifiers)
        } else {
            self
        }
    }
}

extension ShellActionShortcut {
    var swiftUIKeyEquivalent: KeyEquivalent? {
        switch key {
        case "leftArrow":
            return .leftArrow
        case "rightArrow":
            return .rightArrow
        case "upArrow":
            return .upArrow
        case "downArrow":
            return .downArrow
        case "space":
            return .space
        case "return":
            return .return
        default:
            guard key.count == 1, let character = key.first else { return nil }
            return KeyEquivalent(character)
        }
    }

    var swiftUIModifiers: EventModifiers {
        modifiers.reduce(into: EventModifiers()) { result, modifier in
            switch modifier {
            case .command:
                result.insert(.command)
            case .option:
                result.insert(.option)
            case .shift:
                result.insert(.shift)
            case .control:
                result.insert(.control)
            }
        }
    }
}
#endif
