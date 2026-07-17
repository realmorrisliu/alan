#if os(macOS)
import AppKit

@MainActor
enum ShellAppActivityProvider {
    static var isActive: Bool {
        NSApp.isActive
    }
}
#endif
