#if os(macOS)
import AppKit

@MainActor
protocol ShellPasteboardAccessing: AnyObject {
    func readString() -> String?
    func writeString(_ value: String)
}

@MainActor
final class ShellSystemPasteboard: ShellPasteboardAccessing {
    private let pasteboard: NSPasteboard

    init(pasteboard: NSPasteboard = .general) {
        self.pasteboard = pasteboard
    }

    func readString() -> String? {
        pasteboard.string(forType: .string)
    }

    func writeString(_ value: String) {
        pasteboard.clearContents()
        pasteboard.setString(value, forType: .string)
    }
}
#endif
