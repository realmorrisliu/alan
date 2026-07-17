#if os(macOS)
import AppKit

extension AlanTerminalHostNSView {
    func traceTerminalInput(
        _ eventName: String,
        event: NSEvent? = nil,
        details: @autoclosure () -> String = ""
    ) {
        guard Self.inputTrace.isEnabled else { return }
        let paneID = pane?.paneID ?? "nil"
        let firstResponder = traceResponderName(window?.firstResponder)
        let windowKey = window?.isKeyWindow == true
        let timestamp = event.map { String(format: "%.6f", $0.timestamp) } ?? "nil"
        let point = event.map { tracePoint(localPoint(for: $0)) } ?? "nil"
        let detailText = details()
        let suffix = detailText.isEmpty ? "" : " \(detailText)"
        Self.inputTrace.log(
            "\(eventName) pane=\(paneID) selected=\(isSelected) firstResponderSelf=\(isFocused) active=\(terminalInputIsActive) appActive=\(NSApp.isActive) windowKey=\(windowKey) firstResponder=\(firstResponder) ts=\(timestamp) point=\(point)\(suffix)"
        )
    }

    func terminalInputTraceStart() -> DispatchTime? {
        Self.inputTrace.isEnabled ? DispatchTime.now() : nil
    }

    func traceTerminalInputDuration(
        _ eventName: String,
        event: NSEvent? = nil,
        startedAt: DispatchTime?,
        details: @autoclosure () -> String = ""
    ) {
        guard let startedAt else { return }
        let now = DispatchTime.now()
        let nanos = now.uptimeNanoseconds >= startedAt.uptimeNanoseconds
            ? now.uptimeNanoseconds - startedAt.uptimeNanoseconds
            : 0
        let elapsedMs = Double(nanos) / 1_000_000
        let detailText = details()
        let suffix = detailText.isEmpty ? "" : " \(detailText)"
        traceTerminalInput(
            eventName,
            event: event,
            details: "elapsed_ms=\(String(format: "%.3f", elapsedMs))\(suffix)"
        )
    }

    func tracePointerInput(
        _ eventName: String,
        input: AlanTerminalPointerInput,
        decision: AlanTerminalPointerRoutingDecision,
        handled: Bool
    ) {
        guard Self.inputTrace.isEnabled else { return }
        switch input.phase {
        case .buttonDown, .buttonUp, .drag:
            break
        case .entered, .moved, .exited, .pressure:
            return
        }

        let paneID = pane?.paneID ?? "nil"
        Self.inputTrace.log(
            "\(eventName) pane=\(paneID) selected=\(isSelected) firstResponderSelf=\(isFocused) active=\(terminalInputIsActive) phase=\(input.phase) button=\(input.normalizedButton.map { String(describing: $0) } ?? "nil") x=\(String(format: "%.1f", input.x)) y=\(String(format: "%.1f", input.y)) decision=\(decision) handled=\(handled)"
        )
    }

    private func tracePoint(_ point: CGPoint) -> String {
        "(\(String(format: "%.1f", point.x)),\(String(format: "%.1f", point.y)))"
    }

    private func traceResponderName(_ responder: NSResponder?) -> String {
        guard let responder else { return "nil" }
        return String(describing: type(of: responder))
    }

    func traceViewName(_ view: NSView?) -> String {
        guard let view else { return "nil" }
        return String(describing: type(of: view))
    }
}
#endif
