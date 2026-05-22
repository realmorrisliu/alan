import Foundation

#if os(macOS)
enum ShellContentRenderKind: String, Codable, CaseIterable, Equatable {
    case terminal
    case markdown
    case settings
    case unavailable
}

struct ShellContentRenderDescriptor: Equatable {
    let renderKind: ShellContentRenderKind
    let contentID: String?
    let contentKind: ShellContentKind?
    let title: String
    let iconName: String
    let capabilities: [ShellContentCapability]
    let payload: ShellContentPayload?
    let rendererPhase: String
    let detail: String?

    var isTerminalSurface: Bool {
        renderKind == .terminal
    }
}

enum ShellContentRenderingRegistry {
    static func descriptor(
        forPaneSlotID paneSlotID: String,
        in contentState: ShellContentStateSnapshot,
        fallbackPane: ShellPane? = nil
    ) -> ShellContentRenderDescriptor {
        let content =
            contentState.contentMounted(in: paneSlotID)
            ?? fallbackPane.map(ShellContentInstance.projectingTerminalPane)
        return descriptor(for: content)
    }

    static func descriptor(for content: ShellContentInstance?) -> ShellContentRenderDescriptor {
        guard let content else {
            return ShellContentRenderDescriptor(
                renderKind: .unavailable,
                contentID: nil,
                contentKind: nil,
                title: "Unavailable",
                iconName: "exclamationmark.triangle",
                capabilities: [],
                payload: nil,
                rendererPhase: "missing_content",
                detail: "No content is mounted in this pane."
            )
        }

        return ShellContentRenderDescriptor(
            renderKind: renderKind(for: content.kind),
            contentID: content.contentID,
            contentKind: content.kind,
            title: content.title,
            iconName: iconName(for: content),
            capabilities: content.capabilities,
            payload: content.payload,
            rendererPhase: content.rendererState.phase,
            detail: content.rendererState.detail
        )
    }

    private static func renderKind(for contentKind: ShellContentKind) -> ShellContentRenderKind {
        switch contentKind {
        case .terminal:
            return .terminal
        case .markdown:
            return .markdown
        case .settings:
            return .settings
        }
    }

    private static func iconName(for content: ShellContentInstance) -> String {
        if let iconName = content.iconName,
           !iconName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            return iconName
        }

        switch content.kind {
        case .terminal:
            return "terminal"
        case .markdown:
            return "doc.text"
        case .settings:
            return "gearshape"
        }
    }
}
#endif
