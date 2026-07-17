import Foundation
import SwiftUI

struct ShellTerminalLeafView: View {
    @AppStorage("alanShellDimsInactiveSplitPanes") private var dimsInactiveSplitPanes = true

    let pane: ShellPane
    let bootProfile: AlanShellBootProfile?
    let restoredTranscriptSnapshot: TerminalTranscriptSnapshot?
    let isSelected: Bool
    let renderPriority: TerminalRuntimeRenderPriority
    let isZoomed: Bool
    let canZoom: Bool
    let canMovePane: (ShellPaneSplitDirection) -> Bool
    let canCopyTerminalSelection: Bool
    let canPasteIntoTerminal: Bool
    let canOpenTerminalSearch: Bool
    let runtimeRegistry: TerminalRuntimeRegistry
    let activationDelegate: TerminalHostActivationDelegate?
    let onShellAction: (ShellActionID, ShellActionTarget) -> Void
    let onClearRestoredTranscript: () -> Void
    let onToggleZoom: () -> Void
    let onMovePane: (ShellPaneSplitDirection) -> Void
    let onCopyTerminalSelection: () -> Void
    let onPasteIntoTerminal: () -> Void
    let onOpenTerminalSearch: () -> Void
    let onClosePane: () -> Void
    let onTerminalRuntimeExit: () -> Void
    let onRuntimeUpdate: (TerminalHostRuntimeSnapshot) -> Void
    let onMetadataUpdate: (TerminalPaneMetadataSnapshot) -> Void

    var body: some View {
        VStack(spacing: 0) {
            ShellPaneTitleBarView(
                title: shellPaneTitleBarTitle(for: pane),
                pane: pane,
                isSelected: isSelected,
                isZoomed: isZoomed,
                canZoom: canZoom,
                canMovePane: canMovePane,
                canCopyTerminalSelection: canCopyTerminalSelection,
                canPasteIntoTerminal: canPasteIntoTerminal,
                canOpenTerminalSearch: canOpenTerminalSearch,
                onFocusPane: {
                    activationDelegate?.terminalHostDidRequestActivation(paneID: pane.paneID)
                },
                onToggleZoom: onToggleZoom,
                onMovePane: onMovePane,
                onCopyTerminalSelection: onCopyTerminalSelection,
                onPasteIntoTerminal: onPasteIntoTerminal,
                onOpenTerminalSearch: onOpenTerminalSearch,
                onClosePane: onClosePane
            )

            ZStack(alignment: .topTrailing) {
                VStack(spacing: 0) {
                    if let restoredTranscriptSnapshot {
                        RestoredTerminalTranscriptView(snapshot: restoredTranscriptSnapshot)
                    }

                    TerminalHostView(
                        pane: pane,
                        terminalContentMount: TerminalContentMount(pane: pane),
                        bootProfile: bootProfile,
                        isSelected: isSelected,
                        renderPriority: renderPriority,
                        runtimeRegistry: runtimeRegistry,
                        activationDelegate: activationDelegate,
                        onShellAction: onShellAction,
                        onClearRestoredTranscript: onClearRestoredTranscript,
                        onCloseRequest: { requiresConfirmation in
                            guard !requiresConfirmation else { return }
                            onTerminalRuntimeExit()
                        },
                        onRuntimeUpdate: onRuntimeUpdate,
                        onMetadataUpdate: onMetadataUpdate
                    )
                    .id(pane.paneID)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
                .background(ShellPalette.terminal)
                .overlay {
                    ShellInactivePaneDim(
                        isSelected: isSelected,
                        isEnabled: dimsInactiveSplitPanes
                    )
                }

                if isSelected,
                   let searchState = runtimeRegistry.snapshot(for: pane.paneID).surfaceState.search,
                   searchState.isActive
                {
                    ShellFindBarView(
                        searchState: searchState,
                        onQueryChange: { query in
                            _ = runtimeRegistry.updateFindQuery(for: pane.paneID, query: query)
                        },
                        onNext: {
                            runtimeRegistry.selectNextFindMatch(for: pane.paneID)
                        },
                        onPrevious: {
                            runtimeRegistry.selectPreviousFindMatch(for: pane.paneID)
                        },
                        onClose: {
                            runtimeRegistry.dismissFindInteraction(for: pane.paneID)
                        }
                    )
                    .padding(10)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}
private struct RestoredTerminalTranscriptView: View {
    let snapshot: TerminalTranscriptSnapshot

    private var lines: [String] {
        let boundedLines = snapshot.boundedForManifest().transcriptLines
        guard boundedLines.contains(where: { !$0.trimmingCharacters(in: .whitespaces).isEmpty })
        else {
            return []
        }
        return boundedLines
    }

    private var presentation: RestoredTerminalTranscriptPanelPresentation {
        RestoredTerminalTranscriptPanelPresentation(snapshot: snapshot)
    }

    var body: some View {
        if !lines.isEmpty {
            GeometryReader { proxy in
                ScrollView([.vertical, .horizontal]) {
                    HStack(alignment: .top, spacing: 0) {
                        Text(presentation.transcriptText)
                            .font(
                                .system(
                                    size: presentation.fontSize,
                                    weight: .regular,
                                    design: .monospaced
                                )
                            )
                            .foregroundStyle(Color.white.opacity(0.72))
                            .textSelection(.enabled)
                            .fixedSize(horizontal: true, vertical: false)
                        Spacer(minLength: 0)
                    }
                    .padding(.leading, presentation.leadingInset)
                    .padding(.trailing, presentation.trailingInset)
                    .padding(.vertical, presentation.verticalInset)
                    .frame(minWidth: proxy.size.width, alignment: .topLeading)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            .frame(
                maxWidth: .infinity,
                minHeight: presentation.height,
                maxHeight: presentation.height,
                alignment: .topLeading
            )
            .background(ShellPalette.terminal)
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(ShellPalette.line.opacity(0.20))
                    .frame(height: 0.6)
            }
            .accessibilityIdentifier("restored-terminal-transcript")
        }
    }
}
