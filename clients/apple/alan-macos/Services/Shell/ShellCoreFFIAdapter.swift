import Darwin
import Foundation

struct ShellCoreFFIAdapter {
    static var shared: ShellCoreFFIAdapter {
        get throws {
            try sharedStorage.adapter()
        }
    }

    private let libraryHandle: UnsafeMutableRawPointer
    private let abiVersionFunction: ShellCoreABIVersionFunction
    private let handleRequestFunction: ShellCoreHandleRequestOutFunction
    private let freeBytesFunction: ShellCoreFreeBytesFunction
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder

    init(libraryPath: String? = ProcessInfo.processInfo.environment["ALAN_SHELL_CORE_FFI_LIBRARY"]) throws {
        let resolvedPath = libraryPath ?? Self.bundledLibraryPath()
        guard let libraryHandle = dlopen(resolvedPath, RTLD_NOW | RTLD_LOCAL) else {
            throw ShellCoreFFIAdapterError.libraryLoadFailed(resolvedPath, Self.dlerrorMessage())
        }
        self.libraryHandle = libraryHandle

        abiVersionFunction = try Self.loadSymbol(
            "alan_shell_core_ffi_abi_version",
            from: libraryHandle,
            as: ShellCoreABIVersionFunction.self
        )
        handleRequestFunction = try Self.loadSymbol(
            "alan_shell_core_ffi_handle_request_out",
            from: libraryHandle,
            as: ShellCoreHandleRequestOutFunction.self
        )
        freeBytesFunction = try Self.loadSymbol(
            "alan_shell_core_ffi_free_bytes",
            from: libraryHandle,
            as: ShellCoreFreeBytesFunction.self
        )

        let abiVersion = abiVersionFunction()
        guard abiVersion == Self.expectedABIVersion else {
            throw ShellCoreFFIAdapterError.abiVersionMismatch(
                expected: Self.expectedABIVersion,
                actual: abiVersion
            )
        }

        encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
    }

    func defaultContentWorkspaceManifest(
        windowID: String,
        defaultWorkingDirectory: String,
        now: Date
    ) throws -> ShellContentWorkspaceManifest {
        let payload = DefaultManifestPayload(
            windowID: windowID,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: Self.iso8601Formatter.string(from: now)
        )
        let response: ManifestPayload = try send(
            operation: "manifest.default_manifest",
            payload: payload
        )
        return response.manifest
    }

    func migrateLegacyTerminalManifest(
        _ manifest: ShellWorkspaceManifest
    ) throws -> ShellContentWorkspaceManifest {
        let response: ManifestPayload = try send(
            operation: "manifest.migrate_legacy_terminal_manifest",
            payload: LegacyManifestPayload(manifest: manifest)
        )
        return response.manifest
    }

    func pruningExpiredTabs(
        manifest: ShellContentWorkspaceManifest,
        now: Date,
        ttl: TimeInterval
    ) throws -> ShellContentWorkspaceManifest {
        let response: ManifestPayload = try send(
            operation: "manifest.pruning_expired_tabs",
            payload: PruningExpiredTabsPayload(
                manifest: manifest,
                now: Self.iso8601Formatter.string(from: now),
                ttlSeconds: Int64(max(0, ttl.rounded(.down)))
            )
        )
        return response.manifest
    }

    func materializeContentWorkspaceManifest(
        manifest: ShellContentWorkspaceManifest,
        defaultWorkingDirectory: String,
        now: Date
    ) throws -> ShellStateSnapshot {
        let response: MaterializedWorkspaceStatePayload = try send(
            operation: "manifest.materialize",
            payload: MaterializeManifestPayload(
                manifest: manifest,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: Self.iso8601Formatter.string(from: now)
            )
        )
        return try response.state.materializedShellState()
    }

    func applyReducer(
        state: ShellStateSnapshot,
        operation: ShellCoreReducerOperation
    ) throws -> ShellStateMutationResult {
        let response: ShellCoreReducerApplyResponse = try send(
            operation: "reducer.apply",
            payload: ShellCoreReducerApplyPayload(
                state: ShellCorePortableWorkspaceState(projecting: state),
                operation: operation
            )
        )

        guard response.status == "ok",
              let result = response.result
        else {
            let code = response.errorCode ?? "unknown_reducer_error"
            if let mutationError = ShellStateMutationError(rawValue: code) {
                throw mutationError
            }
            throw ShellCoreFFIAdapterError.reducerError(
                code: code,
                message: response.errorMessage ?? "shell-core reducer returned an error"
            )
        }

        return ShellStateMutationResult(
            state: try result.state.materializedShellState()
                .preservingPlatformPaneFields(from: state),
            spaceID: result.focus.spaceID,
            tabID: result.focus.tabID,
            paneID: result.focus.paneSlotID
        )
    }

    func actionTitle(for id: ShellActionID) throws -> String? {
        let response: ShellCoreStandardActionDescriptorsResponse = try send(
            operation: "actions.standard_descriptors",
            payload: ShellCoreEmptyPayload()
        )
        return response.actions.first { $0.id == id }?.title
    }

    func defaultActionShortcut(
        for id: ShellActionID,
        target: ShellActionTarget = .currentSelection
    ) throws -> ShellActionShortcut? {
        let response: ShellCoreDefaultActionShortcutResponse = try send(
            operation: "actions.default_shortcut",
            payload: ShellCoreActionDefaultShortcutPayload(
                id: id,
                target: ShellCoreActionTarget(target)
            )
        )
        return response.shortcut?.shellShortcut
    }

    func keyboardAction(for shortcut: ShellActionShortcut) throws -> ShellKeyboardAction? {
        let response: ShellCoreKeyboardActionResponse = try send(
            operation: "actions.keyboard_action",
            payload: ShellCoreKeyboardActionPayload(shortcut: ShellCoreActionShortcut(shortcut))
        )
        return response.keyboardAction?.shellKeyboardAction
    }

    func actionAvailability(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) throws -> ShellActionAvailability {
        let result = try coreActionExecutionResult(id, target: target, state: state)
        switch result.status {
        case .executed:
            return .available
        case .failed, .unavailable:
            return .unavailable(reason: result.reason ?? "Action is unavailable")
        }
    }

    func executeAction(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot,
        handler: (ShellActionEffect) -> Bool
    ) throws -> ShellActionExecutionResult {
        let result = try coreActionExecutionResult(id, target: target, state: state)
        switch result.status {
        case .executed:
            guard let effect = result.effect?.shellActionEffect else {
                return .failed(reason: "Action effect is unavailable")
            }
            return handler(effect) ? .executed : .failed(reason: "Action handler failed")
        case .failed:
            return .failed(reason: result.reason ?? "Action failed")
        case .unavailable:
            return .unavailable(reason: result.reason ?? "Action is unavailable")
        }
    }

    private func coreActionExecutionResult(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) throws -> ShellCoreActionExecutionResult {
        let response: ShellCoreActionExecuteResponse = try send(
            operation: "actions.execute",
            payload: ShellCoreActionExecutePayload(
                state: ShellCorePortableWorkspaceState(projecting: state),
                id: id,
                target: ShellCoreActionTarget(target)
            )
        )
        return response.result
    }

    func terminalProfileRows(
        _ summary: TerminalProfileSettingsSummary
    ) throws -> [ShellSettingsRowModel] {
        let response: ShellCoreSettingsRowsResponse = try send(
            operation: "settings.terminal_profile_rows",
            payload: ShellCoreTerminalProfileSettingsSummaryPayload(summary)
        )
        return response.rows.map(\.settingsRow)
    }

    func capabilityRows(
        _ summary: ShellSettingsCapabilitiesSummary
    ) throws -> [ShellSettingsRowModel] {
        let response: ShellCoreSettingsRowsResponse = try send(
            operation: "settings.capability_rows",
            payload: ShellCoreCapabilitiesSettingsSummaryPayload(summary)
        )
        return response.rows.map(\.settingsRow)
    }

    func localRows(
        _ local: ShellSettingsLocalSummary,
        diagnostics: ShellSettingsDiagnosticsSummary
    ) throws -> [ShellSettingsRowModel] {
        let response: ShellCoreSettingsRowsResponse = try send(
            operation: "settings.local_rows",
            payload: ShellCoreLocalRowsPayload(local: local, diagnostics: diagnostics)
        )
        return response.rows.map(\.settingsRow)
    }

    func validateTerminalProfileDocument(
        _ document: TerminalProfileDocument
    ) throws -> TerminalProfileValidationResult {
        let response: ShellCoreTerminalProfileValidationResponse = try send(
            operation: "terminal_profile.validate",
            payload: document
        )
        return response.validationResult
    }

    func makeTerminalProfileDefinition(
        from draft: TerminalProfileEditorDraft
    ) throws -> TerminalProfileEditorResult {
        let response: ShellCoreTerminalProfileEditorResponse = try send(
            operation: "terminal_profile.make_definition",
            payload: ShellCoreTerminalProfileEditorDraft(draft)
        )
        return response.editorResult
    }

    func resolveTerminalLaunchIntent(
        terminalProfileReference: String?,
        terminalProfiles: TerminalProfileDocument?,
        executablePaths: Set<String>,
        environment: [String: String]
    ) throws -> ShellCoreTerminalLaunchIntent {
        let response: ShellCoreTerminalLaunchIntentResponse = try send(
            operation: "terminal_profile.resolve_launch_intent",
            payload: ShellCoreTerminalLaunchIntentPayload(
                terminalProfileReference: terminalProfileReference,
                terminalProfiles: terminalProfiles,
                executablePaths: executablePaths,
                environment: environment
            )
        )
        return response.intent
    }

    func handleControlCommand(
        _ command: AlanShellControlCommand,
        state: ShellStateSnapshot
    ) throws -> ShellCoreControlCommandResult {
        let response: ShellCoreControlHandleResponse = try send(
            operation: "control.handle",
            payload: ShellCoreControlHandlePayload(
                state: ShellCorePortableWorkspaceState(projecting: state),
                command: command
            )
        )
        return try response.result.shellCommandResult(fallbackState: state)
    }

    private func send<Input: Encodable, Output: Decodable>(
        operation: String,
        payload: Input
    ) throws -> Output {
        let payloadObject = try Self.jsonObject(from: payload, encoder: encoder)
        let request: [String: Any] = [
            "schema_version": ["major": 1, "minor": 0],
            "id": UUID().uuidString.lowercased(),
            "operation": operation,
            "payload": payloadObject,
        ]
        let requestData = try JSONSerialization.data(withJSONObject: request)
        let responseData = try requestData.withUnsafeBytes { requestBytes -> Data in
            let baseAddress = requestBytes.bindMemory(to: UInt8.self).baseAddress
            var responsePointer: UnsafeMutablePointer<UInt8>?
            var responseLength = 0
            let handled = handleRequestFunction(
                baseAddress,
                requestData.count,
                &responsePointer,
                &responseLength
            )
            guard handled != 0 else {
                throw ShellCoreFFIAdapterError.requestFailed
            }
            defer { freeBytesFunction(responsePointer, responseLength) }
            guard let pointer = responsePointer else {
                throw ShellCoreFFIAdapterError.nullResponseBuffer
            }
            return Data(bytes: pointer, count: responseLength)
        }

        let response = try decoder.decode(ShellCoreResponseEnvelope.self, from: responseData)
        if let error = response.error {
            throw ShellCoreFFIAdapterError.facadeError(error)
        }
        guard let payload = response.payload else {
            throw ShellCoreFFIAdapterError.missingPayload(operation)
        }
        return try decoder.decode(Output.self, from: payload)
    }

    private static func jsonObject<T: Encodable>(
        from value: T,
        encoder: JSONEncoder
    ) throws -> Any {
        let data = try encoder.encode(value)
        return try JSONSerialization.jsonObject(with: data)
    }

    private static func loadSymbol<T>(
        _ name: String,
        from handle: UnsafeMutableRawPointer,
        as type: T.Type
    ) throws -> T {
        guard let symbol = dlsym(handle, name) else {
            throw ShellCoreFFIAdapterError.symbolMissing(name, dlerrorMessage())
        }
        return unsafeBitCast(symbol, to: type)
    }

    private static func bundledLibraryPath() -> String {
        Bundle.main.privateFrameworksURL?
            .appendingPathComponent("libalan_shell_core_ffi.dylib")
            .path
            ?? "libalan_shell_core_ffi.dylib"
    }

    private static func dlerrorMessage() -> String {
        dlerror().map { String(cString: $0) } ?? "unknown dynamic linker error"
    }

    private static let expectedABIVersion: UInt32 = 1
    private static let iso8601Formatter = ISO8601DateFormatter()
    private static let sharedStorage = ShellCoreFFIAdapterSharedStorage()
}

private final class ShellCoreFFIAdapterSharedStorage: @unchecked Sendable {
    private let lock = NSLock()
    private var cachedAdapter: ShellCoreFFIAdapter?

    func adapter() throws -> ShellCoreFFIAdapter {
        lock.lock()
        defer { lock.unlock() }
        if let cachedAdapter {
            return cachedAdapter
        }
        let adapter = try ShellCoreFFIAdapter()
        cachedAdapter = adapter
        return adapter
    }
}

enum ShellCoreReducerOperation: Encodable {
    case focusPane(paneSlotID: String)
    case focusAdjacentPane(direction: ShellSpatialFocusDirection)
    case selectSpace(spaceID: String)
    case selectTab(tabID: String)
    case setTerminalProfile(spaceID: String, terminalProfileID: String?)
    case setPresentationIcon(spaceID: String, presentationIcon: String?)
    case deleteSpace(spaceID: String, defaultWorkingDirectory: String?)
    case createTerminalSpace(
        title: String?,
        tabTitle: String?,
        workingDirectory: String?,
        terminalProfileID: String?,
        presentationIcon: String?,
        reservedPaneSlotIDs: [String]
    )
    case openTerminalTab(
        spaceID: String?,
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String?,
        reservedPaneSlotIDs: [String]
    )
    case openContentTab(
        spaceID: String?,
        kind: ShellContentKind,
        title: String,
        payload: ShellContentPayload,
        reservedPaneSlotIDs: [String]
    )
    case duplicateTab(tabID: String, reservedPaneSlotIDs: [String])
    case moveTab(tabID: String, sectionOffset: Int)
    case moveTabToSpace(tabID: String, targetSpaceID: String)
    case organizeTab(
        tabID: String,
        targetSpaceID: String?,
        section: ShellTabOrganizationSection,
        index: Int?
    )
    case showQuickTerminal(workingDirectory: String?, defaultWorkingDirectory: String?)
    case hideQuickTerminal
    case closeQuickTerminal
    case promoteQuickTerminal(targetSpaceID: String)
    case pinTab(tabID: String)
    case unpinTab(tabID: String)
    case renameTab(tabID: String, title: String)
    case closeTab(tabID: String)
    case closePane(paneSlotID: String)
    case clearInactiveTemporaryTabs(spaceID: String, protectedTabIDs: [String])
    case splitPane(
        paneSlotID: String,
        placement: ShellPaneSplitDirection,
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String?,
        reservedPaneSlotIDs: [String]
    )
    case splitContentPane(
        paneSlotID: String,
        placement: ShellPaneSplitDirection,
        kind: ShellContentKind,
        title: String,
        payload: ShellContentPayload,
        reservedPaneSlotIDs: [String]
    )
    case resizeSplit(splitNodeID: String, ratio: Double)
    case equalizeSplits(tabID: String?)
    case movePaneWithinTab(paneSlotID: String, placement: ShellPaneSplitDirection)
    case movePaneToNewTab(paneSlotID: String, title: String?)
    case movePaneToTab(paneSlotID: String, targetTabID: String, direction: ShellSplitDirection)
    case setAttention(paneSlotID: String, attention: ShellAttentionState)

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case direction
        case spaceID = "space_id"
        case tabID = "tab_id"
        case targetTabID = "target_tab_id"
        case sectionOffset = "section_offset"
        case targetSpaceID = "target_space_id"
        case section
        case index
        case title
        case tabTitle = "tab_title"
        case workingDirectory = "working_directory"
        case defaultWorkingDirectory = "default_working_directory"
        case terminalProfileID = "terminal_profile_id"
        case presentationIcon = "presentation_icon"
        case reservedPaneSlotIDs = "reserved_pane_slot_ids"
        case protectedTabIDs = "protected_tab_ids"
        case attention
        case splitNodeID = "split_node_id"
        case ratio
        case placement
        case kind
        case payload
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .focusPane(let paneSlotID):
            try container.encode("focus_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
        case .focusAdjacentPane(let direction):
            try container.encode("focus_adjacent_pane", forKey: .type)
            try container.encode(direction, forKey: .direction)
        case .selectSpace(let spaceID):
            try container.encode("select_space", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
        case .selectTab(let tabID):
            try container.encode("select_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .setTerminalProfile(let spaceID, let terminalProfileID):
            try container.encode("set_terminal_profile", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
        case .setPresentationIcon(let spaceID, let presentationIcon):
            try container.encode("set_presentation_icon", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(presentationIcon, forKey: .presentationIcon)
        case .deleteSpace(let spaceID, let defaultWorkingDirectory):
            try container.encode("delete_space", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(
                defaultWorkingDirectory,
                forKey: .defaultWorkingDirectory
            )
        case .createTerminalSpace(
            let title,
            let tabTitle,
            let workingDirectory,
            let terminalProfileID,
            let presentationIcon,
            let reservedPaneSlotIDs
        ):
            try container.encode("create_terminal_space", forKey: .type)
            try container.encodeIfPresent(title, forKey: .title)
            try container.encodeIfPresent(tabTitle, forKey: .tabTitle)
            try container.encodeIfPresent(workingDirectory, forKey: .workingDirectory)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
            try container.encodeIfPresent(presentationIcon, forKey: .presentationIcon)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .openTerminalTab(
            let spaceID,
            let title,
            let workingDirectory,
            let terminalProfileID,
            let reservedPaneSlotIDs
        ):
            try container.encode("open_terminal_tab", forKey: .type)
            try container.encodeIfPresent(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(title, forKey: .title)
            try container.encodeIfPresent(workingDirectory, forKey: .workingDirectory)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .openContentTab(
            let spaceID,
            let kind,
            let title,
            let payload,
            let reservedPaneSlotIDs
        ):
            try container.encode("open_content_tab", forKey: .type)
            try container.encodeIfPresent(spaceID, forKey: .spaceID)
            try container.encode(kind, forKey: .kind)
            try container.encode(title, forKey: .title)
            try container.encode(payload, forKey: .payload)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .duplicateTab(let tabID, let reservedPaneSlotIDs):
            try container.encode("duplicate_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .moveTab(let tabID, let sectionOffset):
            try container.encode("move_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(sectionOffset, forKey: .sectionOffset)
        case .moveTabToSpace(let tabID, let targetSpaceID):
            try container.encode("move_tab_to_space", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(targetSpaceID, forKey: .targetSpaceID)
        case .organizeTab(let tabID, let targetSpaceID, let section, let index):
            try container.encode("organize_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encodeIfPresent(targetSpaceID, forKey: .targetSpaceID)
            try container.encode(section, forKey: .section)
            try container.encodeIfPresent(index, forKey: .index)
        case .showQuickTerminal(let workingDirectory, let defaultWorkingDirectory):
            try container.encode("show_quick_terminal", forKey: .type)
            try container.encodeIfPresent(workingDirectory, forKey: .workingDirectory)
            try container.encodeIfPresent(defaultWorkingDirectory, forKey: .defaultWorkingDirectory)
        case .hideQuickTerminal:
            try container.encode("hide_quick_terminal", forKey: .type)
        case .closeQuickTerminal:
            try container.encode("close_quick_terminal", forKey: .type)
        case .promoteQuickTerminal(let targetSpaceID):
            try container.encode("promote_quick_terminal", forKey: .type)
            try container.encode(targetSpaceID, forKey: .targetSpaceID)
        case .pinTab(let tabID):
            try container.encode("pin_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .unpinTab(let tabID):
            try container.encode("unpin_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .renameTab(let tabID, let title):
            try container.encode("rename_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(title, forKey: .title)
        case .closeTab(let tabID):
            try container.encode("close_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .closePane(let paneSlotID):
            try container.encode("close_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
        case .clearInactiveTemporaryTabs(let spaceID, let protectedTabIDs):
            try container.encode("clear_inactive_temporary_tabs", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encode(protectedTabIDs, forKey: .protectedTabIDs)
        case .splitPane(
            let paneSlotID,
            let placement,
            let title,
            let workingDirectory,
            let terminalProfileID,
            let reservedPaneSlotIDs
        ):
            try container.encode("split_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(placement, forKey: .placement)
            try container.encodeIfPresent(title, forKey: .title)
            try container.encodeIfPresent(workingDirectory, forKey: .workingDirectory)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .splitContentPane(
            let paneSlotID,
            let placement,
            let kind,
            let title,
            let payload,
            let reservedPaneSlotIDs
        ):
            try container.encode("split_content_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(placement, forKey: .placement)
            try container.encode(kind, forKey: .kind)
            try container.encode(title, forKey: .title)
            try container.encode(payload, forKey: .payload)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .resizeSplit(let splitNodeID, let ratio):
            try container.encode("resize_split", forKey: .type)
            try container.encode(splitNodeID, forKey: .splitNodeID)
            try container.encode(ratio, forKey: .ratio)
        case .equalizeSplits(let tabID):
            try container.encode("equalize_splits", forKey: .type)
            try container.encodeIfPresent(tabID, forKey: .tabID)
        case .movePaneWithinTab(let paneSlotID, let placement):
            try container.encode("move_pane_within_tab", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(placement, forKey: .placement)
        case .movePaneToNewTab(let paneSlotID, let title):
            try container.encode("move_pane_to_new_tab", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encodeIfPresent(title, forKey: .title)
        case .movePaneToTab(let paneSlotID, let targetTabID, let direction):
            try container.encode("move_pane_to_tab", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(targetTabID, forKey: .targetTabID)
            try container.encode(direction, forKey: .direction)
        case .setAttention(let paneSlotID, let attention):
            try container.encode("set_attention", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(attention, forKey: .attention)
        }
    }
}

enum ShellCoreControlSideEffect: Equatable {
    case sendText(paneID: String, text: String)
}

struct ShellCoreControlCommandResult {
    let response: AlanShellControlResponse
    let updatedState: ShellStateSnapshot?
    let sideEffect: ShellCoreControlSideEffect?
}

private typealias ShellCoreABIVersionFunction = @convention(c) () -> UInt32
private typealias ShellCoreHandleRequestOutFunction =
    @convention(c) (
        UnsafePointer<UInt8>?,
        Int,
        UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
        UnsafeMutablePointer<Int>?
    ) -> UInt8
private typealias ShellCoreFreeBytesFunction =
    @convention(c) (UnsafeMutablePointer<UInt8>?, Int) -> Void

private enum ShellCoreFFIAdapterError: Error, CustomStringConvertible {
    case libraryLoadFailed(String, String)
    case symbolMissing(String, String)
    case abiVersionMismatch(expected: UInt32, actual: UInt32)
    case requestFailed
    case nullResponseBuffer
    case facadeError(ShellCoreErrorPayload)
    case missingPayload(String)
    case materializationFailed(String)
    case reducerError(code: String, message: String)

    var description: String {
        switch self {
        case .libraryLoadFailed(let path, let message):
            return "failed to load shell core FFI library at \(path): \(message)"
        case .symbolMissing(let name, let message):
            return "missing shell core FFI symbol \(name): \(message)"
        case .abiVersionMismatch(let expected, let actual):
            return "shell core FFI ABI version mismatch: expected \(expected), got \(actual)"
        case .requestFailed:
            return "shell core FFI request failed before producing a response buffer"
        case .nullResponseBuffer:
            return "shell core FFI returned a null response buffer"
        case .facadeError(let error):
            return "shell core FFI \(error.code): \(error.message)"
        case .missingPayload(let operation):
            return "shell core FFI operation \(operation) returned neither payload nor error"
        case .materializationFailed(let message):
            return "shell core FFI materialization failed: \(message)"
        case .reducerError(let code, let message):
            return "shell core FFI reducer \(code): \(message)"
        }
    }
}

private struct ShellCoreResponseEnvelope: Decodable {
    let payload: Data?
    let error: ShellCoreErrorPayload?

    private enum CodingKeys: String, CodingKey {
        case payload
        case error
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        error = try container.decodeIfPresent(ShellCoreErrorPayload.self, forKey: .error)
        if container.contains(.payload),
           !(try container.decodeNil(forKey: .payload)) {
            let rawPayload = try container.decode(RawJSONValue.self, forKey: .payload)
            payload = try JSONSerialization.data(withJSONObject: rawPayload.value)
        } else {
            payload = nil
        }
    }
}

#if ALAN_SHELL_CORE_FFI_TESTING
extension ShellCoreFFIAdapter {
    func testingSend<Input: Encodable, Output: Decodable>(
        operation: String,
        payload: Input,
        as _: Output.Type
    ) throws -> Output {
        try send(operation: operation, payload: payload)
    }
}
#endif

private struct ShellCoreErrorPayload: Decodable {
    let code: String
    let message: String
}

private struct DefaultManifestPayload: Encodable {
    let windowID: String
    let defaultWorkingDirectory: String
    let now: String

    private enum CodingKeys: String, CodingKey {
        case windowID = "window_id"
        case defaultWorkingDirectory = "default_working_directory"
        case now
    }
}

private struct LegacyManifestPayload: Encodable {
    let manifest: ShellWorkspaceManifest
}

private struct PruningExpiredTabsPayload: Encodable {
    let manifest: ShellContentWorkspaceManifest
    let now: String
    let ttlSeconds: Int64

    private enum CodingKeys: String, CodingKey {
        case manifest
        case now
        case ttlSeconds = "ttl_seconds"
    }
}

private struct MaterializeManifestPayload: Encodable {
    let manifest: ShellContentWorkspaceManifest
    let defaultWorkingDirectory: String
    let now: String

    private enum CodingKeys: String, CodingKey {
        case manifest
        case defaultWorkingDirectory = "default_working_directory"
        case now
    }
}

private struct ManifestPayload: Decodable {
    let manifest: ShellContentWorkspaceManifest
}

private struct MaterializedWorkspaceStatePayload: Decodable {
    let state: ShellCorePortableWorkspaceState
}

private struct ShellCoreEmptyPayload: Encodable {}

private struct ShellCoreReducerApplyPayload: Encodable {
    let state: ShellCorePortableWorkspaceState
    let operation: ShellCoreReducerOperation
}

private struct ShellCoreReducerApplyResponse: Decodable {
    let status: String
    let result: ShellCoreReducerResult?
    let errorCode: String?
    let errorMessage: String?
    let state: ShellCorePortableWorkspaceState?

    private enum CodingKeys: String, CodingKey {
        case status
        case result
        case errorCode = "error_code"
        case errorMessage = "error_message"
        case state
    }
}

private struct ShellCoreReducerResult: Decodable {
    let state: ShellCorePortableWorkspaceState
    let focus: ShellCoreReducerFocus
}

private struct ShellCoreReducerFocus: Decodable {
    let spaceID: String?
    let tabID: String?
    let paneSlotID: String?

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneSlotID = "pane_slot_id"
    }
}

private struct ShellCoreControlHandlePayload: Encodable {
    let state: ShellCorePortableWorkspaceState
    let command: AlanShellControlCommand
}

private struct ShellCoreControlHandleResponse: Decodable {
    let result: ShellCoreControlResult
}

private struct ShellCoreControlResult: Decodable {
    let response: ShellCoreControlResponse
    let updatedState: ShellCorePortableWorkspaceState?
    let runtimeIntents: [ShellCoreControlRuntimeIntent]

    private enum CodingKeys: String, CodingKey {
        case response
        case updatedState = "updated_state"
        case runtimeIntents = "runtime_intents"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        response = try container.decode(ShellCoreControlResponse.self, forKey: .response)
        updatedState = try container.decodeIfPresent(
            ShellCorePortableWorkspaceState.self,
            forKey: .updatedState
        )
        runtimeIntents = try container.decodeIfPresent(
            [ShellCoreControlRuntimeIntent].self,
            forKey: .runtimeIntents
        ) ?? []
    }

    func shellCommandResult(fallbackState: ShellStateSnapshot) throws -> ShellCoreControlCommandResult {
        // shell-core returns portable state that does not carry Swift-only pane fields
        // (live cwd/process/activity/viewport/alanBinding). Merge them back from the live
        // fallback state, matching `applyReducer`, so adopted updates and local control
        // responses don't drop platform data until the next terminal metadata callback.
        let materializedUpdatedState = try updatedState?.materializedShellState()
            .preservingPlatformPaneFields(from: fallbackState)
        let materializedResponseState = try response.state?.materializedShellState()
            .preservingPlatformPaneFields(from: fallbackState)
        let projectionState = materializedResponseState ?? materializedUpdatedState ?? fallbackState
        return ShellCoreControlCommandResult(
            response: try response.shellResponse(
                fallbackState: fallbackState,
                projectionState: projectionState,
                materializedResponseState: materializedResponseState
            ),
            updatedState: materializedUpdatedState,
            sideEffect: runtimeIntents.compactMap(\.sideEffect).first
        )
    }
}

private struct ShellCoreControlResponse: Decodable {
    let requestID: String
    let contractVersion: String
    let applied: Bool?
    let state: ShellCorePortableWorkspaceState?
    let spaces: [ShellCorePortableSpace]?
    let tabs: [ShellCorePortableTab]?
    let paneSlots: [ShellPaneSlot]?
    let contents: [ShellCorePortableContentInstance]?
    let focusedPaneSlotID: String?
    let spaceID: String?
    let targetSpaceID: String?
    let tabID: String?
    let paneID: String?
    let paneSlotID: String?
    let contentID: String?
    let contentKind: ShellContentKind?
    let splitNodeID: String?
    let ratio: Double?
    let changedSplitIDs: [String]?
    let zoomedPaneID: String?
    let previousFocusedPaneSlotID: String?
    let currentFocusedPaneSlotID: String?
    let placement: ShellPaneSplitDirection?
    let errorCode: String?
    let errorMessage: String?

    private enum CodingKeys: String, CodingKey {
        case requestID = "request_id"
        case contractVersion = "contract_version"
        case applied
        case state
        case spaces
        case tabs
        case paneSlots = "pane_slots"
        case contents
        case focusedPaneSlotID = "focused_pane_slot_id"
        case spaceID = "space_id"
        case targetSpaceID = "target_space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
        case contentKind = "content_kind"
        case splitNodeID = "split_node_id"
        case ratio
        case changedSplitIDs = "changed_split_ids"
        case zoomedPaneID = "zoomed_pane_id"
        case previousFocusedPaneSlotID = "previous_focused_pane_slot_id"
        case currentFocusedPaneSlotID = "current_focused_pane_slot_id"
        case placement
        case errorCode = "error_code"
        case errorMessage = "error_message"
    }

    func shellResponse(
        fallbackState: ShellStateSnapshot,
        projectionState: ShellStateSnapshot,
        materializedResponseState: ShellStateSnapshot?
    ) throws -> AlanShellControlResponse {
        let projectedContentState = projectionState.contentStateProjection()
        let contentProjection = projectedContentState.controlPlaneContentProjection(
            paneSlotID: paneSlotID ?? paneID,
            contentID: contentID
        )
        let responseState = materializedResponseState
        return AlanShellControlResponse(
            requestID: requestID,
            contractVersion: contractVersion,
            applied: applied,
            state: responseState,
            spaces: spaces.map { portableSpaces in
                materializedSpaces(
                    portableSpaces,
                    from: projectedContentState
                ) ?? projectionState.spaces
            },
            tabs: tabs?.map(\.shellTab),
            panes: nil,
            paneSlots: paneSlots,
            contents: contents?.map(\.contentInstance),
            pane: nil,
            items: nil,
            candidates: nil,
            events: nil,
            focusedPaneID: projectionState.focusedPaneID ?? fallbackState.focusedPaneID,
            focusedPaneSlotID: focusedPaneSlotID ?? projectedContentState.focusedPaneSlotID,
            spaceID: spaceID,
            sourceSpaceID: nil,
            targetSpaceID: targetSpaceID,
            tabID: tabID,
            paneID: paneID,
            paneSlotID: paneSlotID ?? contentProjection.paneSlotID,
            contentID: contentID ?? contentProjection.contentID,
            contentKind: contentKind ?? contentProjection.kind,
            contentTitle: contentProjection.title,
            contentCapabilities: contentProjection.capabilities,
            section: nil,
            index: nil,
            acceptedBytes: nil,
            deliveryCode: nil,
            runtimePhase: nil,
            terminalRenderMetrics: nil,
            latestEventID: nil,
            splitNodeID: splitNodeID,
            ratio: ratio,
            changedSplitIDs: changedSplitIDs,
            affectedPaneIDs: nil,
            zoomedPaneID: zoomedPaneID,
            sourceTabID: nil,
            targetTabID: nil,
            previousFocusedPaneID: previousFocusedPaneSlotID,
            currentFocusedPaneID: currentFocusedPaneSlotID,
            previousFocusedPaneSlotID: previousFocusedPaneSlotID,
            currentFocusedPaneSlotID: currentFocusedPaneSlotID,
            splitDirection: nil,
            spatialDirection: nil,
            placement: placement,
            mountedContentInstanceID: nil,
            diagnosticsEnabled: nil,
            diagnosticsRetainedEventCount: nil,
            diagnosticsStutterMarkerCount: nil,
            diagnosticsBundlePath: nil,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }

    private func materializedSpaces(
        _ portableSpaces: [ShellCorePortableSpace],
        from contentState: ShellContentStateSnapshot
    ) -> [ShellSpace]? {
        ShellContentStateSnapshot(
            contractVersion: contentState.contractVersion,
            windowID: contentState.windowID,
            focusedSpaceID: contentState.focusedSpaceID,
            focusedTabID: contentState.focusedTabID,
            focusedPaneSlotID: contentState.focusedPaneSlotID,
            spaces: portableSpaces.map(\.contentSpace),
            paneSlots: contentState.paneSlots,
            contents: contentState.contents
        )
        .materializingShellState()?
        .spaces
    }
}

private enum ShellCoreControlRuntimeIntent: Decodable {
    case sendTerminalText(paneSlotID: String, contentID: String, text: String)
    case sendTerminalKey(paneSlotID: String, contentID: String, key: TerminalRuntimeControlKey)
    case reducer
    case unsupported

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
        case text
        case key
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "send_terminal_text":
            self = .sendTerminalText(
                paneSlotID: try container.decode(String.self, forKey: .paneSlotID),
                contentID: try container.decode(String.self, forKey: .contentID),
                text: try container.decode(String.self, forKey: .text)
            )
        case "send_terminal_key":
            self = .sendTerminalKey(
                paneSlotID: try container.decode(String.self, forKey: .paneSlotID),
                contentID: try container.decode(String.self, forKey: .contentID),
                key: try container.decode(TerminalRuntimeControlKey.self, forKey: .key)
            )
        case "reducer":
            self = .reducer
        default:
            self = .unsupported
        }
    }

    var sideEffect: ShellCoreControlSideEffect? {
        switch self {
        case .sendTerminalText(let paneSlotID, _, let text):
            return .sendText(paneID: paneSlotID, text: text)
        case .sendTerminalKey(let paneSlotID, _, .returnKey):
            return .sendText(paneID: paneSlotID, text: "\r")
        case .sendTerminalKey:
            return nil
        case .reducer, .unsupported:
            return nil
        }
    }
}

private struct ShellCoreStandardActionDescriptorsResponse: Decodable {
    let actions: [ShellCoreActionDescriptor]
}

private struct ShellCoreActionDescriptor: Decodable {
    let id: ShellActionID
    let title: String
}

private struct ShellCoreActionDefaultShortcutPayload: Encodable {
    let id: ShellActionID
    let target: ShellCoreActionTarget
}

private struct ShellCoreDefaultActionShortcutResponse: Decodable {
    let shortcut: ShellCoreActionShortcut?
}

private struct ShellCoreKeyboardActionPayload: Encodable {
    let shortcut: ShellCoreActionShortcut
}

private struct ShellCoreKeyboardActionResponse: Decodable {
    let keyboardAction: ShellCoreKeyboardAction?

    private enum CodingKeys: String, CodingKey {
        case keyboardAction = "keyboard_action"
    }
}

private struct ShellCoreKeyboardAction: Decodable {
    let id: ShellActionID
    let target: ShellCoreActionTarget

    var shellKeyboardAction: ShellKeyboardAction {
        ShellKeyboardAction(id: id, target: target.shellTarget)
    }
}

private struct ShellCoreActionExecutePayload: Encodable {
    let state: ShellCorePortableWorkspaceState
    let id: ShellActionID
    let target: ShellCoreActionTarget
}

private struct ShellCoreActionExecuteResponse: Decodable {
    let result: ShellCoreActionExecutionResult
}

private struct ShellCoreActionExecutionResult: Decodable {
    let status: ShellCoreActionExecutionStatus
    let effect: ShellCoreActionEffect?
    let reason: String?
}

private enum ShellCoreActionExecutionStatus: String, Decodable {
    case executed
    case failed
    case unavailable
}

private struct ShellCoreActionShortcut: Codable {
    let key: String
    let modifiers: [ShellActionModifier]
    let context: ShellActionShortcutContext

    init(_ shortcut: ShellActionShortcut) {
        key = shortcut.key
        modifiers = shortcut.modifiers.sorted()
        context = shortcut.context
    }

    var shellShortcut: ShellActionShortcut {
        ShellActionShortcut(key: key, modifiers: Set(modifiers), context: context)
    }
}

private enum ShellCoreActionTarget: Codable {
    case currentSelection
    case contextTab(String)
    case contextPane(String)
    case contextSpace(String)
    case spaceIndex(Int)
    case tabToSpace(tabID: String, spaceID: String)
    case unresolved

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
        case paneID = "pane_id"
        case spaceID = "space_id"
        case index
    }

    init(_ target: ShellActionTarget) {
        switch target {
        case .currentSelection:
            self = .currentSelection
        case .contextTab(let tabID):
            self = .contextTab(tabID)
        case .contextPane(let paneID):
            self = .contextPane(paneID)
        case .contextSpace(let spaceID):
            self = .contextSpace(spaceID)
        case .spaceIndex(let index):
            self = .spaceIndex(index)
        case .tabToSpace(let tabID, let spaceID):
            self = .tabToSpace(tabID: tabID, spaceID: spaceID)
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "current_selection":
            self = .currentSelection
        case "context_tab":
            self = .contextTab(try container.decode(String.self, forKey: .tabID))
        case "context_pane":
            self = .contextPane(try container.decode(String.self, forKey: .paneID))
        case "context_space":
            self = .contextSpace(try container.decode(String.self, forKey: .spaceID))
        case "space_index":
            self = .spaceIndex(try container.decode(Int.self, forKey: .index))
        case "tab_to_space":
            self = .tabToSpace(
                tabID: try container.decode(String.self, forKey: .tabID),
                spaceID: try container.decode(String.self, forKey: .spaceID)
            )
        default:
            self = .unresolved
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .currentSelection:
            try container.encode("current_selection", forKey: .type)
        case .contextTab(let tabID):
            try container.encode("context_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .contextPane(let paneID):
            try container.encode("context_pane", forKey: .type)
            try container.encode(paneID, forKey: .paneID)
        case .contextSpace(let spaceID):
            try container.encode("context_space", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
        case .spaceIndex(let index):
            try container.encode("space_index", forKey: .type)
            try container.encode(index, forKey: .index)
        case .tabToSpace(let tabID, let spaceID):
            try container.encode("tab_to_space", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(spaceID, forKey: .spaceID)
        case .unresolved:
            try container.encode("unresolved", forKey: .type)
        }
    }

    var shellTarget: ShellActionTarget {
        switch self {
        case .currentSelection, .unresolved:
            return .currentSelection
        case .contextTab(let tabID):
            return .contextTab(tabID)
        case .contextPane(let paneID):
            return .contextPane(paneID)
        case .contextSpace(let spaceID):
            return .contextSpace(spaceID)
        case .spaceIndex(let index):
            return .spaceIndex(index)
        case .tabToSpace(let tabID, let spaceID):
            return .tabToSpace(tabID: tabID, spaceID: spaceID)
        }
    }
}

private enum ShellCoreActionEffect: Decodable {
    case workspaceCommand(ShellWorkspaceCommand)
    case openTab(ShellLaunchTarget, spaceID: String?)
    case closeTab(String?)
    case renameTab(String?)
    case duplicateTab(String?)
    case openTabInSplitView(String?)
    case closePane(String?)
    case selectAdjacentTab(Int)
    case selectAdjacentSpace(Int)
    case selectSpaceAt(Int)
    case pinTab(String?)
    case unpinTab(String?)
    case updatePinnedTab(String?)
    case moveTab(String?, offset: Int)
    case moveTabToSpace(tabID: String?, spaceID: String?)
    case movePaneInTab(String?, placement: ShellPaneSplitDirection)
    case promoteQuickTerminal(spaceID: String?)
    case terminalClear(String?)
    case disabledPlaceholder

    private enum CodingKeys: String, CodingKey {
        case type
        case command
        case launchTarget = "launch_target"
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case offset
        case index
        case placement
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "workspace_command":
            self = .workspaceCommand(try container.decode(ShellWorkspaceCommand.self, forKey: .command))
        case "open_tab":
            self = .openTab(
                try container.decode(ShellLaunchTarget.self, forKey: .launchTarget),
                spaceID: try container.decodeIfPresent(String.self, forKey: .spaceID)
            )
        case "close_tab":
            self = .closeTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "rename_tab":
            self = .renameTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "duplicate_tab":
            self = .duplicateTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "open_tab_in_split_view":
            self = .openTabInSplitView(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "close_pane":
            self = .closePane(try container.decodeIfPresent(String.self, forKey: .paneID))
        case "select_adjacent_tab":
            self = .selectAdjacentTab(try container.decode(Int.self, forKey: .offset))
        case "select_adjacent_space":
            self = .selectAdjacentSpace(try container.decode(Int.self, forKey: .offset))
        case "select_space_at":
            self = .selectSpaceAt(try container.decode(Int.self, forKey: .index))
        case "pin_tab":
            self = .pinTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "unpin_tab":
            self = .unpinTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "update_pinned_tab":
            self = .updatePinnedTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "move_tab":
            self = .moveTab(
                try container.decodeIfPresent(String.self, forKey: .tabID),
                offset: try container.decode(Int.self, forKey: .offset)
            )
        case "move_tab_to_space":
            self = .moveTabToSpace(
                tabID: try container.decodeIfPresent(String.self, forKey: .tabID),
                spaceID: try container.decodeIfPresent(String.self, forKey: .spaceID)
            )
        case "move_pane_in_tab":
            self = .movePaneInTab(
                try container.decodeIfPresent(String.self, forKey: .paneID),
                placement: try container.decode(ShellPaneSplitDirection.self, forKey: .placement)
            )
        case "promote_quick_terminal":
            self = .promoteQuickTerminal(
                spaceID: try container.decodeIfPresent(String.self, forKey: .spaceID)
            )
        case "terminal_clear":
            self = .terminalClear(try container.decodeIfPresent(String.self, forKey: .paneID))
        case "disabled_placeholder":
            self = .disabledPlaceholder
        default:
            self = .disabledPlaceholder
        }
    }

    var shellActionEffect: ShellActionEffect {
        switch self {
        case .workspaceCommand(let command):
            return .workspaceCommand(command)
        case .openTab(let launchTarget, let spaceID):
            return .openTab(launchTarget, spaceID: spaceID)
        case .closeTab(let tabID):
            return .closeTab(tabID)
        case .renameTab(let tabID):
            return .renameTab(tabID)
        case .duplicateTab(let tabID):
            return .duplicateTab(tabID)
        case .openTabInSplitView(let tabID):
            return .openTabInSplitView(tabID)
        case .closePane(let paneID):
            return .closePane(paneID)
        case .selectAdjacentTab(let offset):
            return .selectAdjacentTab(offset)
        case .selectAdjacentSpace(let offset):
            return .selectAdjacentSpace(offset)
        case .selectSpaceAt(let index):
            return .selectSpaceAt(index)
        case .pinTab(let tabID):
            return .pinTab(tabID)
        case .unpinTab(let tabID):
            return .unpinTab(tabID)
        case .updatePinnedTab(let tabID):
            return .updatePinnedTab(tabID)
        case .moveTab(let tabID, let offset):
            return .moveTab(tabID, offset: offset)
        case .moveTabToSpace(let tabID, let spaceID):
            return .moveTabToSpace(tabID: tabID, spaceID: spaceID)
        case .movePaneInTab(let paneID, let placement):
            return .movePaneInTab(paneID, placement: placement)
        case .promoteQuickTerminal(let spaceID):
            return .promoteQuickTerminal(spaceID: spaceID)
        case .terminalClear(let paneID):
            return .terminalClear(paneID)
        case .disabledPlaceholder:
            return .disabledPlaceholder
        }
    }
}

private struct ShellCoreSettingsRowsResponse: Decodable {
    let rows: [ShellCoreSettingsRowSummary]
}

private struct ShellCoreSettingsRowSummary: Decodable {
    let id: String
    let systemName: String
    let title: String
    let detail: String?
    let value: String?
    let mutability: ShellCoreSettingsRowMutability
    let offersFreeformEditing: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case systemName = "system_name"
        case title
        case detail
        case value
        case mutability
        case offersFreeformEditing = "offers_freeform_editing"
    }

    var settingsRow: ShellSettingsRowModel {
        ShellSettingsRowModel(
            id: id,
            systemName: systemName,
            title: title,
            detail: detail,
            value: value,
            mutability: mutability.settingsMutability,
            offersFreeformEditing: offersFreeformEditing
        )
    }
}

private enum ShellCoreSettingsRowMutability: String, Decodable {
    case editable
    case readOnly = "read_only"
    case actionOnly = "action_only"
    case deferred

    var settingsMutability: ShellSettingsRowMutability {
        switch self {
        case .editable:
            return .editable
        case .readOnly:
            return .readOnly
        case .actionOnly:
            return .actionOnly
        case .deferred:
            return .deferred
        }
    }
}

private struct ShellCoreTerminalProfileSettingsSummaryPayload: Encodable {
    let profiles: [TerminalProfileDefinition]
    let defaultProfileID: String
    let recoveryMessage: String?

    private enum CodingKeys: String, CodingKey {
        case profiles
        case defaultProfileID = "default_profile_id"
        case recoveryMessage = "recovery_message"
    }

    init(_ summary: TerminalProfileSettingsSummary) {
        profiles = summary.profiles
        defaultProfileID = summary.defaultProfileID
        recoveryMessage = summary.recoveryMessage
    }
}

private struct ShellCoreCapabilitiesSettingsSummaryPayload: Encodable {
    let skills: [ShellCoreSettingsSkillSummaryPayload]
    let unavailableReason: String?

    private enum CodingKeys: String, CodingKey {
        case skills
        case unavailableReason = "unavailable_reason"
    }

    init(_ summary: ShellSettingsCapabilitiesSummary) {
        skills = summary.skills.map(ShellCoreSettingsSkillSummaryPayload.init)
        unavailableReason = summary.unavailableReason
    }
}

private struct ShellCoreSettingsSkillSummaryPayload: Encodable {
    let id: String
    let name: String
    let enabled: Bool
    let allowImplicitInvocation: Bool
    let available: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case enabled
        case allowImplicitInvocation = "allow_implicit_invocation"
        case available
    }

    init(_ summary: ShellSettingsSkillSummary) {
        id = summary.id
        name = summary.name
        enabled = summary.enabled
        allowImplicitInvocation = summary.allowImplicitInvocation
        available = summary.available
    }
}

private struct ShellCoreLocalRowsPayload: Encodable {
    let local: ShellCoreLocalSettingsSummaryPayload
    let diagnostics: ShellCoreDiagnosticsSettingsSummaryPayload

    init(local: ShellSettingsLocalSummary, diagnostics: ShellSettingsDiagnosticsSummary) {
        self.local = ShellCoreLocalSettingsSummaryPayload(local)
        self.diagnostics = ShellCoreDiagnosticsSettingsSummaryPayload(diagnostics)
    }
}

private struct ShellCoreLocalSettingsSummaryPayload: Encodable {
    let bundleIdentifier: String
    let channelLabel: String
    let cliToolName: String
    let daemonURL: String
    let daemonBindAddress: String
    let updateSummary: String
    let updateDetail: String
    let alanHomeDisplayPath: String
    let applicationSupportDisplayPath: String
    let globalSkillsDisplayPath: String
    let shellControlNamespace: String

    private enum CodingKeys: String, CodingKey {
        case bundleIdentifier = "bundle_identifier"
        case channelLabel = "channel_label"
        case cliToolName = "cli_tool_name"
        case daemonURL = "daemon_url"
        case daemonBindAddress = "daemon_bind_address"
        case updateSummary = "update_summary"
        case updateDetail = "update_detail"
        case alanHomeDisplayPath = "alan_home_display_path"
        case applicationSupportDisplayPath = "application_support_display_path"
        case globalSkillsDisplayPath = "global_skills_display_path"
        case shellControlNamespace = "shell_control_namespace"
    }

    init(_ summary: ShellSettingsLocalSummary) {
        bundleIdentifier = summary.bundleIdentifier
        channelLabel = summary.channelLabel
        cliToolName = summary.cliToolName
        daemonURL = summary.daemonURL
        daemonBindAddress = summary.daemonBindAddress
        updateSummary = summary.updateSummary
        updateDetail = summary.updateDetail
        alanHomeDisplayPath = summary.alanHomeDisplayPath
        applicationSupportDisplayPath = summary.applicationSupportDisplayPath
        globalSkillsDisplayPath = summary.globalSkillsDisplayPath
        shellControlNamespace = summary.shellControlNamespace
    }
}

private struct ShellCoreDiagnosticsSettingsSummaryPayload: Encodable {
    let isEnabled: Bool
    let retainedEventCount: UInt32
    let stutterMarkerCount: UInt32
    let lastExportURL: String?

    private enum CodingKeys: String, CodingKey {
        case isEnabled = "is_enabled"
        case retainedEventCount = "retained_event_count"
        case stutterMarkerCount = "stutter_marker_count"
        case lastExportURL = "last_export_url"
    }

    init(_ summary: ShellSettingsDiagnosticsSummary) {
        isEnabled = summary.isEnabled
        retainedEventCount = Self.clampedUInt32(summary.retainedEventCount)
        stutterMarkerCount = Self.clampedUInt32(summary.stutterMarkerCount)
        lastExportURL = summary.lastExportURL?.path
    }

    private static func clampedUInt32(_ value: Int) -> UInt32 {
        UInt32(min(max(value, 0), Int(UInt32.max)))
    }
}

private struct ShellCoreTerminalProfileValidationResponse: Decodable {
    let isValid: Bool
    let errors: [ShellCoreTerminalProfileValidationError]

    private enum CodingKeys: String, CodingKey {
        case isValid = "is_valid"
        case errors
    }

    var validationResult: TerminalProfileValidationResult {
        TerminalProfileValidationResult(errors: errors.map(\.swiftError))
    }
}

private struct ShellCoreTerminalProfileEditorResponse: Decodable {
    let isValid: Bool
    let definition: TerminalProfileDefinition?
    let errors: [ShellCoreTerminalProfileValidationError]

    private enum CodingKeys: String, CodingKey {
        case isValid = "is_valid"
        case definition
        case errors
    }

    var editorResult: TerminalProfileEditorResult {
        TerminalProfileEditorResult(
            definition: isValid ? definition : nil,
            errors: errors.map(\.swiftError)
        )
    }
}

private struct ShellCoreTerminalProfileEditorDraft: Encodable {
    let id: String
    let title: String
    let launchKind: TerminalProfileLaunchKind
    let unixUser: String
    let customCommand: String
    let defaultWorkingDirectory: String?
    let presentation: TerminalProfilePresentation?
    let managedTerminalAccountID: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case title
        case launchKind = "launch_kind"
        case unixUser = "unix_user"
        case customCommand = "custom_command"
        case defaultWorkingDirectory = "default_working_directory"
        case presentation
        case managedTerminalAccountID = "managed_terminal_account_id"
    }

    init(_ draft: TerminalProfileEditorDraft) {
        id = draft.id
        title = draft.title
        launchKind = draft.launchKind
        unixUser = draft.unixUser
        customCommand = draft.customCommand
        defaultWorkingDirectory = draft.defaultWorkingDirectory
        presentation = draft.presentation
        managedTerminalAccountID = draft.managedTerminalAccountID
    }
}

struct ShellCoreTerminalLaunchIntent: Decodable {
    let strategy: String
    let executablePath: String?
    let launchPath: String
    let arguments: [String]
    let bootCommand: String
    let surfaceCommand: String?
    let summary: String
    let detail: String?
    let terminalProfile: TerminalProfileDefinition?
    let workingDirectory: String?
    let profileEnvironment: [String: String]
    private let terminalProfileState: ShellCoreTerminalProfileResolutionState

    private enum CodingKeys: String, CodingKey {
        case strategy
        case executablePath = "executable_path"
        case launchPath = "launch_path"
        case arguments
        case bootCommand = "boot_command"
        case surfaceCommand = "surface_command"
        case summary
        case detail
        case terminalProfile = "terminal_profile"
        case terminalProfileState = "terminal_profile_state"
        case workingDirectory = "working_directory"
        case profileEnvironment = "profile_environment"
    }

    var resolvedTerminalProfileState: TerminalProfileResolutionState {
        terminalProfileState.swiftState
    }

}

private struct ShellCoreTerminalLaunchIntentResponse: Decodable {
    let intent: ShellCoreTerminalLaunchIntent
}

private struct ShellCoreTerminalLaunchIntentPayload: Encodable {
    let terminalProfileReference: String?
    let terminalProfiles: TerminalProfileDocument?
    let availability: ShellCoreTerminalExecutableAvailabilityPayload
    let environment: ShellCoreTerminalLaunchEnvironmentPayload

    private enum CodingKeys: String, CodingKey {
        case terminalProfileReference = "terminal_profile_reference"
        case terminalProfiles = "terminal_profiles"
        case availability
        case environment
    }

    init(
        terminalProfileReference: String?,
        terminalProfiles: TerminalProfileDocument?,
        executablePaths: Set<String>,
        environment: [String: String]
    ) {
        self.terminalProfileReference = terminalProfileReference
        self.terminalProfiles = terminalProfiles
        availability = ShellCoreTerminalExecutableAvailabilityPayload(executablePaths: executablePaths)
        self.environment = ShellCoreTerminalLaunchEnvironmentPayload(values: environment)
    }
}

private struct ShellCoreTerminalExecutableAvailabilityPayload: Encodable {
    let executablePaths: [String]
    let enforce: Bool

    private enum CodingKeys: String, CodingKey {
        case executablePaths = "executable_paths"
        case enforce
    }

    init(executablePaths: Set<String>) {
        self.executablePaths = executablePaths.sorted()
        enforce = true
    }
}

private struct ShellCoreTerminalLaunchEnvironmentPayload: Encodable {
    let values: [String: String]
}

private enum ShellCoreTerminalProfileResolutionState: Decodable {
    case absent
    case resolved
    case missing(requestedID: String)
    case unavailable(requestedID: String, reason: String)

    private enum CodingKeys: String, CodingKey {
        case state
        case requestedID = "requested_id"
        case reason
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .state) {
        case "absent":
            self = .absent
        case "resolved":
            self = .resolved
        case "missing":
            self = .missing(requestedID: try container.decode(String.self, forKey: .requestedID))
        case "unavailable":
            self = .unavailable(
                requestedID: try container.decode(String.self, forKey: .requestedID),
                reason: try container.decode(String.self, forKey: .reason)
            )
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "Unsupported Terminal Profile resolution state"
                )
            )
        }
    }

    var swiftState: TerminalProfileResolutionState {
        switch self {
        case .absent:
            return .absent
        case .resolved:
            return .resolved
        case .missing(let requestedID):
            return .missing(requestedID: requestedID)
        case .unavailable(let requestedID, let reason):
            return .unavailable(requestedID: requestedID, reason: reason)
        }
    }
}

private enum ShellCoreTerminalProfileValidationError: Decodable {
    case missingID
    case duplicateID(String)
    case missingTitle(String)
    case missingUnixUser(String)
    case missingCustomCommand(String)
    case missingDefaultProfile(String)
    case unavailableExecutable(profileID: String, path: String)

    private enum CodingKeys: String, CodingKey {
        case type
        case id
        case profileID = "profile_id"
        case path
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "missing_id":
            self = .missingID
        case "duplicate_id":
            self = .duplicateID(try container.decode(String.self, forKey: .id))
        case "missing_title":
            self = .missingTitle(try container.decode(String.self, forKey: .id))
        case "missing_unix_user":
            self = .missingUnixUser(try container.decode(String.self, forKey: .id))
        case "missing_custom_command":
            self = .missingCustomCommand(try container.decode(String.self, forKey: .id))
        case "missing_default_profile":
            self = .missingDefaultProfile(try container.decode(String.self, forKey: .id))
        case "unavailable_executable":
            self = .unavailableExecutable(
                profileID: try container.decode(String.self, forKey: .profileID),
                path: try container.decode(String.self, forKey: .path)
            )
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "Unsupported Terminal Profile validation error variant"
                )
            )
        }
    }

    var swiftError: TerminalProfileValidationError {
        switch self {
        case .missingID:
            return .missingID
        case .duplicateID(let id):
            return .duplicateID(id)
        case .missingTitle(let id):
            return .missingTitle(id)
        case .missingUnixUser(let id):
            return .missingUnixUser(id)
        case .missingCustomCommand(let id):
            return .missingCustomCommand(id)
        case .missingDefaultProfile(let id):
            return .missingDefaultProfile(id)
        case .unavailableExecutable(let profileID, let path):
            return .unavailableExecutable(profileID: profileID, path: path)
        }
    }
}

private struct ShellCorePortableWorkspaceState: Codable {
    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneID: String?
    let spaces: [ShellCorePortableSpace]
    let paneSlots: [ShellPaneSlot]
    let contents: [ShellCorePortableContentInstance]
    let quickTerminal: ShellCorePortableQuickTerminalState?

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneID = "focused_pane_id"
        case spaces
        case paneSlots = "pane_slots"
        case contents
        case quickTerminal = "quick_terminal"
    }

    init(projecting state: ShellStateSnapshot) {
        let contentState = state.contentStateProjection()
        let projectedQuickTerminal = ShellCorePortableQuickTerminalState(projecting: state)
        contractVersion = contentState.contractVersion
        windowID = contentState.windowID
        focusedSpaceID = contentState.focusedSpaceID
        focusedTabID = contentState.focusedTabID
        if projectedQuickTerminal?.paneID == state.focusedPaneID {
            focusedPaneID = state.focusedPaneID
        } else {
            focusedPaneID = contentState.focusedPaneSlotID
        }
        spaces = contentState.spaces.map(ShellCorePortableSpace.init(contentSpace:))
        paneSlots = contentState.paneSlots
        contents = contentState.contents.map(ShellCorePortableContentInstance.init(contentInstance:))
        quickTerminal = projectedQuickTerminal
    }

    func materializedShellState() throws -> ShellStateSnapshot {
        let contentState = ShellContentStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: focusedSpaceID,
            focusedTabID: focusedTabID,
            focusedPaneSlotID: focusedPaneID,
            spaces: spaces.map(\.contentSpace),
            paneSlots: paneSlots,
            contents: contents.map(\.contentInstance)
        )
        guard var shellState = contentState.materializingShellState() else {
            throw ShellCoreFFIAdapterError.materializationFailed(
                "portable workspace state could not be projected into shell state"
            )
        }
        guard let quickTerminal,
              let restoredQuickTerminal = quickTerminal.materialized()
        else {
            return shellState
        }
        guard !shellState.panes.contains(where: { $0.paneID == restoredQuickTerminal.pane.paneID }) else {
            return ShellStateSnapshot(
                contractVersion: shellState.contractVersion,
                windowID: shellState.windowID,
                focusedSpaceID: shellState.focusedSpaceID,
                focusedTabID: shellState.focusedTabID,
                focusedPaneID: shellState.focusedPaneID,
                spaces: shellState.spaces,
                panes: shellState.panes,
                paneSlots: shellState.paneSlots,
                contents: shellState.contents,
                quickTerminal: restoredQuickTerminal.slot
            )
        }

        var materializedContents = shellState.contents ?? []
        if !materializedContents.contains(where: { $0.contentID == restoredQuickTerminal.content.contentID }) {
            materializedContents.append(restoredQuickTerminal.content)
        }
        shellState = ShellStateSnapshot(
            contractVersion: shellState.contractVersion,
            windowID: shellState.windowID,
            focusedSpaceID: shellState.focusedSpaceID,
            focusedTabID: shellState.focusedTabID,
            focusedPaneID: shellState.focusedPaneID,
            spaces: shellState.spaces,
            panes: shellState.panes + [restoredQuickTerminal.pane],
            paneSlots: shellState.paneSlots,
            contents: materializedContents.isEmpty ? nil : materializedContents,
            quickTerminal: restoredQuickTerminal.slot
        )
        return shellState
    }
}

private struct ShellCorePortableSpace: Codable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [ShellCorePortableTab]
    let selectedTabID: String?
    let terminalProfileID: String?
    let presentationIconSystemName: String?

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case selectedTabID = "selected_tab_id"
        case terminalProfileID = "terminal_profile_id"
        case presentationIconSystemName = "presentation_icon"
    }

    init(contentSpace: ShellContentSpace) {
        spaceID = contentSpace.spaceID
        title = contentSpace.title
        attention = contentSpace.attention
        tabs = contentSpace.tabs.map(ShellCorePortableTab.init(contentTab:))
        selectedTabID = contentSpace.selectedTabID
        terminalProfileID = contentSpace.terminalProfileID
        presentationIconSystemName = contentSpace.presentationIconSystemName
    }

    var contentSpace: ShellContentSpace {
        ShellContentSpace(
            spaceID: spaceID,
            title: title,
            attention: attention,
            tabs: tabs.map(\.contentTab),
            selectedTabID: selectedTabID,
            terminalProfileID: terminalProfileID,
            presentationIconSystemName: presentationIconSystemName
        )
    }
}

private struct ShellCorePortableTab: Codable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: ShellPaneTreeNode
    let isPinned: Bool
    let isTitleUserLocked: Bool

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
    }

    init(contentTab: ShellContentTab) {
        tabID = contentTab.tabID
        kind = contentTab.kind
        title = contentTab.title
        paneTree = contentTab.paneTree.restoringPaneTree()
        isPinned = contentTab.isPinned
        isTitleUserLocked = contentTab.isTitleUserLocked
    }

    var contentTab: ShellContentTab {
        ShellContentTab(
            tabID: tabID,
            kind: kind,
            title: title,
            paneTree: ShellPaneSlotTreeNode.migrating(paneTree: paneTree),
            isPinned: isPinned,
            isTitleUserLocked: isTitleUserLocked
        )
    }

    var shellTab: ShellTab {
        ShellTab(
            tabID: tabID,
            kind: kind,
            title: title,
            paneTree: paneTree,
            isPinned: isPinned,
            isTitleUserLocked: isTitleUserLocked
        )
    }
}

private struct ShellCorePortableContentInstance: Codable {
    let contentID: String
    let kind: ShellContentKind
    let title: String
    let iconName: String?
    let capabilities: [ShellContentCapability]
    let payload: ShellContentPayload
    let lifecycle: ShellContentLifecycleState

    private enum CodingKeys: String, CodingKey {
        case contentID = "content_id"
        case kind
        case title
        case iconName = "icon_name"
        case capabilities
        case payload
        case lifecycle
    }

    init(contentInstance: ShellContentInstance) {
        contentID = contentInstance.contentID
        kind = contentInstance.kind
        title = contentInstance.title
        iconName = contentInstance.iconName
        capabilities = contentInstance.capabilities
        payload = contentInstance.payload
        lifecycle = contentInstance.lifecycle
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        contentID = try container.decode(String.self, forKey: .contentID)
        kind = try container.decode(ShellContentKind.self, forKey: .kind)
        title = try container.decode(String.self, forKey: .title)
        iconName = try container.decodeIfPresent(String.self, forKey: .iconName)
        capabilities = try container.decodeIfPresent(
            [ShellContentCapability].self,
            forKey: .capabilities
        ) ?? ShellContentInstance.defaultCapabilities(for: kind)
        payload = try container.decodeIfPresent(ShellContentPayload.self, forKey: .payload)
            ?? ShellContentPayload(terminal: nil, markdown: nil, settings: nil)
        lifecycle = try container.decodeIfPresent(
            ShellContentLifecycleState.self,
            forKey: .lifecycle
        ) ?? .active
    }

    var contentInstance: ShellContentInstance {
        ShellContentInstance(
            contentID: contentID,
            kind: kind,
            title: title,
            iconName: iconName,
            capabilities: capabilities,
            payload: payload,
            lifecycle: lifecycle,
            rendererState: Self.materializedRendererState(kind: kind, payload: payload)
        )
    }

    /// shell-core's portable contents do not carry the Swift-only `rendererState`. Terminal
    /// renderer state is recomputed from live pane context during content projection, but
    /// markdown/settings contents have no runtime to repopulate it, so reconstruct the same
    /// "ready" state the native mount path assigns instead of leaving them at `.placeholder`
    /// (which would otherwise report non-terminal panes as not ready in the event stream).
    private static func materializedRendererState(
        kind: ShellContentKind,
        payload: ShellContentPayload
    ) -> ShellContentRendererState {
        switch kind {
        case .terminal:
            return .placeholder
        case .markdown:
            let detail = payload.markdown.flatMap { URL(string: $0.fileURL)?.path }
            return ShellContentRendererState(phase: "ready", detail: detail)
        case .settings:
            return ShellContentRendererState(phase: "ready", detail: payload.settings?.surfaceID)
        }
    }
}

private struct ShellCorePortableQuickTerminalState: Codable {
    let paneID: String
    let presentation: ShellQuickTerminalPresentation
    let lastWorkingDirectory: String?
    let contentID: String
    let terminalPayload: ShellTerminalContentPayload?
    let terminalMetadata: ShellCorePortableTerminalMetadata?
    let attention: ShellAttentionState

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case presentation
        case lastWorkingDirectory = "last_working_directory"
        case contentID = "content_id"
        case terminalPayload = "terminal_payload"
        case terminalMetadata = "terminal_metadata"
        case attention
    }

    init?(projecting state: ShellStateSnapshot) {
        guard let slot = state.quickTerminal else { return nil }
        let pane = state.panes.first { $0.paneID == slot.paneID }
        let contentID = pane?.terminalContentID
            ?? ShellContentInstance.terminalContentID(forPaneID: slot.paneID)
        let content = state.contents?.first { $0.contentID == contentID }
        let terminalPayload = content?.payload.terminal ?? pane.map {
            ShellTerminalContentPayload(
                launchTarget: $0.resolvedLaunchTarget,
                cwd: $0.cwd,
                title: $0.viewport?.title,
                terminalProfileID: $0.terminalProfileID
            )
        }

        self.paneID = slot.paneID
        presentation = slot.presentation
        lastWorkingDirectory = slot.lastWorkingDirectory
        self.contentID = contentID
        self.terminalPayload = terminalPayload
        terminalMetadata = pane.map {
            ShellCorePortableTerminalMetadata(
                title: $0.viewport?.title,
                cwd: $0.cwd,
                activity: $0.activity
            )
        }
        attention = pane?.attention ?? .idle
    }

    func materialized() -> (
        slot: ShellQuickTerminalSlot,
        pane: ShellPane,
        content: ShellContentInstance
    )? {
        guard let terminalPayload else { return nil }
        let title = terminalPayload.title ?? terminalMetadata?.title ?? "Shell"
        let payload = ShellTerminalContentPayload(
            launchTarget: terminalPayload.launchTarget,
            cwd: terminalPayload.cwd ?? terminalMetadata?.cwd,
            title: terminalPayload.title ?? terminalMetadata?.title,
            transcriptSnapshot: terminalPayload.transcriptSnapshot,
            terminalProfileID: terminalPayload.terminalProfileID
        )
        let content = ShellContentInstance(
            contentID: contentID,
            kind: .terminal,
            title: title,
            payload: .terminal(payload),
            rendererState: .placeholder
        )
        let pane = ShellPane(
            paneID: paneID,
            tabID: ShellQuickTerminalSlot.globalTabID,
            spaceID: ShellQuickTerminalSlot.globalSpaceID,
            launchTarget: payload.launchTarget,
            cwd: payload.cwd,
            process: nil,
            attention: attention,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: title,
                summary: nil,
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            activity: terminalMetadata?.activity,
            alanBinding: nil,
            terminalProfileID: payload.terminalProfileID
        )
        let slot = ShellQuickTerminalSlot(
            paneID: paneID,
            presentation: presentation,
            lastWorkingDirectory: lastWorkingDirectory ?? payload.cwd
        )
        return (slot, pane, content)
    }
}

private struct ShellCorePortableTerminalMetadata: Codable {
    let title: String?
    let cwd: String?
    let activity: TerminalActivitySnapshot?
}

private extension ShellStateSnapshot {
    func preservingPlatformPaneFields(from authoritative: ShellStateSnapshot) -> ShellStateSnapshot {
        let authoritativePanesByID = Dictionary(
            uniqueKeysWithValues: authoritative.panes.map { ($0.paneID, $0) }
        )
        let mergedPanes = panes.map { pane in
            pane.preservingPlatformFields(from: authoritativePanesByID[pane.paneID])
        }

        return ShellStateSnapshot(
            contractVersion: contractVersion,
            windowID: windowID,
            focusedSpaceID: focusedSpaceID,
            focusedTabID: focusedTabID,
            focusedPaneID: focusedPaneID,
            spaces: spaces,
            panes: mergedPanes,
            paneSlots: paneSlots,
            contents: contents,
            quickTerminal: quickTerminal
        )
    }
}

private extension ShellPane {
    func preservingPlatformFields(from authoritative: ShellPane?) -> ShellPane {
        guard let authoritative else { return self }

        return ShellPane(
            paneID: paneID,
            tabID: tabID,
            spaceID: spaceID,
            launchTarget: launchTarget ?? authoritative.launchTarget,
            cwd: cwd ?? authoritative.cwd,
            process: process ?? authoritative.process,
            attention: attention,
            context: context.preservingPlatformFields(from: authoritative.context),
            viewport: viewport.preservingPlatformFields(from: authoritative.viewport),
            activity: activity ?? authoritative.activity,
            alanBinding: alanBinding ?? authoritative.alanBinding,
            terminalProfileID: terminalProfileID ?? authoritative.terminalProfileID
        )
    }
}

private extension Optional where Wrapped == ShellContextSnapshot {
    func preservingPlatformFields(from authoritative: ShellContextSnapshot?) -> ShellContextSnapshot? {
        guard self != nil || authoritative != nil else { return nil }

        return ShellContextSnapshot(
            workingDirectoryName: self?.workingDirectoryName ?? authoritative?.workingDirectoryName,
            repositoryRoot: self?.repositoryRoot ?? authoritative?.repositoryRoot,
            gitBranch: self?.gitBranch ?? authoritative?.gitBranch,
            controlPath: self?.controlPath ?? authoritative?.controlPath,
            socketPath: self?.socketPath ?? authoritative?.socketPath,
            alanBindingFile: self?.alanBindingFile ?? authoritative?.alanBindingFile,
            launchCommand: self?.launchCommand ?? authoritative?.launchCommand,
            launchStrategy: self?.launchStrategy ?? authoritative?.launchStrategy,
            terminalProfileState: self?.terminalProfileState ?? authoritative?.terminalProfileState,
            terminalProfileRequestedID: self?.terminalProfileRequestedID
                ?? authoritative?.terminalProfileRequestedID,
            terminalProfileID: self?.terminalProfileID ?? authoritative?.terminalProfileID,
            terminalProfileKind: self?.terminalProfileKind ?? authoritative?.terminalProfileKind,
            terminalProfileTitle: self?.terminalProfileTitle ?? authoritative?.terminalProfileTitle,
            shellIntegrationSource: self?.shellIntegrationSource
                ?? authoritative?.shellIntegrationSource,
            processState: self?.processState ?? authoritative?.processState,
            rendererPhase: self?.rendererPhase ?? authoritative?.rendererPhase,
            rendererHealth: self?.rendererHealth ?? authoritative?.rendererHealth,
            surfaceReadiness: self?.surfaceReadiness ?? authoritative?.surfaceReadiness,
            inputReady: self?.inputReady ?? authoritative?.inputReady,
            readonly: self?.readonly ?? authoritative?.readonly,
            terminalMode: self?.terminalMode ?? authoritative?.terminalMode,
            displayName: self?.displayName ?? authoritative?.displayName,
            displayID: self?.displayID ?? authoritative?.displayID,
            windowTitle: self?.windowTitle ?? authoritative?.windowTitle,
            lastMetadataAt: self?.lastMetadataAt ?? authoritative?.lastMetadataAt,
            lastCommandExitCode: self?.lastCommandExitCode ?? authoritative?.lastCommandExitCode
        )
    }
}

private extension Optional where Wrapped == ShellViewportSnapshot {
    func preservingPlatformFields(from authoritative: ShellViewportSnapshot?) -> ShellViewportSnapshot? {
        guard self != nil || authoritative != nil else { return nil }

        return ShellViewportSnapshot(
            title: self?.title ?? authoritative?.title,
            summary: self?.summary ?? authoritative?.summary,
            visibleExcerpt: self?.visibleExcerpt ?? authoritative?.visibleExcerpt,
            lastActivityAt: self?.lastActivityAt ?? authoritative?.lastActivityAt
        )
    }
}

private struct RawJSONValue: Decodable {
    let value: Any

    init(from decoder: Decoder) throws {
        if let container = try? decoder.singleValueContainer() {
            if container.decodeNil() {
                value = NSNull()
                return
            }
            if let bool = try? container.decode(Bool.self) {
                value = bool
                return
            }
            if let int = try? container.decode(Int.self) {
                value = int
                return
            }
            if let double = try? container.decode(Double.self) {
                value = double
                return
            }
            if let string = try? container.decode(String.self) {
                value = string
                return
            }
        }
        if var array = try? decoder.unkeyedContainer() {
            var values: [Any] = []
            while !array.isAtEnd {
                values.append(try array.decode(RawJSONValue.self).value)
            }
            value = values
            return
        }
        let object = try decoder.container(keyedBy: DynamicCodingKey.self)
        var values: [String: Any] = [:]
        for key in object.allKeys {
            values[key.stringValue] = try object.decode(RawJSONValue.self, forKey: key).value
        }
        value = values
    }
}

private struct DynamicCodingKey: CodingKey {
    let stringValue: String
    let intValue: Int?

    init?(stringValue: String) {
        self.stringValue = stringValue
        intValue = nil
    }

    init?(intValue: Int) {
        stringValue = String(intValue)
        self.intValue = intValue
    }
}
