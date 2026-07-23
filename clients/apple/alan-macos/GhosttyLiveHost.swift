import Foundation

#if os(macOS) && canImport(GhosttyKit)
import AppKit
import GhosttyKit
import OSLog
import QuartzCore

final class AlanGhosttyLiveHost: NSObject {
    var onDiagnosticsChange: ((TerminalRendererSnapshot) -> Void)?
    var onMetadataChange: ((TerminalPaneMetadataSnapshot) -> Void)?
    var onCloseRequest: ((Bool) -> Void)?
    var onSearchUpdate: ((AlanTerminalSearchEngineUpdate) -> Void)?
    var onScrollbackUpdate: ((AlanTerminalScrollbackMetrics) -> Void)?
    var renderCoordinator: TerminalRenderCoordinator?

    private let logger = Logger(
        subsystem: AlanInstallChannel.current().logSubsystem,
        category: "GhosttyLiveHost"
    )

    private weak var canvasView: AlanGhosttyCanvasView?
    private var config: ghostty_config_t?
    private var app: ghostty_app_t?
    private var surface: ghostty_surface_t?
    private var bootProfile: AlanShellBootProfile?
    private var envStorage: [(UnsafeMutablePointer<CChar>, UnsafeMutablePointer<CChar>)] = []
    private let tickScheduleLock = NSLock()
    private var tickScheduled = false
    private let appFocusObserver = AlanGhosttyAppFocusObserver()
    private var didEmitFirstRefresh = false
    private var diagnostics = TerminalRendererSnapshot.placeholder
    private var metadata = TerminalPaneMetadataSnapshot.placeholder
    private var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground
    private let terminalModeTracker = AlanTerminalModeTracker()
    private var didEmitNonConfirmingCloseRequest = false
    private var foregroundCommandStartedAt: Date?

    func attach(
        to canvasView: AlanGhosttyCanvasView,
        bootProfile: AlanShellBootProfile?,
        ptyAttachmentProvider: () -> AlanTerminalPtyRendererAttachmentResult,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority
    ) {
        let canvasChanged = self.canvasView !== canvasView
        let needsSurfaceRecreation = bootProfile?.requiresSurfaceRecreation(
            comparedTo: self.bootProfile
        ) ?? (self.bootProfile != nil)

        self.canvasView = canvasView
        self.bootProfile = bootProfile
        self.renderPriority = renderPriority

        guard let bootProfile else {
            teardownSurface()
            transition(
                kind: .ghosttyLive,
                phase: .pending,
                summary: "Ghostty host is ready but no pane is selected.",
                detail: "Select a pane to build a terminal boot contract."
            )
            return
        }

        guard ensureApp() else {
            return
        }

        if canvasView.window == nil {
            transition(
                kind: .ghosttyLive,
                phase: app == nil ? .pending : .appReady,
                summary: "Ghostty app is ready and waiting for a window attachment.",
                detail: bootProfile.command.summary
            )
            return
        }

        if surface == nil || needsSurfaceRecreation || canvasChanged {
            let ptyAttachment: AlanTerminalPtyRendererAttachment
            switch ptyAttachmentProvider() {
            case .attached(let attachment):
                ptyAttachment = attachment
            case .rejected(let result):
                transition(
                    kind: .ghosttyLive,
                    phase: .failed,
                    summary: "Ghostty external PTY attachment is unavailable.",
                    detail: result.message,
                    failureReason: result.code
                )
                return
            }
            createSurface(
                on: canvasView,
                bootProfile: bootProfile,
                ptyAttachment: ptyAttachment
            )
        }

        synchronizeViewState(focused: focused, renderPriority: renderPriority)
    }

    func updateRenderPriority(_ priority: TerminalRuntimeRenderPriority) {
        renderPriority = priority
    }

    func requestRenderCatchUp() {
        if let renderCoordinator {
            renderCoordinator.requestCatchUp(from: self)
            return
        }
        scheduleTick()
    }

    func synchronizeViewState(
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority? = nil
    ) {
        guard let canvasView, let surface else { return }
        let priority = renderPriority ?? self.renderPriority
        self.renderPriority = priority
        synchronizeDrawableMetrics(for: canvasView)
        ghostty_surface_set_focus(surface, focused && priority.isForegroundInteractive)
        let visible = priority.isVisible
            && (canvasView.window?.occlusionState.contains(.visible) ?? false)
        ghostty_surface_set_occlusion(surface, visible)
        if visible {
            requestRenderCatchUp()
        }
    }

    var latestMetadata: TerminalPaneMetadataSnapshot {
        metadata
    }

    var isSurfaceReady: Bool {
        surface != nil
    }

    var terminalGridDimensions: AlanTerminalPtyDimensions? {
        guard let surface else { return nil }
        let size = ghostty_surface_size(surface)
        guard size.columns > 0, size.rows > 0 else { return nil }
        return AlanTerminalPtyDimensions(
            columns: Int(size.columns),
            rows: Int(size.rows)
        )
    }

    func keyTranslationMods(for mods: ghostty_input_mods_e) -> ghostty_input_mods_e {
        guard let surface else { return mods }
        return ghostty_surface_key_translation_mods(surface, mods)
    }

    func sendKey(_ keyEvent: ghostty_input_key_s) -> Bool {
        guard let surface else { return false }
        let handled = ghostty_surface_key(surface, keyEvent)
        ghostty_surface_refresh(surface)
        if handled, isCommandSubmissionKey(keyEvent) {
            markForegroundCommandStarted()
        }
        return handled
    }

    func sendControlKey(_ key: TerminalRuntimeControlKey) -> Bool {
        guard surface != nil else { return false }
        let keycode: UInt32
        let mods: ghostty_input_mods_e
        switch key {
        case .interrupt:
            keycode = AlanGhosttyKeyCode.c
            mods = GHOSTTY_MODS_CTRL
        case .endOfTransmission:
            keycode = AlanGhosttyKeyCode.d
            mods = GHOSTTY_MODS_CTRL
        case .returnKey:
            keycode = AlanGhosttyKeyCode.returnKey
            mods = GHOSTTY_MODS_NONE
        }

        var keyDown = ghostty_input_key_s()
        keyDown.action = GHOSTTY_ACTION_PRESS
        keyDown.keycode = keycode
        keyDown.mods = mods
        keyDown.consumed_mods = GHOSTTY_MODS_NONE
        keyDown.text = nil
        keyDown.composing = false
        keyDown.unshifted_codepoint = 0

        var keyUp = keyDown
        keyUp.action = GHOSTTY_ACTION_RELEASE

        let handled = sendKey(keyDown)
        _ = sendKey(keyUp)
        return handled
    }

    func keyIsBinding(_ keyEvent: ghostty_input_key_s, flags: UnsafeMutablePointer<ghostty_binding_flags_e>?) -> Bool {
        guard let surface else { return false }
        return ghostty_surface_key_is_binding(surface, keyEvent, flags)
    }

    func sendProgrammaticText(_ text: String) {
        guard let surface, !text.isEmpty else { return }
        let isCommandSubmission = isCommandSubmissionText(text)
        recordProgrammaticCommandSubmission(in: text)
        text.withCString { cString in
            ghostty_surface_text(surface, cString, UInt(strlen(cString)))
        }
        ghostty_surface_refresh(surface)
        updateMetadata(
            summary: "input committed",
            attention: .active,
            activeTaskState: isCommandSubmission ? .foregroundCommand : nil
        )
    }

    func recordProgrammaticCommandSubmission(in text: String) {
        guard isCommandSubmissionText(text) else { return }
        markForegroundCommandStarted()
    }

    func sendPreedit(_ text: String?) {
        guard let surface else { return }
        guard let text, !text.isEmpty else {
            ghostty_surface_preedit(surface, nil, 0)
            return
        }

        text.withCString { cString in
            ghostty_surface_preedit(surface, cString, UInt(strlen(cString)))
        }
    }

    func sendMousePosition(x: Double, y: Double, mods: ghostty_input_mods_e) {
        guard let surface else { return }
        ghostty_surface_mouse_pos(surface, x, y, mods)
    }

    func sendMouseButton(
        state: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
        mods: ghostty_input_mods_e
    ) -> Bool {
        guard let surface else { return false }
        return ghostty_surface_mouse_button(surface, state, button, mods)
    }

    func sendMouseScroll(x: Double, y: Double, mods: ghostty_input_scroll_mods_t) {
        guard let surface else { return }
        ghostty_surface_mouse_scroll(surface, x, y, mods)
    }

    func sendMousePressure(stage: UInt32, pressure: Double) {
        guard let surface else { return }
        ghostty_surface_mouse_pressure(surface, stage, pressure)
    }

    func readSelectionText() -> String? {
        guard let surface else { return nil }
        var text = ghostty_text_s()
        guard ghostty_surface_read_selection(surface, &text) else { return nil }
        defer { ghostty_surface_free_text(surface, &text) }
        return string(from: text)
    }

    func readText(in range: AlanTerminalBufferRange) -> String? {
        guard let surface, !range.isEmpty else { return nil }
        let surfaceSize = ghostty_surface_size(surface)
        guard surfaceSize.columns > 0 else { return nil }

        let endRow = max(range.lowerBound, range.upperBound - 1)
        let lastColumn = UInt32(surfaceSize.columns - 1)
        let selection = ghostty_selection_s(
            top_left: ghostty_point_s(
                tag: GHOSTTY_POINT_SCREEN,
                coord: GHOSTTY_POINT_COORD_EXACT,
                x: 0,
                y: clampedScreenRow(range.lowerBound)
            ),
            bottom_right: ghostty_point_s(
                tag: GHOSTTY_POINT_SCREEN,
                coord: GHOSTTY_POINT_COORD_EXACT,
                x: lastColumn,
                y: clampedScreenRow(endRow)
            ),
            rectangle: false
        )

        var text = ghostty_text_s()
        guard ghostty_surface_read_text(surface, selection, &text) else { return nil }
        defer { ghostty_surface_free_text(surface, &text) }
        return string(from: text)
    }

    func hasSelection() -> Bool {
        guard let surface else { return false }
        return ghostty_surface_has_selection(surface)
    }

    func performBindingAction(_ action: String) -> Bool {
        guard let surface, !action.isEmpty else { return false }
        let handled = action.withCString { cString in
            ghostty_surface_binding_action(
                surface,
                cString,
                UInt(action.lengthOfBytes(using: .utf8))
            )
        }
        if handled {
            ghostty_surface_refresh(surface)
        }
        return handled
    }

    func imeRect(in view: NSView) -> NSRect? {
        guard let surface else { return nil }

        var x: Double = 0
        var y: Double = 0
        var width: Double = 0
        var height: Double = 0
        ghostty_surface_ime_point(surface, &x, &y, &width, &height)

        let viewRect = NSRect(
            x: x,
            y: view.bounds.height - y,
            width: width,
            height: max(height, 0)
        )

        guard let window = view.window else { return viewRect }
        let windowRect = view.convert(viewRect, to: nil)
        return window.convertToScreen(windowRect)
    }

    func teardown() {
        teardownSurface()
        appFocusObserver.remove()
        if let app {
            ghostty_app_free(app)
            self.app = nil
        }
        if let config {
            ghostty_config_free(config)
            self.config = nil
        }
        transition(
            kind: .scaffold,
            phase: .pending,
            summary: "Ghostty host has been torn down.",
            detail: nil
        )
        resetMetadata()
    }

    private func ensureApp() -> Bool {
        if app != nil {
            return true
        }

        var runtimeConfig = ghostty_runtime_config_s()
        runtimeConfig.userdata = Unmanaged.passUnretained(self).toOpaque()
        runtimeConfig.supports_selection_clipboard = true
        runtimeConfig.wakeup_cb = { userdata in
            AlanGhosttyLiveHost.from(userdata)?.scheduleTick()
        }
        runtimeConfig.action_cb = { app, target, action in
            guard let host = AlanGhosttyLiveHost.from(ghostty_app_userdata(app)) else {
                return false
            }
            return host.handleAction(target: target, action: action)
        }
        runtimeConfig.read_clipboard_cb = { userdata, location, state in
            guard let host = AlanGhosttyLiveHost.from(userdata),
                  let surface = host.surface
            else { return false }

            let text = AlanGhosttyClipboard.readText(location: location) ?? ""
            text.withCString { cString in
                ghostty_surface_complete_clipboard_request(surface, cString, state, false)
            }
            return true
        }
        runtimeConfig.confirm_read_clipboard_cb = { userdata, string, state, _ in
            guard let host = AlanGhosttyLiveHost.from(userdata),
                  let surface = host.surface
            else { return }
            ghostty_surface_complete_clipboard_request(surface, string, state, true)
        }
        runtimeConfig.write_clipboard_cb = { _, location, content, len, _ in
            AlanGhosttyClipboard.write(
                location: location,
                content: content,
                len: len
            )
        }
        runtimeConfig.close_surface_cb = { userdata, processAlive in
            AlanGhosttyLiveHost.from(userdata)?
                .handleSurfaceCloseRequest(processAlive: processAlive)
        }

        guard let primaryConfig = makePrimaryConfig() else {
            transition(
                kind: .ghosttyLive,
                phase: .failed,
                summary: "Failed to allocate a Ghostty config.",
                detail: nil,
                failureReason: "ghostty_config_new returned nil."
            )
            return false
        }

        if let created = ghostty_app_new(&runtimeConfig, primaryConfig) {
            self.app = created
            self.config = primaryConfig
            appFocusObserver.install(for: created)
            ghostty_app_set_focus(created, NSApp.isActive)
            transition(
                kind: .ghosttyLive,
                phase: .appReady,
                summary: "Ghostty app initialized.",
                detail: "Using the user's Ghostty config if present."
            )
            return true
        }

        let primaryDiagnostics = diagnosticMessages(for: primaryConfig)
        logger.error("ghostty_app_new(primary) failed: \(primaryDiagnostics.joined(separator: " | "))")
        ghostty_config_free(primaryConfig)

        guard let fallbackConfig = makeFallbackConfig() else {
            transition(
                kind: .ghosttyLive,
                phase: .failed,
                summary: "Ghostty app initialization failed.",
                detail: primaryDiagnostics.first,
                failureReason: primaryDiagnostics.joined(separator: " | ")
            )
            return false
        }

        guard let created = ghostty_app_new(&runtimeConfig, fallbackConfig) else {
            let fallbackDiagnostics = diagnosticMessages(for: fallbackConfig)
            logger.error("ghostty_app_new(fallback) failed: \(fallbackDiagnostics.joined(separator: " | "))")
            ghostty_config_free(fallbackConfig)
            transition(
                kind: .ghosttyLive,
                phase: .failed,
                summary: "Ghostty app initialization failed for both primary and fallback config.",
                detail: primaryDiagnostics.first ?? fallbackDiagnostics.first,
                failureReason: (primaryDiagnostics + fallbackDiagnostics).joined(separator: " | ")
            )
            return false
        }

        self.app = created
        self.config = fallbackConfig
        appFocusObserver.install(for: created)
        ghostty_app_set_focus(created, NSApp.isActive)
        transition(
            kind: .ghosttyLive,
            phase: .appReady,
            summary: "Ghostty app initialized with a minimal fallback config.",
            detail: primaryDiagnostics.first ?? "User config was skipped after diagnostics."
        )
        return true
    }

    private func createSurface(
        on canvasView: AlanGhosttyCanvasView,
        bootProfile: AlanShellBootProfile,
        ptyAttachment: AlanTerminalPtyRendererAttachment
    ) {
        guard let app else { return }

        teardownSurface()
        didEmitFirstRefresh = false
        didEmitNonConfirmingCloseRequest = false
        terminalModeTracker.reset()

        transition(
            kind: .ghosttyLive,
            phase: .appReady,
            summary: "Ghostty app is creating a surface.",
            detail: bootProfile.command.summary
        )
        updateMetadata(
            title: nil,
            workingDirectory: bootProfile.workingDirectory,
            summary: "booting \(bootProfile.command.summary.lowercased())",
            attention: .active,
            processExited: false,
            lastCommandExitCode: nil,
            activeTaskState: bootProfile.surfaceCommand == nil ? .inactive : .foregroundCommand
        )

        var surfaceConfig = ghostty_surface_config_new()
        surfaceConfig.platform_tag = GHOSTTY_PLATFORM_MACOS
        surfaceConfig.platform = ghostty_platform_u(
            macos: ghostty_platform_macos_s(
                nsview: Unmanaged.passUnretained(canvasView).toOpaque()
            )
        )
        surfaceConfig.userdata = Unmanaged.passUnretained(self).toOpaque()
        surfaceConfig.scale_factor = Double(
            canvasView.window?.backingScaleFactor
                ?? NSScreen.main?.backingScaleFactor
                ?? 2
        )
        surfaceConfig.context = GHOSTTY_SURFACE_CONTEXT_WINDOW
        if let displayID = (canvasView.window?.screen ?? NSScreen.main)?.alanGhosttyDisplayID,
           displayID != 0
        {
            surfaceConfig.initial_macos_display_id = displayID
        }
        surfaceConfig.external_pty_read_fd = ptyAttachment.readFileDescriptor
        surfaceConfig.external_pty_write_fd = ptyAttachment.writeFileDescriptor
        surfaceConfig.external_pty_close_fds = ptyAttachment.closeFileDescriptors

        envStorage = makeEnvStorage(bootProfile.environment)
        var envVars = envStorage.map { ghostty_env_var_s(key: UnsafePointer($0.0), value: UnsafePointer($0.1)) }

        let createSurface = {
            if envVars.isEmpty {
                self.surface = ghostty_surface_new(app, &surfaceConfig)
            } else {
                let envVarsCount = envVars.count
                envVars.withUnsafeMutableBufferPointer { buffer in
                    surfaceConfig.env_vars = buffer.baseAddress
                    surfaceConfig.env_var_count = envVarsCount
                    self.surface = ghostty_surface_new(app, &surfaceConfig)
                }
            }
        }

        bootProfile.workingDirectory.withCString { cwdCString in
            surfaceConfig.working_directory = cwdCString
            surfaceConfig.command = nil
            createSurface()
        }

        guard surface != nil else {
            let diagnostics = diagnosticMessages(for: config)
            let detail = diagnostics.first ?? bootProfile.command.detail
            transition(
                kind: .ghosttyLive,
                phase: .failed,
                summary: "Ghostty surface creation failed.",
                detail: detail,
                failureReason: diagnostics.joined(separator: " | ")
            )
            logger.error("ghostty_surface_new failed: \(diagnostics.joined(separator: " | "))")
            return
        }

        transition(
            kind: .ghosttyLive,
            phase: .surfaceReady,
            summary: "Ghostty surface attached to the macOS canvas.",
            detail: bootProfile.command.launchCommandString
        )
        updateMetadata(
            workingDirectory: bootProfile.workingDirectory,
            summary: "surface ready",
            attention: .active,
            processExited: false,
            lastCommandExitCode: nil
        )
    }

    private func synchronizeDrawableMetrics(for canvasView: AlanGhosttyCanvasView) {
        guard let surface else { return }
        guard let window = canvasView.window else { return }

        let size = canvasView.bounds.size
        guard size.width > 0, size.height > 0 else { return }

        let backingSize = canvasView.convertToBacking(NSRect(origin: .zero, size: size)).size
        guard backingSize.width > 0, backingSize.height > 0 else { return }

        let xScale = backingSize.width / size.width
        let yScale = backingSize.height / size.height
        let layerScale = max(1.0, window.backingScaleFactor)

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        canvasView.layer?.contentsScale = layerScale
        CATransaction.commit()

        ghostty_surface_set_content_scale(surface, xScale, yScale)
        ghostty_surface_set_size(
            surface,
            UInt32(max(1, Int(floor(backingSize.width)))),
            UInt32(max(1, Int(floor(backingSize.height))))
        )

        if let displayID = (window.screen ?? NSScreen.main)?.alanGhosttyDisplayID, displayID != 0 {
            ghostty_surface_set_display_id(surface, displayID)
        }
    }

    private func clampedScreenRow(_ row: Int) -> UInt32 {
        UInt32(min(max(row, 0), Int(UInt32.max)))
    }

    private func string(from text: ghostty_text_s) -> String? {
        guard let raw = text.text else { return nil }
        let length = Int(text.text_len)
        guard length > 0 else { return "" }
        let bytes = UnsafeRawPointer(raw).assumingMemoryBound(to: UInt8.self)
        let buffer = UnsafeBufferPointer(start: bytes, count: length)
        return String(decoding: buffer, as: UTF8.self)
    }

    private func markFirstRefreshIfNeeded(on canvasView: AlanGhosttyCanvasView) {
        guard !didEmitFirstRefresh else { return }
        didEmitFirstRefresh = true
        let size = canvasView.convertToBacking(canvasView.bounds).size
        transition(
            kind: .ghosttyLive,
            phase: .firstRefresh,
            summary: "Ghostty surface issued its first refresh.",
            detail: "\(Int(size.width)) × \(Int(size.height)) backing pixels"
        )
        updateMetadata(summary: "terminal rendering", attention: .active, processExited: false)
    }

    private func scheduleTick() {
        guard markTickScheduledIfNeeded() else { return }

        if let renderCoordinator {
            renderCoordinator.requestWakeup(from: self)
            return
        }

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.clearScheduledTick()
            if let app = self.app {
                ghostty_app_tick(app)
            }
            if let surface = self.surface {
                ghostty_surface_refresh(surface)
            }
        }
    }

    private func markTickScheduledIfNeeded() -> Bool {
        tickScheduleLock.lock()
        defer { tickScheduleLock.unlock() }
        guard !tickScheduled else { return false }
        tickScheduled = true
        return true
    }

    private func clearScheduledTick() {
        tickScheduleLock.lock()
        tickScheduled = false
        tickScheduleLock.unlock()
    }

    private func handleSurfaceCloseRequest(processAlive: Bool) {
        performOnMain {
            self.updateMetadata(
                summary: processAlive ? "surface close requested" : "process exited",
                attention: .awaitingUser,
                processExited: !processAlive,
                activeTaskState: processAlive ? self.metadata.activeTaskState : .inactive
            )
            self.emitCloseRequest(requiresConfirmation: processAlive)

            guard !processAlive else { return }
            self.teardownSurface(preservingMetadata: true)
        }
    }

    private func emitCloseRequest(requiresConfirmation: Bool) {
        if !requiresConfirmation {
            guard !didEmitNonConfirmingCloseRequest else { return }
            didEmitNonConfirmingCloseRequest = true
        }
        onCloseRequest?(requiresConfirmation)
    }

    private func teardownSurface(preservingMetadata: Bool = false) {
        if let surface {
            ghostty_surface_free(surface)
            self.surface = nil
        }

        envStorage.forEach {
            free($0.0)
            free($0.1)
        }
        envStorage.removeAll()
        terminalModeTracker.reset()

        if app != nil {
            transition(
                kind: .ghosttyLive,
                phase: .appReady,
                summary: "Ghostty app is idle and waiting for a new surface.",
                detail: bootProfile?.command.summary
            )
        }

        guard !preservingMetadata else { return }
        updateMetadata(
            summary: app == nil ? nil : "surface released",
            attention: .idle,
            processExited: false,
            lastCommandExitCode: metadata.lastCommandExitCode
        )
    }

    private func makeEnvStorage(_ environment: [String: String]) -> [(UnsafeMutablePointer<CChar>, UnsafeMutablePointer<CChar>)] {
        environment
            .compactMap { key, value in
                guard let keyPtr = strdup(key), let valuePtr = strdup(value) else {
                    return nil
                }
                return (keyPtr, valuePtr)
            }
    }

    private func makePrimaryConfig() -> ghostty_config_t? {
        guard let config = ghostty_config_new() else {
            return nil
        }
        ghostty_config_load_default_files(config)
        ghostty_config_load_recursive_files(config)
        ghostty_config_finalize(config)
        return config
    }

    private func makeFallbackConfig() -> ghostty_config_t? {
        guard let config = ghostty_config_new() else {
            return nil
        }
        ghostty_config_finalize(config)
        return config
    }

    private func diagnosticMessages(for config: ghostty_config_t?) -> [String] {
        guard let config else { return [] }
        let count = Int(ghostty_config_diagnostics_count(config))
        guard count > 0 else { return [] }
        return (0..<count).compactMap { index in
            let diagnostic = ghostty_config_get_diagnostic(config, UInt32(index))
            guard let message = diagnostic.message else { return nil }
            return String(cString: message)
        }
    }

    private func transition(
        kind: TerminalRendererKind,
        phase: TerminalRendererPhase,
        summary: String,
        detail: String?,
        failureReason: String? = nil
    ) {
        let event = detail.map { "\(summary) \($0)" } ?? summary
        var recentEvents = diagnostics.recentEvents
        if recentEvents.last != event {
            recentEvents.append(event)
            recentEvents = Array(recentEvents.suffix(6))
        }

        let snapshot = TerminalRendererSnapshot(
            kind: kind,
            phase: phase,
            summary: summary,
            detail: detail,
            failureReason: failureReason,
            recentEvents: recentEvents
        )

        guard diagnostics != snapshot else { return }
        diagnostics = snapshot
        onDiagnosticsChange?(snapshot)

        if let failureReason, phase == .failed {
            logger.error("\(summary) \(failureReason)")
        } else {
            logger.info("\(summary)")
        }
    }

    private func handleAction(target: ghostty_target_s, action: ghostty_action_s) -> Bool {
        if target.tag == GHOSTTY_TARGET_SURFACE,
           let surface,
           target.target.surface != surface {
            return false
        }

        switch action.tag {
        case GHOSTTY_ACTION_SET_TITLE:
            let title = action.action.set_title.title.flatMap { String(cString: $0) }
            performOnMain {
                self.updateMetadata(
                    title: title,
                    summary: title.flatMap { !$0.isEmpty ? "title updated" : nil }
                )
            }
            return true

        case GHOSTTY_ACTION_PWD:
            let workingDirectory = action.action.pwd.pwd.flatMap { String(cString: $0) }
            performOnMain {
                self.updateMetadata(
                    workingDirectory: workingDirectory,
                    summary: workingDirectory.flatMap { !$0.isEmpty ? "working directory updated" : nil }
                )
            }
            return true

        case GHOSTTY_ACTION_RING_BELL:
            performOnMain {
                self.updateMetadata(
                    summary: "terminal bell",
                    attention: .notable,
                    activity: TerminalActivitySnapshot.bellActivity(now: .now)
                )
            }
            return true

        case GHOSTTY_ACTION_SHOW_CHILD_EXITED:
            let exitCode = action.action.child_exited.exit_code
            performOnMain {
                self.updateMetadata(
                    summary: "process exited with status \(exitCode)",
                    attention: .awaitingUser,
                    processExited: true,
                    lastCommandExitCode: Int(exitCode),
                    activeTaskState: .inactive,
                    activity: TerminalActivitySnapshot.processExitedActivity(
                        exitCode: Int(exitCode),
                        now: .now
                    )
                )
            }
            return true

        case GHOSTTY_ACTION_COMMAND_FINISHED:
            let exitCode = action.action.command_finished.exit_code
            let finishedAt = Date()
            let summary: String
            if exitCode < 0 {
                summary = "command finished"
            } else if exitCode == 0 {
                summary = "command succeeded"
            } else {
                summary = "command failed (\(exitCode))"
            }
            performOnMain {
                let durationMilliseconds = self.commandDurationMilliseconds(finishedAt: finishedAt)
                self.foregroundCommandStartedAt = nil
                let activity = exitCode >= 0
                    ? TerminalActivitySnapshot.commandCompletion(
                        exitCode: Int(exitCode),
                        now: finishedAt,
                        durationMilliseconds: durationMilliseconds
                    )
                    : nil
                self.updateMetadata(
                    summary: summary,
                    attention: exitCode == 0 ? .active : .notable,
                    processExited: false,
                    lastCommandExitCode: Int(exitCode),
                    activeTaskState: .inactive,
                    activity: activity,
                    clearActivity: exitCode < 0
                )
            }
            return true

        case GHOSTTY_ACTION_PROGRESS_REPORT:
            let progress = action.action.progress_report
            let summary = progressSummary(progress)
            let activity = progressActivity(progress)
            performOnMain {
                self.updateMetadata(
                    summary: summary,
                    attention: .active,
                    activity: activity,
                    clearActivity: progress.state == GHOSTTY_PROGRESS_STATE_REMOVE
                )
            }
            return true

        case GHOSTTY_ACTION_SCROLLBAR:
            let scrollbar = action.action.scrollbar
            let totalRows = clampedInt(scrollbar.total)
            let visibleRows = clampedInt(scrollbar.len)
            let mode = terminalModeTracker.resolveMode(
                totalRows: totalRows,
                visibleRows: visibleRows,
                mouseCaptured: surface.map { ghostty_surface_mouse_captured($0) } ?? false
            )
            let metrics = AlanTerminalScrollbackMetrics(
                totalRows: totalRows,
                visibleRows: visibleRows,
                firstVisibleRow: clampedInt(scrollbar.offset),
                mode: mode
            )
            performOnMain {
                self.onScrollbackUpdate?(metrics)
            }
            return true

        case GHOSTTY_ACTION_START_SEARCH:
            let query = action.action.start_search.needle.flatMap { String(cString: $0) } ?? ""
            performOnMain {
                self.onSearchUpdate?(.started(query: query))
            }
            return true

        case GHOSTTY_ACTION_END_SEARCH:
            performOnMain {
                self.onSearchUpdate?(.ended)
            }
            return true

        case GHOSTTY_ACTION_SEARCH_TOTAL:
            let rawTotal = action.action.search_total.total
            let total = rawTotal >= 0 ? Int(rawTotal) : nil
            performOnMain {
                self.onSearchUpdate?(.matches(total: total))
            }
            return true

        case GHOSTTY_ACTION_SEARCH_SELECTED:
            let rawSelected = action.action.search_selected.selected
            let selected = rawSelected >= 0 ? Int(rawSelected) : nil
            performOnMain {
                self.onSearchUpdate?(.selected(index: selected))
            }
            return true

        default:
            return false
        }
    }

    private func progressSummary(_ progress: ghostty_action_progress_report_s) -> String? {
        switch progress.state {
        case GHOSTTY_PROGRESS_STATE_REMOVE:
            return "progress cleared"
        case GHOSTTY_PROGRESS_STATE_SET:
            return progress.progress >= 0 ? "progress \(progress.progress)%" : "progress updated"
        case GHOSTTY_PROGRESS_STATE_ERROR:
            return "progress error"
        case GHOSTTY_PROGRESS_STATE_INDETERMINATE:
            return "progress running"
        case GHOSTTY_PROGRESS_STATE_PAUSE:
            return "progress paused"
        default:
            return nil
        }
    }

    private func progressActivity(
        _ progress: ghostty_action_progress_report_s,
        now: Date = .now
    ) -> TerminalActivitySnapshot? {
        switch progress.state {
        case GHOSTTY_PROGRESS_STATE_REMOVE:
            return nil
        case GHOSTTY_PROGRESS_STATE_SET:
            if progress.progress >= 0 {
                return TerminalActivitySnapshot.progressActivity(
                    percent: Int(progress.progress),
                    now: now
                )
            }
            return TerminalActivitySnapshot.progressActivity(
                progress: .indeterminate,
                status: .progress,
                priority: .active,
                stateLabel: "Running",
                now: now
            )
        case GHOSTTY_PROGRESS_STATE_ERROR:
            return TerminalActivitySnapshot.progressActivity(
                progress: .failed,
                status: .failed,
                priority: .notable,
                stateLabel: "Failed",
                now: now
            )
        case GHOSTTY_PROGRESS_STATE_INDETERMINATE:
            return TerminalActivitySnapshot.progressActivity(
                progress: .indeterminate,
                status: .progress,
                priority: .active,
                stateLabel: "Running",
                now: now
            )
        case GHOSTTY_PROGRESS_STATE_PAUSE:
            return TerminalActivitySnapshot.progressActivity(
                progress: .paused,
                status: .paused,
                priority: .active,
                stateLabel: "Paused",
                now: now
            )
        default:
            return nil
        }
    }

    private func clampedInt(_ value: UInt64) -> Int {
        value > UInt64(Int.max) ? Int.max : Int(value)
    }

    private func updateMetadata(
        title: String? = nil,
        workingDirectory: String? = nil,
        summary: String? = nil,
        attention: ShellAttentionState? = nil,
        processExited: Bool? = nil,
        lastCommandExitCode: Int? = nil,
        activeTaskState: ShellTabActiveTaskState? = nil,
        activity: TerminalActivitySnapshot? = nil,
        clearActivity: Bool = false
    ) {
        let nextTitle = title ?? metadata.title
        let nextWorkingDirectory = workingDirectory ?? metadata.workingDirectory
        let nextSummary = summary ?? metadata.summary
        let nextAttention = attention ?? metadata.attention
        let nextProcessExited = processExited ?? metadata.processExited
        let nextLastCommandExitCode = lastCommandExitCode ?? metadata.lastCommandExitCode
        let nextActiveTaskState = activeTaskState ?? metadata.activeTaskState
        let nextActivity = clearActivity ? nil : (activity ?? metadata.activity)

        guard
            nextTitle != metadata.title
                || nextWorkingDirectory != metadata.workingDirectory
                || nextSummary != metadata.summary
                || nextAttention != metadata.attention
                || nextProcessExited != metadata.processExited
                || nextLastCommandExitCode != metadata.lastCommandExitCode
                || nextActiveTaskState != metadata.activeTaskState
                || nextActivity != metadata.activity
                || clearActivity
        else {
            return
        }

        let snapshot = TerminalPaneMetadataSnapshot(
            title: nextTitle,
            workingDirectory: nextWorkingDirectory,
            summary: nextSummary,
            attention: nextAttention,
            processExited: nextProcessExited,
            lastCommandExitCode: nextLastCommandExitCode,
            lastUpdatedAt: .now,
            activeTaskState: nextActiveTaskState,
            activity: nextActivity,
            clearsActivity: clearActivity
        )
        metadata = snapshot
        onMetadataChange?(snapshot)
    }

    private func isCommandSubmissionKey(_ keyEvent: ghostty_input_key_s) -> Bool {
        keyEvent.action == GHOSTTY_ACTION_PRESS
            && (
                keyEvent.keycode == AlanGhosttyKeyCode.returnKey
                    || keyEvent.keycode == AlanGhosttyKeyCode.keypadEnter
            )
    }

    private func isCommandSubmissionText(_ text: String) -> Bool {
        text.contains("\n") || text.contains("\r")
    }

    private func markForegroundCommandStarted() {
        if foregroundCommandStartedAt == nil {
            foregroundCommandStartedAt = .now
        }
        updateMetadata(attention: .active, activeTaskState: .foregroundCommand)
    }

    private func commandDurationMilliseconds(finishedAt: Date) -> Int? {
        guard let foregroundCommandStartedAt else { return nil }
        let duration = finishedAt.timeIntervalSince(foregroundCommandStartedAt)
        guard duration >= 0 else { return nil }
        return Int((duration * 1_000).rounded())
    }

    private func resetMetadata() {
        foregroundCommandStartedAt = nil
        guard metadata != .placeholder else { return }
        metadata = .placeholder
        onMetadataChange?(.placeholder)
    }

    private func performOnMain(_ body: @escaping () -> Void) {
        if Thread.isMainThread {
            body()
        } else {
            DispatchQueue.main.async(execute: body)
        }
    }

    private static func from(_ userdata: UnsafeMutableRawPointer?) -> AlanGhosttyLiveHost? {
        guard let userdata else { return nil }
        return Unmanaged<AlanGhosttyLiveHost>.fromOpaque(userdata).takeUnretainedValue()
    }
}

extension AlanGhosttyLiveHost: TerminalRenderCoordinatedHost {
    var terminalRenderPriority: TerminalRuntimeRenderPriority {
        renderPriority
    }

    var isRenderCoordinatorTargetAlive: Bool {
        app != nil || surface != nil
    }

    func renderCoordinatorDrainAppTick() {
        clearScheduledTick()
        if let app {
            ghostty_app_tick(app)
        }
    }

    func renderCoordinatorRefreshSurface(reason: TerminalRenderRefreshReason) {
        guard let surface else { return }
        ghostty_surface_refresh(surface)
        if let canvasView {
            markFirstRefreshIfNeeded(on: canvasView)
        }
    }
}

#endif
