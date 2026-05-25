import SwiftUI

#if os(macOS)
import AppKit
import Sparkle

@MainActor
final class AlanMacUpdateController: NSObject, ObservableObject {
    private static let errorDomain = "app.alanworks.macos.update"

    @Published private(set) var decision: AlanMacUpdateDecision
    private var standardUpdaterController: SPUStandardUpdaterController?

    var menuTitle: String {
        decision.menuTitle
    }

    override init() {
        let resolvedDecision = AlanMacUpdatePolicy.decision()
        decision = resolvedDecision
        super.init()

        if resolvedDecision.allowsSparkleUpdates {
            standardUpdaterController = SPUStandardUpdaterController(
                startingUpdater: true,
                updaterDelegate: self,
                userDriverDelegate: nil
            )
        }
    }

    func checkForUpdates() {
        decision = AlanMacUpdatePolicy.decision()

        guard decision.allowsSparkleUpdates, let standardUpdaterController else {
            showUnsupportedUpdateAlert()
            return
        }

        standardUpdaterController.checkForUpdates(nil)
    }

    private func showUnsupportedUpdateAlert() {
        let alert = NSAlert()
        alert.messageText = "Updates are managed outside Sparkle"
        alert.informativeText = decision.userMessage
        alert.alertStyle = .informational
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }
}

extension AlanMacUpdateController: SPUUpdaterDelegate {
    func updater(_ updater: SPUUpdater, mayPerform updateCheck: SPUUpdateCheck) throws {
        let currentDecision = AlanMacUpdatePolicy.decision()
        guard currentDecision.allowsSparkleUpdates else {
            throw NSError(
                domain: Self.errorDomain,
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: currentDecision.userMessage]
            )
        }
    }
}
#endif
