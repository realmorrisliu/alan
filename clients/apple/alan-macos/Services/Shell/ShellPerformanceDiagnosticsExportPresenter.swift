import Foundation

#if os(macOS)
import AppKit

@MainActor
enum AlanPerformanceDiagnosticsExportPresenter {
    static func exportRecentDiagnostics(installChannel: String) -> URL? {
        let panel = NSOpenPanel()
        panel.title = "Export Recent Diagnostics"
        panel.prompt = "Export"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false

        guard panel.runModal() == .OK,
              let directory = panel.url
        else {
            return nil
        }

        do {
            return try AlanPerformanceDiagnosticsController.shared.exportRecentDiagnostics(
                to: directory,
                appVersion: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString")
                    as? String ?? "unknown",
                installChannel: installChannel
            )
        } catch {
            presentExportFailure(error)
            return nil
        }
    }

    private static func presentExportFailure(_ error: Error) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "Diagnostics export failed."
        alert.informativeText = error.localizedDescription
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }
}
#endif
