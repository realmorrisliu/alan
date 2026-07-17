import Foundation

struct ShellSidebarTabDragSource: Codable, Equatable {
    let tabID: String
    let sourceSpaceID: String
    let sourceSection: ShellTabOrganizationSection
    let sourceIndex: Int

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case sourceSpaceID = "source_space_id"
        case sourceSection = "source_section"
        case sourceIndex = "source_index"
    }

    func encodedPlainTextPayload() throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(self)
        return "alan.sidebar-tab-drag:" + data.base64EncodedString()
    }

    static func decodedPlainTextPayload(_ payload: String) throws -> ShellSidebarTabDragSource {
        let prefix = "alan.sidebar-tab-drag:"
        guard payload.hasPrefix(prefix),
              let data = Data(base64Encoded: String(payload.dropFirst(prefix.count)))
        else {
            throw ShellSidebarTabDragPayloadError.invalidPayload
        }
        return try JSONDecoder().decode(ShellSidebarTabDragSource.self, from: data)
    }
}
enum ShellSidebarTabDragPayloadError: Error {
    case invalidPayload
}

struct ShellSidebarTabInsertionTarget: Equatable {
    let spaceID: String
    let section: ShellTabOrganizationSection
    let index: Int
}

enum ShellSidebarTabDropModel {
    static func mutationIndex(
        for insertionTarget: ShellSidebarTabInsertionTarget,
        source: ShellSidebarTabDragSource
    ) -> Int {
        guard source.sourceSpaceID == insertionTarget.spaceID,
              source.sourceSection == insertionTarget.section,
              insertionTarget.index > source.sourceIndex
        else {
            return insertionTarget.index
        }

        return insertionTarget.index - 1
    }
}
