import SwiftUI

#if os(macOS)
import AppKit

struct TerminalHostView: NSViewRepresentable {
    let pane: ShellPane?
    let terminalContentMount: TerminalContentMount?
    let bootProfile: AlanShellBootProfile?
    let isSelected: Bool
    let renderPriority: TerminalRuntimeRenderPriority
    let runtimeRegistry: TerminalRuntimeRegistry
    let activationDelegate: TerminalHostActivationDelegate?
    var attachmentPolicy: TerminalHostAttachmentPolicy = .immediate
    let onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?
    var onClearRestoredTranscript: (() -> Void)? = nil
    let onCloseRequest: ((Bool) -> Void)?
    let onRuntimeUpdate: (TerminalHostRuntimeSnapshot) -> Void
    let onMetadataUpdate: (TerminalPaneMetadataSnapshot) -> Void

    func makeNSView(context: Context) -> AlanTerminalHostNSView {
        runtimeRegistry.hostView(
            forTerminalContent: terminalContentMount,
            pane: pane,
            bootProfile: bootProfile,
            isSelected: isSelected,
            renderPriority: renderPriority,
            activationDelegate: activationDelegate,
            attachmentPolicy: attachmentPolicy,
            onShellAction: onShellAction,
            onClearRestoredTranscript: onClearRestoredTranscript,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
    }

    func updateNSView(_ nsView: AlanTerminalHostNSView, context: Context) {
        runtimeRegistry.configureHostView(
            nsView,
            forTerminalContent: terminalContentMount,
            pane: pane,
            bootProfile: bootProfile,
            isSelected: isSelected,
            renderPriority: renderPriority,
            activationDelegate: activationDelegate,
            attachmentPolicy: attachmentPolicy,
            onShellAction: onShellAction,
            onClearRestoredTranscript: onClearRestoredTranscript,
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
