import SwiftUI

#if os(macOS)
import AppKit

struct TerminalHostView: NSViewRepresentable {
    let pane: ShellPane?
    let terminalContentMount: TerminalContentMount?
    let bootProfile: AlanShellBootProfile?
    let isSelected: Bool
    let runtimeRegistry: TerminalRuntimeRegistry
    let activationDelegate: TerminalHostActivationDelegate?
    let onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?
    let onCommandInput: (() -> Void)?
    let onCloseRequest: ((Bool) -> Void)?
    let onRuntimeUpdate: (TerminalHostRuntimeSnapshot) -> Void
    let onMetadataUpdate: (TerminalPaneMetadataSnapshot) -> Void

    func makeNSView(context: Context) -> AlanTerminalHostNSView {
        runtimeRegistry.hostView(
            forTerminalContent: terminalContentMount,
            pane: pane,
            bootProfile: bootProfile,
            isSelected: isSelected,
            activationDelegate: activationDelegate,
            onShellAction: onShellAction,
            onCommandInput: onCommandInput,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
    }

    func updateNSView(_ nsView: AlanTerminalHostNSView, context: Context) {
        nsView.configure(
            pane: pane,
            terminalContentID: terminalContentMount?.contentID,
            bootProfile: bootProfile,
            isSelected: isSelected,
            surfaceHandle: terminalContentMount.map {
                runtimeRegistry.surfaceHandle(forTerminalContent: $0, bootProfile: bootProfile)
            },
            activationDelegate: activationDelegate,
            onShellAction: onShellAction,
            onCommandInput: onCommandInput,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
    }
}

@MainActor
protocol TerminalHostActivationDelegate: AnyObject {
    func terminalHostDidRequestActivation(paneID: String)
}

func makeCanvasView() -> NSView {
#if canImport(GhosttyKit)
    let view = AlanGhosttyCanvasView(frame: .zero)
#else
    let view = AlanTerminalFallbackCanvasView(frame: .zero)
    view.wantsLayer = true
    view.layer?.backgroundColor = NSColor.clear.cgColor
#endif
    view.translatesAutoresizingMaskIntoConstraints = false
    return view
}

func terminalHostShouldAutoFocusAfterConfigure(
    isSelected: Bool,
    previousPaneID: String?,
    paneID: String?,
    wasSelected: Bool
) -> Bool {
    guard isSelected, paneID != nil else { return false }
    return previousPaneID != paneID || !wasSelected
}

final class AlanTerminalFallbackCanvasView: NSView {
    override var mouseDownCanMoveWindow: Bool { false }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
#endif
