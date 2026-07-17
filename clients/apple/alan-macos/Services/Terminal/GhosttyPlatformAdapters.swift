import Foundation

#if os(macOS) && canImport(GhosttyKit)
import AppKit
import GhosttyKit

enum AlanGhosttyKeyCode {
    static let d: UInt32 = 2
    static let c: UInt32 = 8
    static let returnKey: UInt32 = 36
    static let keypadEnter: UInt32 = 76
}

enum AlanGhosttyClipboard {
    static func readText(location: ghostty_clipboard_e) -> String? {
        let pasteboard: NSPasteboard?
        switch location {
        case GHOSTTY_CLIPBOARD_STANDARD:
            pasteboard = .general
        default:
            pasteboard = nil
        }

        return pasteboard?.string(forType: .string)
    }

    static func write(
        location: ghostty_clipboard_e,
        content: UnsafePointer<ghostty_clipboard_content_s>?,
        len: Int
    ) {
        guard location == GHOSTTY_CLIPBOARD_STANDARD,
              let content,
              len > 0
        else { return }

        let buffer = UnsafeBufferPointer(start: content, count: len)
        guard let first = buffer.first,
              let data = first.data
        else { return }

        let text = String(cString: data)
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }
}

final class AlanGhosttyAppFocusObserver {
    private var observers: [NSObjectProtocol] = []

    func install(for app: ghostty_app_t) {
        remove()
        let center = NotificationCenter.default
        observers = [
            center.addObserver(
                forName: NSApplication.didBecomeActiveNotification,
                object: nil,
                queue: .main
            ) { _ in
                ghostty_app_set_focus(app, true)
            },
            center.addObserver(
                forName: NSApplication.didResignActiveNotification,
                object: nil,
                queue: .main
            ) { _ in
                ghostty_app_set_focus(app, false)
            },
        ]
    }

    func remove() {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers.removeAll()
    }

    deinit {
        remove()
    }
}

final class AlanGhosttyCanvasView: NSView {
    override var mouseDownCanMoveWindow: Bool { false }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }
}

extension NSScreen {
    var alanGhosttyDisplayID: UInt32? {
        (deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber).map {
            $0.uint32Value
        }
    }
}
#endif
