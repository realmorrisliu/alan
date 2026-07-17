import Foundation

struct ShellViewportSnapshot: Codable, Equatable {
    let title: String?
    let summary: String?
    let visibleExcerpt: String?
    let lastActivityAt: String?

    private enum CodingKeys: String, CodingKey {
        case title
        case summary
        case visibleExcerpt = "visible_excerpt"
        case lastActivityAt = "last_activity_at"
    }
}

struct ShellAlanBinding: Codable, Equatable {
    let processPath: String
    let machineState: String
    let pendingRequest: Bool
    let source: String?
    let lastProjectedAt: String?

    private enum CodingKeys: String, CodingKey {
        case processPath = "process_path"
        case machineState = "machine_state"
        case pendingRequest = "pending_request"
        case source
        case lastProjectedAt = "last_projected_at"
    }
}

struct ShellPane: Identifiable, Codable, Equatable {
    let paneID: String
    let tabID: String
    let spaceID: String
    let launchTarget: ShellLaunchTarget?
    let cwd: String?
    let process: ShellProcessBinding?
    let attention: ShellAttentionState
    let context: ShellContextSnapshot?
    let viewport: ShellViewportSnapshot?
    let activity: TerminalActivitySnapshot?
    let alanBinding: ShellAlanBinding?
    let terminalProfileID: String?

    var id: String { paneID }

    init(
        paneID: String,
        tabID: String,
        spaceID: String,
        launchTarget: ShellLaunchTarget?,
        cwd: String?,
        process: ShellProcessBinding?,
        attention: ShellAttentionState,
        context: ShellContextSnapshot?,
        viewport: ShellViewportSnapshot?,
        activity: TerminalActivitySnapshot? = nil,
        alanBinding: ShellAlanBinding?,
        terminalProfileID: String? = nil
    ) {
        self.paneID = paneID
        self.tabID = tabID
        self.spaceID = spaceID
        self.launchTarget = launchTarget
        self.cwd = cwd
        self.process = process
        self.attention = attention
        self.context = context
        self.viewport = viewport
        self.activity = activity
        self.alanBinding = alanBinding
        self.terminalProfileID = terminalProfileID
    }

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case tabID = "tab_id"
        case spaceID = "space_id"
        case launchTarget = "launch_target"
        case cwd
        case process
        case attention
        case context
        case viewport
        case activity
        case alanBinding = "alan_binding"
        case terminalProfileID = "terminal_profile_id"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        // Binding data is a disposable live projection. An unsupported shape must not
        // invalidate otherwise durable workspace topology.
        let alanBinding = try? container.decodeIfPresent(
            ShellAlanBinding.self,
            forKey: .alanBinding
        )
        self.init(
            paneID: try container.decode(String.self, forKey: .paneID),
            tabID: try container.decode(String.self, forKey: .tabID),
            spaceID: try container.decode(String.self, forKey: .spaceID),
            launchTarget: try container.decodeIfPresent(ShellLaunchTarget.self, forKey: .launchTarget),
            cwd: try container.decodeIfPresent(String.self, forKey: .cwd),
            process: try container.decodeIfPresent(ShellProcessBinding.self, forKey: .process),
            attention: try container.decode(ShellAttentionState.self, forKey: .attention),
            context: try container.decodeIfPresent(ShellContextSnapshot.self, forKey: .context),
            viewport: try container.decodeIfPresent(ShellViewportSnapshot.self, forKey: .viewport),
            activity: try container.decodeIfPresent(TerminalActivitySnapshot.self, forKey: .activity),
            alanBinding: alanBinding,
            terminalProfileID: try container.decodeIfPresent(String.self, forKey: .terminalProfileID)
        )
    }
}

extension ShellPane {
    var terminalContentID: String {
        ShellContentInstance.terminalContentID(forPaneID: paneID)
    }
}

extension ShellPane {
    var resolvedLaunchTarget: ShellLaunchTarget {
        launchTarget ?? .shell
    }
}
