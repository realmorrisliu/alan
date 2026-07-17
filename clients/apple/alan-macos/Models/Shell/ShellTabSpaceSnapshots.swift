import Foundation

struct ShellTab: Identifiable, Codable, Equatable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: ShellPaneTreeNode
    let isPinned: Bool
    let isTitleUserLocked: Bool

    var id: String { tabID }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
    }

    init(
        tabID: String,
        kind: ShellTabKind,
        title: String?,
        paneTree: ShellPaneTreeNode,
        isPinned: Bool = false,
        isTitleUserLocked: Bool = false
    ) {
        self.tabID = tabID
        self.kind = kind
        self.title = title
        self.paneTree = paneTree
        self.isPinned = isPinned
        self.isTitleUserLocked = isTitleUserLocked
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            tabID: try container.decode(String.self, forKey: .tabID),
            kind: try container.decode(ShellTabKind.self, forKey: .kind),
            title: try container.decodeIfPresent(String.self, forKey: .title),
            paneTree: try container.decode(ShellPaneTreeNode.self, forKey: .paneTree),
            isPinned: try container.decodeIfPresent(Bool.self, forKey: .isPinned) ?? false,
            isTitleUserLocked: try container.decodeIfPresent(Bool.self, forKey: .isTitleUserLocked) ?? false
        )
    }
}

struct ShellContentTab: Identifiable, Codable, Equatable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: ShellPaneSlotTreeNode
    let isPinned: Bool
    let isTitleUserLocked: Bool

    var id: String { tabID }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
    }

    init(
        tabID: String,
        kind: ShellTabKind,
        title: String?,
        paneTree: ShellPaneSlotTreeNode,
        isPinned: Bool = false,
        isTitleUserLocked: Bool = false
    ) {
        self.tabID = tabID
        self.kind = kind
        self.title = title
        self.paneTree = paneTree
        self.isPinned = isPinned
        self.isTitleUserLocked = isTitleUserLocked
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            tabID: try container.decode(String.self, forKey: .tabID),
            kind: try container.decode(ShellTabKind.self, forKey: .kind),
            title: try container.decodeIfPresent(String.self, forKey: .title),
            paneTree: try container.decode(ShellPaneSlotTreeNode.self, forKey: .paneTree),
            isPinned: try container.decodeIfPresent(Bool.self, forKey: .isPinned) ?? false,
            isTitleUserLocked: try container.decodeIfPresent(Bool.self, forKey: .isTitleUserLocked) ?? false
        )
    }
}

enum ShellSpacePresentationIcon {
    static let defaultSystemName = "square.grid.2x2"

    // MARK: - Symbol support

    static func resolvedSystemName(_ systemName: String?) -> String {
        guard let systemName,
              isSupportedSystemName(systemName)
        else {
            return defaultSystemName
        }
        return systemName.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func isSupportedSystemName(_ systemName: String?) -> Bool {
        guard let systemName else { return false }
        let trimmed = systemName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              trimmed.rangeOfCharacter(from: .whitespacesAndNewlines) == nil
        else {
            return false
        }
        return trimmed.unicodeScalars.allSatisfy { scalar in
            CharacterSet.alphanumerics.contains(scalar)
                || scalar == "."
                || scalar == "-"
                || scalar == "_"
        }
    }

    // MARK: - Monogram derivation

    /// Derives a single-grapheme monogram from a Space title.
    ///
    /// - If the first grapheme cluster is a Latin letter, it is returned uppercased.
    /// - For non-Latin scripts (CJK, emoji, other), the first grapheme is returned as-is.
    /// - Returns an empty string when `title` is nil or blank (signals caller to use fallback symbol).
    static func monogram(forTitle title: String?) -> String {
        guard let title else { return "" }
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let firstChar = trimmed.first else { return "" }
        // Check whether the character is a Latin letter (ASCII + isLetter).
        if firstChar.isLetter && firstChar.isASCII {
            return String(firstChar).uppercased()
        }
        // For non-Latin letters (CJK, emoji, extended scripts), return the grapheme cluster as-is.
        return String(firstChar)
    }

    // MARK: - Resolution policy

    /// The result of resolving a Space's icon — symbol, monogram, or fallback symbol.
    enum Resolved: Equatable {
        /// A supported SF Symbol name (user override).
        case symbol(String)
        /// A single-grapheme monogram derived from the Space title (auto-default).
        case monogram(String)
        /// The neutral fallback symbol used when no title monogram is available.
        case fallbackSymbol(String)
    }

    /// Resolves the display icon for a Space given an optional stored symbol name and title.
    ///
    /// Resolution order:
    /// 1. If `systemName` is present and passes `isSupportedSystemName` → `.symbol`.
    /// 2. Else if a monogram can be derived from `title` → `.monogram`.
    /// 3. Else → `.fallbackSymbol(defaultSystemName)`.
    static func resolve(systemName: String?, title: String?) -> Resolved {
        if let name = systemName, isSupportedSystemName(name) {
            return .symbol(name.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        let m = monogram(forTitle: title)
        if !m.isEmpty {
            return .monogram(m)
        }
        return .fallbackSymbol(defaultSystemName)
    }
}

/// Derives a human default Space name from a working directory. Pure; the
/// "Space N" index fallback stays in `creatingSpace` and is used when this
/// returns "".
enum ShellSpaceDefaultName {
    static func derive(fromWorkingDirectory path: String?) -> String {
        guard let path else { return "" }
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }

        var components = trimmed.split(separator: "/", omittingEmptySubsequences: true)
            .map(String.init)
        if components.last == ".git" {
            components.removeLast()
        }
        return components.last ?? ""
    }
}

struct ShellSpace: Identifiable, Codable, Equatable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [ShellTab]
    let selectedTabID: String?
    let terminalProfileID: String?
    let presentationIconSystemName: String?

    var id: String { spaceID }

    var resolvedPresentationIconSystemName: String {
        ShellSpacePresentationIcon.resolvedSystemName(presentationIconSystemName)
    }

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case selectedTabID = "selected_tab_id"
        case terminalProfileID = "terminal_profile_id"
        case presentationIconSystemName = "presentation_icon"
    }

    init(
        spaceID: String,
        title: String,
        attention: ShellAttentionState,
        tabs: [ShellTab],
        selectedTabID: String? = nil,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil
    ) {
        self.spaceID = spaceID
        self.title = title
        self.attention = attention
        self.tabs = tabs
        self.selectedTabID = selectedTabID
        self.terminalProfileID = terminalProfileID
        self.presentationIconSystemName = presentationIconSystemName
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            spaceID: try container.decode(String.self, forKey: .spaceID),
            title: try container.decode(String.self, forKey: .title),
            attention: try container.decode(ShellAttentionState.self, forKey: .attention),
            tabs: try container.decode([ShellTab].self, forKey: .tabs),
            selectedTabID: try container.decodeIfPresent(String.self, forKey: .selectedTabID),
            terminalProfileID: try container.decodeIfPresent(String.self, forKey: .terminalProfileID),
            presentationIconSystemName: try container.decodeIfPresent(
                String.self,
                forKey: .presentationIconSystemName
            )
        )
    }
}

struct ShellContentSpace: Identifiable, Codable, Equatable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [ShellContentTab]
    let selectedTabID: String?
    let terminalProfileID: String?
    let presentationIconSystemName: String?

    var id: String { spaceID }

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case selectedTabID = "selected_tab_id"
        case terminalProfileID = "terminal_profile_id"
        case presentationIconSystemName = "presentation_icon"
    }

    init(
        spaceID: String,
        title: String,
        attention: ShellAttentionState,
        tabs: [ShellContentTab],
        selectedTabID: String? = nil,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil
    ) {
        self.spaceID = spaceID
        self.title = title
        self.attention = attention
        self.tabs = tabs
        self.selectedTabID = selectedTabID
        self.terminalProfileID = terminalProfileID
        self.presentationIconSystemName = presentationIconSystemName
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            spaceID: try container.decode(String.self, forKey: .spaceID),
            title: try container.decode(String.self, forKey: .title),
            attention: try container.decode(ShellAttentionState.self, forKey: .attention),
            tabs: try container.decode([ShellContentTab].self, forKey: .tabs),
            selectedTabID: try container.decodeIfPresent(String.self, forKey: .selectedTabID),
            terminalProfileID: try container.decodeIfPresent(String.self, forKey: .terminalProfileID),
            presentationIconSystemName: try container.decodeIfPresent(
                String.self,
                forKey: .presentationIconSystemName
            )
        )
    }
}
