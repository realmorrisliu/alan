#if os(macOS)
import AppKit
import Foundation

enum ShellLocalFolderOpener {
    static func canOpenFolder(displayPath: String?) -> Bool {
        folderURL(displayPath: displayPath) != nil
    }

    @MainActor
    static func openFolder(displayPath: String?) {
        guard let url = folderURL(displayPath: displayPath) else {
            return
        }
        NSWorkspace.shared.open(url)
    }

    private static func folderURL(displayPath: String?) -> URL? {
        guard let displayPath = displayPath?.trimmingCharacters(in: .whitespacesAndNewlines),
              !displayPath.isEmpty
        else {
            return nil
        }

        let expandedPath = NSString(string: displayPath).expandingTildeInPath
        guard expandedPath.hasPrefix("/") else {
            return nil
        }

        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: expandedPath, isDirectory: &isDirectory),
              isDirectory.boolValue
        else {
            return nil
        }
        return URL(fileURLWithPath: expandedPath, isDirectory: true)
    }
}
#endif
