import Foundation
import SwiftUI

struct ShellBoundedContentLeafView: View {
    let descriptor: ShellContentRenderDescriptor
    let paneSlotID: String
    let onAgentRendererStateUpdate: (
        AlanAgentStreamOffsets,
        AlanAgentContentPresentation
    ) -> Void
    let onOpenAgentView: (AlanAgentAttachment) -> Void
    let isSelected: Bool
    let isZoomed: Bool
    let canZoom: Bool
    let canMovePane: (ShellPaneSplitDirection) -> Bool
    let onFocusPane: () -> Void
    let onToggleZoom: () -> Void
    let onMovePane: (ShellPaneSplitDirection) -> Void
    let onClosePane: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            ShellContentPaneTitleBarView(
                descriptor: descriptor,
                isSelected: isSelected,
                isZoomed: isZoomed,
                canZoom: canZoom,
                canMovePane: canMovePane,
                onFocusPane: onFocusPane,
                onToggleZoom: onToggleZoom,
                onMovePane: onMovePane,
                onClosePane: onClosePane
            )

            switch descriptor.renderKind {
            case .markdown:
                ShellMarkdownContentView(descriptor: descriptor)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onFocusPane)
            case .settings:
                ShellSettingsContentView(descriptor: descriptor)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onFocusPane)
            case .agent:
                ShellAgentContentView(
                    descriptor: descriptor,
                    onRendererStateUpdate: onAgentRendererStateUpdate,
                    onOpenAnotherView: onOpenAgentView
                )
                    .id(descriptor.contentID)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onFocusPane)
            case .terminal, .unavailable:
                boundedPlaceholder
            }
        }
    }

    private var boundedPlaceholder: some View {
        ZStack {
            ShellPalette.workspace

            VStack(spacing: 10) {
                Image(systemName: descriptor.iconName)
                    .font(.system(size: 22, weight: .medium))
                    .foregroundStyle(ShellPalette.mutedInk)
                    .frame(width: 32, height: 32)

                Text(descriptor.title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(ShellPalette.ink)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: 260)

                Text(contentKindLabel)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(ShellPalette.mutedInk)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .onTapGesture(perform: onFocusPane)
        }
    }

    private var contentKindLabel: String {
        switch descriptor.renderKind {
        case .terminal:
            return "Terminal"
        case .markdown:
            return "Document"
        case .settings:
            return "Settings"
        case .agent:
            return "Agent Process"
        case .unavailable:
            return "Unavailable"
        }
    }
}

private struct ShellAgentContentView: View {
    let descriptor: ShellContentRenderDescriptor
    let onRendererStateUpdate: (
        AlanAgentStreamOffsets,
        AlanAgentContentPresentation
    ) -> Void
    let onOpenAnotherView: (AlanAgentAttachment) -> Void

    @ObservedObject private var hostAttachment = AlanOSAttachmentController.shared
    @State private var output = ""
    @State private var activity: [String] = []
    @State private var continuityNotices: [String] = []
    @State private var processStatus = "Attaching"
    @State private var visibleError: String?
    @State private var input = ""
    @State private var requestResponse = ""
    @State private var pendingRequest: AlanAgentPendingRequest?
    @State private var streamOffsets: AlanAgentStreamOffsets
    @State private var presentation: AlanAgentContentPresentation
    @State private var isConfirmingStop = false

    private var agent: AlanAgentAttachment? { descriptor.payload?.agent }

    init(
        descriptor: ShellContentRenderDescriptor,
        onRendererStateUpdate: @escaping (
            AlanAgentStreamOffsets,
            AlanAgentContentPresentation
        ) -> Void,
        onOpenAnotherView: @escaping (AlanAgentAttachment) -> Void
    ) {
        self.descriptor = descriptor
        self.onRendererStateUpdate = onRendererStateUpdate
        self.onOpenAnotherView = onOpenAnotherView
        _streamOffsets = State(initialValue: descriptor.payload?.agent?.offsets ?? .zero)
        _presentation = State(initialValue: descriptor.payload?.agent?.presentation ?? .default)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Circle()
                    .fill(statusColor)
                    .frame(width: 7, height: 7)
                Text(processStatus)
                    .font(ShellType.pro(ShellType.caption, weight: .medium))
                    .foregroundStyle(ShellPalette.mutedInk)
                Spacer()
                Menu {
                    Toggle("Follow Output", isOn: followOutputBinding)
                    Button("Open Another View", action: openAnotherView)
                    Divider()
                    Button("Compact Context") { performMachineControl("compact") }
                    Button("Roll Back Last Turn") { performMachineControl("rollback") }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .accessibilityLabel("Agent controls")
                .disabled(!isProcessRunning)
                Button("Interrupt") { performInterrupt() }
                    .buttonStyle(.borderless)
                    .disabled(!isProcessRunning)
                Button("Stop…") { isConfirmingStop = true }
                    .buttonStyle(.borderless)
                    .disabled(!isProcessRunning)
            }
            .padding(.horizontal, ShellSpacing.row)
            .frame(height: 34)

            Divider().opacity(0.45)

            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(output.isEmpty ? "No Agent output yet." : output)
                            .font(.system(.body, design: .monospaced))
                            .foregroundStyle(output.isEmpty ? ShellPalette.mutedInk : ShellPalette.ink)
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .topLeading)

                        if !activity.isEmpty {
                            Divider().opacity(0.4)
                            Text("Activity")
                                .font(ShellType.pro(ShellType.monoCaption, weight: .semibold))
                                .textCase(.uppercase)
                                .foregroundStyle(ShellPalette.mutedInk)
                            ForEach(Array(activity.enumerated()), id: \.offset) { entry in
                                Text(entry.element)
                                    .font(ShellType.mono(ShellType.monoLabel))
                                    .foregroundStyle(ShellPalette.mutedInk)
                                    .textSelection(.enabled)
                            }
                        }

                        Color.clear.frame(height: 1).id("agent-output-bottom")
                    }
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                    .padding(ShellSpacing.row)
                }
                .onChange(of: output) { _, _ in
                    guard presentation.followsOutput else { return }
                    proxy.scrollTo("agent-output-bottom", anchor: .bottom)
                }
            }

            if let pendingRequest {
                pendingRequestView(pendingRequest)
            }

            ForEach(Array(continuityNotices.enumerated()), id: \.offset) { notice in
                Text(notice.element)
                    .font(ShellType.pro(ShellType.caption))
                    .foregroundStyle(ShellSignal.action)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, ShellSpacing.row)
                    .padding(.bottom, ShellSpacing.tight)
            }

            if let visibleError {
                Text(visibleError)
                    .font(ShellType.pro(ShellType.caption))
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, ShellSpacing.row)
                    .padding(.bottom, ShellSpacing.control)
            }

            HStack(spacing: 8) {
                TextField("Send input to Agent", text: $input)
                    .textFieldStyle(.plain)
                    .onSubmit(sendInput)
                Button("Send", action: sendInput)
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .disabled(
                        !isProcessRunning
                            || input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    )
            }
            .padding(ShellSpacing.control)
            .background(ShellPalette.workspace)
        }
        .background(ShellPalette.workspace)
        .task(id: refreshIdentity) { await tailAgentFiles() }
        .alert("Stop Agent Process?", isPresented: $isConfirmingStop) {
            Button("Cancel", role: .cancel) {}
            Button("Stop Process", role: .destructive) { stopProcess() }
        } message: {
            Text("This writes an explicit stop action to Alan OS. Closing this view only detaches.")
        }
    }

    private var refreshIdentity: String {
        guard let agent else { return "missing" }
        return "\(agent.process.bootID):\(agent.process.pid):\(hostAttachment.state)"
    }

    private var statusColor: Color {
        visibleError == nil && isProcessRunning ? .green : .secondary
    }

    private var isProcessRunning: Bool {
        processStatus == "Running"
    }

    private var followOutputBinding: Binding<Bool> {
        Binding(
            get: { presentation.followsOutput },
            set: { followsOutput in
                presentation.followsOutput = followsOutput
                onRendererStateUpdate(streamOffsets, presentation)
            }
        )
    }

    private func pendingRequestView(_ request: AlanAgentPendingRequest) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(request.kind.isEmpty ? "Agent request" : request.kind)
                    .font(ShellType.pro(ShellType.caption, weight: .semibold))
                Spacer()
                Text(request.id)
                    .font(ShellType.mono(ShellType.monoCaption))
                    .foregroundStyle(ShellPalette.mutedInk)
            }
            Text(request.prompt)
                .font(ShellType.pro(ShellType.row))
                .lineLimit(8)
                .textSelection(.enabled)
            if !request.options.isEmpty {
                Text(request.options)
                    .font(ShellType.mono(ShellType.monoCaption))
                    .foregroundStyle(ShellPalette.mutedInk)
                    .lineLimit(8)
                    .textSelection(.enabled)
            }
            HStack(spacing: 8) {
                TextField("Response", text: $requestResponse)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(sendRequestResponse)
                Button("Respond", action: sendRequestResponse)
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .disabled(requestResponse.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(ShellSpacing.control)
        .background(ShellPalette.workspace)
        .overlay(alignment: .top) { Divider().opacity(0.45) }
    }

    @MainActor
    private func tailAgentFiles() async {
        guard let agent else {
            processStatus = "Unavailable"
            visibleError = "This content does not contain an Agent attachment."
            return
        }
        guard let session = hostAttachment.session else {
            processStatus = "Alan OS unavailable"
            if case .unavailable(let detail) = hostAttachment.state { visibleError = detail }
            return
        }
        var hydratedOutput = false
        var hydratedRequest = false
        while !Task.isCancelled {
            do {
                let validated = try await session.validate(agent.process)
                processStatus = validated.status.isEmpty ? "Running" : validated.status.capitalized

                if !hydratedOutput {
                    let history = try await session.readAgentStreamWindow(
                        reference: agent.process,
                        relativePath: "io/output",
                        endingAt: streamOffsets.output
                    )
                    if !history.isEmpty { output = String(decoding: history, as: UTF8.self) }
                    hydratedOutput = true
                }

                var nextOffsets = streamOffsets
                let outputChunk = try await pollStream(
                    session: session,
                    process: agent.process,
                    path: "io/output",
                    offset: nextOffsets.output
                )
                nextOffsets.output = outputChunk.nextOffset
                appendOutput(outputChunk.data)

                let requestChunk = try await pollStream(
                    session: session,
                    process: agent.process,
                    path: "requests/events",
                    offset: nextOffsets.requests
                )
                nextOffsets.requests = requestChunk.nextOffset
                appendActivity("Request", data: requestChunk.data)

                let actionChunk = try await pollStream(
                    session: session,
                    process: agent.process,
                    path: "actions/events",
                    offset: nextOffsets.actions
                )
                nextOffsets.actions = actionChunk.nextOffset
                appendActivity("Action", data: actionChunk.data)

                let uiChunk = try await pollStream(
                    session: session,
                    process: agent.process,
                    path: "machine/ui/events",
                    offset: nextOffsets.ui
                )
                nextOffsets.ui = uiChunk.nextOffset
                appendActivity("UI", data: uiChunk.data)

                if !hydratedRequest || !requestChunk.data.isEmpty || pendingRequest != nil {
                    pendingRequest = try await session.latestPendingRequest(reference: agent.process)
                    hydratedRequest = true
                }

                let advanced = nextOffsets != streamOffsets
                if advanced {
                    streamOffsets = nextOffsets
                    onRendererStateUpdate(streamOffsets, presentation)
                }
                visibleError = nil
                if validated.status == "exited" && !advanced { return }
            } catch let error as AlanOSAttachmentError {
                if case .retentionGap(let stream, let requested, let available) = error {
                    recordRetentionGap(stream: stream, requested: requested, available: available)
                    continue
                }
                processStatus = "Unavailable"
                visibleError = error.localizedDescription
            } catch {
                if Task.isCancelled { return }
                processStatus = "Unavailable"
                visibleError = error.localizedDescription
            }
            try? await Task.sleep(for: .milliseconds(250))
        }
    }

    private func pollStream(
        session: AlanOSAttachmentSession,
        process: AlanOSProcessReference,
        path: String,
        offset: UInt64
    ) async throws -> (data: Data, nextOffset: UInt64) {
        let chunk = try await session.readAgentStream(
            reference: process,
            relativePath: path,
            offset: offset,
            overlap: 256
        )
        var accumulator = AlanAgentStreamAccumulator(nextOffset: offset)
        return (try accumulator.accept(chunk), accumulator.nextOffset)
    }

    private func appendOutput(_ data: Data) {
        guard !data.isEmpty else { return }
        output.append(String(decoding: data, as: UTF8.self))
        if output.count > 262_144 { output = String(output.suffix(262_144)) }
    }

    private func appendActivity(_ label: String, data: Data) {
        guard !data.isEmpty else { return }
        activity.append(contentsOf: String(decoding: data, as: UTF8.self)
            .split(whereSeparator: \.isNewline)
            .map { "\(label): \($0)" })
        if activity.count > 80 { activity.removeFirst(activity.count - 80) }
    }

    private func recordRetentionGap(stream: String, requested: UInt64, available: UInt64) {
        let notice = "Continuity gap in \(stream): saved offset \(requested), available length \(available). Resumed at the visible edge."
        if continuityNotices.last != notice { continuityNotices.append(notice) }
        if continuityNotices.count > 4 {
            continuityNotices.removeFirst(continuityNotices.count - 4)
        }
        switch stream {
        case "io/output": streamOffsets.output = available
        case "requests/events": streamOffsets.requests = available
        case "actions/events": streamOffsets.actions = available
        case "machine/ui/events": streamOffsets.ui = available
        default: return
        }
        onRendererStateUpdate(streamOffsets, presentation)
    }

    private func sendInput() {
        guard let agent, let session = hostAttachment.session else { return }
        let value = input
        input = ""
        Task { @MainActor in
            do {
                try await session.writeAgentInput(reference: agent.process, data: Data(value.utf8))
            } catch {
                visibleError = error.localizedDescription
            }
        }
    }

    private func performInterrupt() {
        guard let agent, let session = hostAttachment.session else { return }
        Task { @MainActor in
            do {
                try await session.interrupt(reference: agent.process)
            } catch {
                visibleError = error.localizedDescription
            }
        }
    }

    private func sendRequestResponse() {
        guard let agent, let request = pendingRequest, let session = hostAttachment.session else { return }
        let value = requestResponse
        requestResponse = ""
        Task { @MainActor in
            do {
                try await session.respond(
                    reference: agent.process,
                    requestID: request.id,
                    data: Data(value.utf8)
                )
                pendingRequest = nil
            } catch {
                visibleError = error.localizedDescription
            }
        }
    }

    private func performMachineControl(_ command: String) {
        guard let agent, let session = hostAttachment.session else { return }
        Task { @MainActor in
            do {
                try await session.controlMachine(reference: agent.process, command: command)
            } catch {
                visibleError = error.localizedDescription
            }
        }
    }

    private func stopProcess() {
        guard let agent, let session = hostAttachment.session else { return }
        Task { @MainActor in
            do {
                try await session.stop(reference: agent.process)
            } catch {
                visibleError = error.localizedDescription
            }
        }
    }

    private func openAnotherView() {
        guard let agent else { return }
        onOpenAnotherView(
            AlanAgentAttachment(
                process: agent.process,
                offsets: streamOffsets,
                presentation: presentation
            )
        )
    }
}

private struct ShellMarkdownContentView: View {
    let descriptor: ShellContentRenderDescriptor
    @State private var renderedContent = AttributedString("")
    @State private var loadError: String?
    @State private var isLoading = false

    var body: some View {
        ZStack {
            ShellPalette.workspace

            ScrollView {
                if isLoading {
                    ProgressView()
                        .controlSize(.small)
                        .frame(maxWidth: .infinity, minHeight: 180)
                        .padding(24)
                } else if let loadError {
                    VStack(spacing: 8) {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.system(size: 20, weight: .medium))
                            .foregroundStyle(ShellPalette.mutedInk)
                        Text(loadError)
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(ShellPalette.ink)
                    }
                    .frame(maxWidth: .infinity, minHeight: 180)
                    .padding(24)
                } else {
                    Text(renderedContent)
                        .font(.system(size: 13))
                        .foregroundStyle(ShellPalette.ink)
                        .lineSpacing(3)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 24)
                        .padding(.vertical, 20)
                }
            }
        }
        .task(id: markdownSource) {
            await loadMarkdown()
        }
    }

    @MainActor
    private func loadMarkdown() async {
        guard let fileURL else {
            renderedContent = AttributedString("")
            loadError = "Unable to open this document."
            isLoading = false
            return
        }

        isLoading = true
        loadError = nil
        renderedContent = AttributedString("")

        let result = await Task.detached(priority: .userInitiated) {
            ShellMarkdownContentLoader.load(fileURL: fileURL)
        }.value
        if Task.isCancelled {
            isLoading = false
            return
        }

        isLoading = false
        switch result {
        case .success(let content):
            renderedContent = content
            loadError = nil
        case .failure:
            renderedContent = AttributedString("")
            loadError = "Unable to read this document."
        }
    }

    private var markdownSource: String {
        descriptor.payload?.markdown?.fileURL ?? ""
    }

    private var fileURL: URL? {
        ShellMarkdownContentLoader.fileURL(from: descriptor.payload?.markdown?.fileURL)
    }
}

private enum ShellMarkdownContentLoader {
    static func fileURL(from rawValue: String?) -> URL? {
        guard let rawValue else { return nil }
        let value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { return nil }

        if let url = URL(string: value),
           url.scheme != nil
        {
            return url.isFileURL ? url.standardizedFileURL : url
        }

        return URL(fileURLWithPath: NSString(string: value).expandingTildeInPath)
            .standardizedFileURL
    }

    static func load(fileURL: URL) -> ShellMarkdownContentLoadResult {
        do {
            let markdown = try String(contentsOf: fileURL, encoding: .utf8)
            let content = (try? AttributedString(markdown: markdown)) ?? AttributedString(markdown)
            return .success(content)
        } catch {
            return .failure
        }
    }
}

private enum ShellMarkdownContentLoadResult {
    case success(AttributedString)
    case failure
}
