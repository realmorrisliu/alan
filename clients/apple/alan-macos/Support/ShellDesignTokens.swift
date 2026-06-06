import SwiftUI

#if os(macOS)
import AppKit

private extension Color {
    static func shellAdaptive(
        light: (Double, Double, Double),
        dark: (Double, Double, Double),
        alpha: Double = 1
    ) -> Color {
        shellAdaptive(light: light, lightAlpha: alpha, dark: dark, darkAlpha: alpha)
    }

    static func shellAdaptive(
        light: (Double, Double, Double),
        lightAlpha: Double,
        dark: (Double, Double, Double),
        darkAlpha: Double
    ) -> Color {
        Color(
            NSColor(name: nil) { appearance in
                let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                let rgb = isDark ? dark : light
                let alpha = isDark ? darkAlpha : lightAlpha
                return NSColor(red: rgb.0, green: rgb.1, blue: rgb.2, alpha: alpha)
            }
        )
    }
}

enum ShellPalette {
    static let rootBacking = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        dark: (0.055, 0.061, 0.074)
    )
    static let canvas = Color.shellAdaptive(
        light: (0.94, 0.94, 0.965),
        dark: (0.045, 0.050, 0.062)
    )
    static let window = Color.shellAdaptive(
        light: (0.972, 0.973, 0.985),
        dark: (0.055, 0.061, 0.074)
    )
    static let windowBackdropTint = Color.shellAdaptive(
        light: (0.755, 0.765, 0.850),
        lightAlpha: 0.44,
        dark: (0.040, 0.047, 0.062),
        darkAlpha: 0.78
    )
    static let sidebarInk = Color.shellAdaptive(
        light: (0.030, 0.060, 0.220),
        dark: (0.90, 0.92, 0.97)
    )
    static let sidebarMutedInk = Color.shellAdaptive(
        light: (0.430, 0.430, 0.540),
        dark: (0.65, 0.68, 0.76)
    )
    static let sidebar = Color.shellAdaptive(
        light: (0.922, 0.924, 0.953),
        dark: (0.071, 0.079, 0.096)
    )
    static let sidebarRail = Color.shellAdaptive(
        light: (0.902, 0.907, 0.941),
        dark: (0.083, 0.092, 0.112)
    )
    static let sidebarCard = Color.shellAdaptive(
        light: (0.98, 0.98, 0.995),
        lightAlpha: 1.0,
        dark: (0.172, 0.188, 0.224),
        darkAlpha: 0.92
    )
    static let sidebarSelection = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.24,
        dark: (0.205, 0.225, 0.270),
        darkAlpha: 0.72
    )
    static let sidebarHover = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.11,
        dark: (0.185, 0.205, 0.245),
        darkAlpha: 0.46
    )
    static let sidebarRowHover = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.13,
        dark: (0.185, 0.205, 0.245),
        darkAlpha: 0.40
    )
    static let sidebarRowSelected = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.78,
        dark: (0.215, 0.235, 0.282),
        darkAlpha: 0.78
    )
    static let sidebarControl = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.18,
        dark: (0.190, 0.210, 0.252),
        darkAlpha: 0.54
    )
    static let sidebarControlStrong = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.24,
        dark: (0.215, 0.235, 0.282),
        darkAlpha: 0.72
    )
    static let commandGlassTint = Color.shellAdaptive(
        light: (0.720, 0.730, 0.790),
        lightAlpha: 1.0,
        dark: (0.215, 0.235, 0.282),
        darkAlpha: 0.72
    )
    static let titlebarToolGlassTint = Color.shellAdaptive(
        light: (0.720, 0.730, 0.790),
        lightAlpha: 1.0,
        dark: (0.215, 0.235, 0.282),
        darkAlpha: 0.70
    )
    static let railBase = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.08,
        dark: (0.155, 0.172, 0.210),
        darkAlpha: 0.58
    )
    static let railHover = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.14,
        dark: (0.190, 0.210, 0.252),
        darkAlpha: 0.66
    )
    static let railSelection = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.30,
        dark: (0.235, 0.255, 0.310),
        darkAlpha: 0.78
    )
    static let workspace = Color.shellAdaptive(
        light: (0.979, 0.98, 0.989),
        dark: (0.050, 0.056, 0.070)
    )
    static let settingsPane = Color.shellAdaptive(
        light: (0.954, 0.957, 0.970),
        dark: (0.054, 0.061, 0.076)
    )
    static let settingsSheet = Color.shellAdaptive(
        light: (0.996, 0.997, 1.0),
        dark: (0.118, 0.132, 0.160)
    )
    static let settingsNavigationHover = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.32,
        dark: (0.150, 0.168, 0.205),
        darkAlpha: 0.48
    )
    static let settingsNavigationSelection = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.68,
        dark: (0.175, 0.192, 0.236),
        darkAlpha: 0.68
    )
    static let terminal = Color.shellAdaptive(
        light: (0.10, 0.12, 0.16),
        dark: (0.050, 0.061, 0.076)
    )
    static let terminalSoft = Color.shellAdaptive(
        light: (0.16, 0.18, 0.24),
        dark: (0.100, 0.116, 0.145)
    )
    static let accent = Color.shellAdaptive(
        light: (0.31, 0.39, 0.71),
        dark: (0.50, 0.60, 0.94)
    )
    static let accentSoft = Color.shellAdaptive(
        light: (0.90, 0.92, 0.98),
        dark: (0.18, 0.22, 0.34)
    )
    static let ink = Color.shellAdaptive(
        light: (0.16, 0.18, 0.24),
        dark: (0.90, 0.92, 0.96)
    )
    static let mutedInk = Color.shellAdaptive(
        light: (0.43, 0.45, 0.54),
        dark: (0.64, 0.68, 0.75)
    )
    static let settingsPrimaryInk = Color.shellAdaptive(
        light: (0.145, 0.165, 0.225),
        dark: (0.90, 0.92, 0.96)
    )
    static let settingsValueInk = Color.shellAdaptive(
        light: (0.300, 0.318, 0.390),
        dark: (0.74, 0.78, 0.85)
    )
    static let settingsSecondaryInk = Color.shellAdaptive(
        light: (0.455, 0.475, 0.565),
        dark: (0.60, 0.64, 0.72)
    )
    static let settingsTertiaryInk = Color.shellAdaptive(
        light: (0.505, 0.530, 0.630),
        dark: (0.52, 0.56, 0.64)
    )
    static let settingsDisabledInk = Color.shellAdaptive(
        light: (0.620, 0.640, 0.710),
        dark: (0.43, 0.47, 0.55)
    )
    static let line = Color.shellAdaptive(
        light: (0.82, 0.83, 0.89),
        dark: (0.255, 0.285, 0.345)
    )
    static let panel = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.74,
        dark: (0.135, 0.150, 0.180),
        darkAlpha: 0.78
    )
    static let panelSoft = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.60,
        dark: (0.125, 0.140, 0.170),
        darkAlpha: 0.64
    )
    static let overlayScrim = Color.shellAdaptive(
        light: (0.10, 0.11, 0.15),
        lightAlpha: 0.16,
        dark: (0.0, 0.0, 0.0),
        darkAlpha: 0.34
    )
    static let materialScrim = Color.shellAdaptive(
        light: (0.760, 0.770, 0.865),
        lightAlpha: 0.34,
        dark: (0.030, 0.037, 0.050),
        darkAlpha: 0.78
    )
    static let materialTopWash = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.08,
        dark: (0.130, 0.150, 0.205),
        darkAlpha: 0.18
    )
    static let materialBottomShade = Color.shellAdaptive(
        light: (0.60, 0.62, 0.76),
        lightAlpha: 0.06,
        dark: (0.012, 0.016, 0.024),
        darkAlpha: 0.34
    )
    static let attention = Color.shellAdaptive(
        light: (0.82, 0.55, 0.24),
        dark: (0.94, 0.68, 0.34)
    )
}

enum ShellRadii {
    static let micro: CGFloat = 2
    static let badge: CGFloat = 4
    static let control: CGFloat = 6
    static let row: CGFloat = 8
    static let surface: CGFloat = 10
    static let overlay: CGFloat = 12
    static let floatingSidebarPanel: CGFloat = 14
    static let titlebarTool: CGFloat = 9
    static let terminalSurface: CGFloat = 10
}

enum ShellSidebarMetrics {
    static let edgeInset: CGFloat = 12
    static let rowInset: CGFloat = 10
    static let iconColumnWidth: CGFloat = 16
    static let iconPointSize: CGFloat = 12
    static let trafficLightLeadingInset: CGFloat = 14
    static let trafficLightTopInset: CGFloat = 16
    static let trafficLightFallbackGroupWidth: CGFloat = 58
    static let trafficLightFallbackButtonHeight: CGFloat = 14
    static let titlebarToolWidth: CGFloat = 31
    static let titlebarToolHeight: CGFloat = 30
    static let titlebarToolGapAfterTrafficLights: CGFloat = 12
    static let titlebarToolSpacing: CGFloat = 6
    static let collapsedRevealEdgeWidth: CGFloat = 12
    static let commandLauncherGapBelowTrafficLights: CGFloat = 15
    static let commandLauncherHeight: CGFloat = 34
}

enum ShellWorkspaceMetrics {
    static let terminalSurfaceInset: CGFloat = 8

    static func terminalSurfaceInsets(expandedSidebarProgress: CGFloat) -> EdgeInsets {
        let progress = min(max(expandedSidebarProgress, 0), 1)
        return EdgeInsets(
            top: terminalSurfaceInset,
            leading: terminalSurfaceInset * (1 - progress),
            bottom: terminalSurfaceInset,
            trailing: terminalSurfaceInset
        )
    }

    static func terminalSurfaceInsets(hasExpandedSidebar: Bool) -> EdgeInsets {
        terminalSurfaceInsets(expandedSidebarProgress: hasExpandedSidebar ? 1 : 0)
    }
}

struct ShellShadowStyle {
    let color: Color
    let radius: CGFloat
    let x: CGFloat
    let y: CGFloat

    init(color: Color, radius: CGFloat, x: CGFloat = 0, y: CGFloat) {
        self.color = color
        self.radius = radius
        self.x = x
        self.y = y
    }
}

enum ShellShadows {
    static let none = ShellShadowStyle(color: .clear, radius: 0, y: 0)
    static let navigationSelection = ShellShadowStyle(
        color: Color.shellAdaptive(
            light: (0.18, 0.20, 0.28),
            lightAlpha: 0.11,
            dark: (0, 0, 0),
            darkAlpha: 0.26
        ),
        radius: 2.2,
        x: -0.2,
        y: 0.9
    )
    static let terminalSurface = ShellShadowStyle(
        color: Color.shellAdaptive(
            light: (0.18, 0.20, 0.28),
            lightAlpha: 0.22,
            dark: (0, 0, 0),
            darkAlpha: 0.34
        ),
        radius: 3,
        x: -0.7,
        y: 1.4
    )
    static let terminalSurfaceRim = ShellShadowStyle(
        color: Color.shellAdaptive(
            light: (0.18, 0.20, 0.28),
            lightAlpha: 0.12,
            dark: (0, 0, 0),
            darkAlpha: 0.28
        ),
        radius: 0.8,
        x: -0.5,
        y: 0.3
    )
    static let floatingInput = ShellShadowStyle(
        color: Color.shellAdaptive(
            light: (0.16, 0.17, 0.24),
            lightAlpha: 0.16,
            dark: (0, 0, 0),
            darkAlpha: 0.32
        ),
        radius: 5,
        x: -0.5,
        y: 2.2
    )
    static let floatingPanel = ShellShadowStyle(
        color: Color.shellAdaptive(
            light: (0.16, 0.17, 0.24),
            lightAlpha: 0.18,
            dark: (0, 0, 0),
            darkAlpha: 0.36
        ),
        radius: 10,
        x: -1,
        y: 5
    )
    static let commandPalette = ShellShadowStyle(
        color: Color.shellAdaptive(
            light: (0.16, 0.17, 0.24),
            lightAlpha: 0.20,
            dark: (0, 0, 0),
            darkAlpha: 0.40
        ),
        radius: 22,
        x: -1,
        y: 12
    )
    static let floatingOverlay = floatingPanel
    static let sidebarSelection = navigationSelection
    static let spaceSelection = navigationSelection
}

extension View {
    func shellShadow(_ style: ShellShadowStyle) -> some View {
        shadow(color: style.color, radius: style.radius, x: style.x, y: style.y)
    }
}

enum ShellAppearanceMode: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var label: String {
        switch self {
        case .system:
            return "System"
        case .light:
            return "Light"
        case .dark:
            return "Dark"
        }
    }

    var symbolName: String {
        switch self {
        case .system:
            return "circle.lefthalf.filled"
        case .light:
            return "sun.max"
        case .dark:
            return "moon"
        }
    }

    var next: ShellAppearanceMode {
        switch self {
        case .system:
            return .light
        case .light:
            return .dark
        case .dark:
            return .system
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system:
            return nil
        case .light:
            return .light
        case .dark:
            return .dark
        }
    }

    func resolvedColorScheme(systemColorScheme: ColorScheme) -> ColorScheme {
        colorScheme ?? systemColorScheme
    }

    static var currentSystemColorScheme: ColorScheme {
        colorScheme(for: NSApplication.shared.effectiveAppearance)
    }

    static func colorScheme(for appearance: NSAppearance) -> ColorScheme {
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? .dark : .light
    }

    var nsAppearanceName: NSAppearance.Name? {
        switch self {
        case .system:
            return nil
        case .light:
            return .aqua
        case .dark:
            return .darkAqua
        }
    }
}

private extension ColorScheme {
    var shellNSAppearance: NSAppearance? {
        switch self {
        case .light:
            return NSAppearance(named: .aqua)
        case .dark:
            return NSAppearance(named: .darkAqua)
        @unknown default:
            return nil
        }
    }
}

enum ShellMaterialRole {
    case windowBackdrop
    case sidebarGlass
    case workspaceBackdrop
    case terminalSurround
    case terminalChrome
    case terminalChromeSelected
    case floatingOverlay
    case floatingInput
    case controlGlass
    case controlGlassStrong
    case controlGlassHover
    case controlGlassSelected
    case panel
    case panelSoft

    var visualEffectMaterial: NSVisualEffectView.Material? {
        switch self {
        case .windowBackdrop:
            return .sidebar
        case .sidebarGlass:
            return .sidebar
        case .workspaceBackdrop:
            return .contentBackground
        case .floatingOverlay, .floatingInput:
            return .popover
        case .terminalSurround,
             .terminalChrome,
             .terminalChromeSelected,
             .controlGlass,
             .controlGlassStrong,
             .controlGlassHover,
             .controlGlassSelected,
             .panel,
             .panelSoft:
            return nil
        }
    }

    var blendingMode: NSVisualEffectView.BlendingMode {
        switch self {
        case .windowBackdrop, .sidebarGlass:
            return .behindWindow
        case .workspaceBackdrop,
             .floatingOverlay,
             .floatingInput,
             .terminalSurround,
             .terminalChrome,
             .terminalChromeSelected,
             .controlGlass,
             .controlGlassStrong,
             .controlGlassHover,
             .controlGlassSelected,
             .panel,
             .panelSoft:
            return .withinWindow
        }
    }

    var fill: Color {
        switch self {
        case .windowBackdrop:
            return ShellPalette.windowBackdropTint
        case .sidebarGlass:
            return ShellPalette.materialScrim
        case .workspaceBackdrop:
            return ShellPalette.workspace.opacity(0.74)
        case .terminalSurround:
            return ShellPalette.terminal
        case .terminalChrome:
            return ShellPalette.terminalSoft.opacity(0.34)
        case .terminalChromeSelected:
            return ShellPalette.terminalSoft.opacity(0.52)
        case .floatingOverlay:
            return ShellPalette.window.opacity(0.86)
        case .floatingInput:
            return ShellPalette.panel.opacity(0.92)
        case .controlGlass:
            return ShellPalette.sidebarControl
        case .controlGlassStrong:
            return ShellPalette.sidebarControlStrong
        case .controlGlassHover:
            return ShellPalette.sidebarHover
        case .controlGlassSelected:
            return ShellPalette.sidebarSelection
        case .panel:
            return ShellPalette.panel
        case .panelSoft:
            return ShellPalette.panelSoft
        }
    }

    var stroke: Color {
        switch self {
        case .terminalSurround:
            return ShellPalette.line.opacity(0.18)
        case .floatingOverlay:
            return ShellPalette.line.opacity(0.42)
        case .floatingInput:
            return ShellPalette.line.opacity(0.32)
        case .controlGlass,
             .controlGlassStrong,
             .controlGlassHover,
             .controlGlassSelected:
            return ShellPalette.line.opacity(0.18)
        case .terminalChrome, .terminalChromeSelected:
            return ShellPalette.line.opacity(0.16)
        case .panel, .panelSoft:
            return ShellPalette.line.opacity(0.22)
        case .windowBackdrop, .sidebarGlass, .workspaceBackdrop:
            return ShellPalette.line.opacity(0.0)
        }
    }

    var gradientOverlay: LinearGradient? {
        switch self {
        case .windowBackdrop, .sidebarGlass, .workspaceBackdrop:
            return LinearGradient(
                colors: [
                    ShellPalette.materialTopWash,
                    ShellPalette.materialBottomShade,
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        case .floatingOverlay, .floatingInput:
            return LinearGradient(
                colors: [
                    Color.white.opacity(0.10),
                    ShellPalette.materialBottomShade,
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        case .terminalSurround,
             .terminalChrome,
             .terminalChromeSelected,
             .controlGlass,
             .controlGlassStrong,
             .controlGlassHover,
             .controlGlassSelected,
             .panel,
             .panelSoft:
            return nil
        }
    }

    func resolvedFill(reduceTransparency: Bool, increasedContrast: Bool) -> Color {
        if reduceTransparency {
            switch self {
            case .windowBackdrop, .sidebarGlass, .workspaceBackdrop:
                return ShellPalette.window
            case .floatingOverlay:
                return ShellPalette.window.opacity(increasedContrast ? 0.98 : 0.94)
            case .floatingInput, .panel, .panelSoft:
                return ShellPalette.panel
            case .controlGlass, .controlGlassHover:
                return increasedContrast ? ShellPalette.sidebarControlStrong : ShellPalette.panelSoft
            case .controlGlassStrong, .controlGlassSelected:
                return ShellPalette.sidebarControlStrong
            case .terminalSurround:
                return ShellPalette.terminal
            case .terminalChrome:
                return ShellPalette.terminalSoft.opacity(increasedContrast ? 0.50 : 0.40)
            case .terminalChromeSelected:
                return ShellPalette.terminalSoft.opacity(increasedContrast ? 0.68 : 0.56)
            }
        }

        if increasedContrast {
            switch self {
            case .floatingOverlay:
                return ShellPalette.window.opacity(0.94)
            case .floatingInput, .panelSoft:
                return ShellPalette.panel
            case .controlGlass, .controlGlassHover:
                return ShellPalette.sidebarControlStrong
            case .controlGlassSelected:
                return ShellPalette.sidebarControlStrong
            case .terminalChrome:
                return ShellPalette.terminalSoft.opacity(0.48)
            case .terminalChromeSelected:
                return ShellPalette.terminalSoft.opacity(0.66)
            case .windowBackdrop,
                 .sidebarGlass,
                 .workspaceBackdrop,
                 .terminalSurround,
                 .controlGlassStrong,
                 .panel:
                break
            }
        }

        return fill
    }

    func resolvedStroke(increasedContrast: Bool) -> Color {
        guard increasedContrast else {
            return stroke
        }

        switch self {
        case .windowBackdrop, .sidebarGlass, .workspaceBackdrop:
            return ShellPalette.line.opacity(0.0)
        case .terminalSurround,
             .terminalChrome,
             .terminalChromeSelected,
             .controlGlass,
             .controlGlassStrong,
             .controlGlassHover,
             .controlGlassSelected,
             .panel,
             .panelSoft:
            return ShellPalette.line.opacity(0.34)
        case .floatingOverlay, .floatingInput:
            return ShellPalette.line.opacity(0.52)
        }
    }
}

private struct ShellVisualEffectView: NSViewRepresentable {
    let role: ShellMaterialRole
    @Environment(\.colorScheme) private var colorScheme

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        applyConfiguration(to: view)
        view.state = .followsWindowActiveState
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {
        applyConfiguration(to: nsView)
    }

    private func applyConfiguration(to nsView: NSVisualEffectView) {
        nsView.material = role.visualEffectMaterial ?? .contentBackground
        nsView.blendingMode = role.blendingMode
        nsView.appearance = colorScheme.shellNSAppearance
        nsView.needsDisplay = true
    }
}

struct ShellMaterialBackgroundView: View {
    let role: ShellMaterialRole
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    init(_ role: ShellMaterialRole = .sidebarGlass) {
        self.role = role
    }

    var body: some View {
        ZStack {
            if role.visualEffectMaterial != nil && !reduceTransparency {
                ShellVisualEffectView(role: role)
            }
            role.resolvedFill(
                reduceTransparency: reduceTransparency,
                increasedContrast: NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
            )
            if !reduceTransparency, let gradient = role.gradientOverlay {
                gradient
            }
        }
    }
}

struct ShellMaterialTintView: View {
    let role: ShellMaterialRole
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    init(_ role: ShellMaterialRole) {
        self.role = role
    }

    var body: some View {
        ZStack {
            role.resolvedFill(
                reduceTransparency: reduceTransparency,
                increasedContrast: NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
            )
            if !reduceTransparency, let gradient = role.gradientOverlay {
                gradient
            }
        }
    }
}

struct ShellMaterialShape<MaterialShape: InsettableShape>: View {
    let role: ShellMaterialRole
    let shape: MaterialShape
    var showsStroke = false
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        let increasedContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast

        shape
            .fill(
                role.resolvedFill(
                    reduceTransparency: reduceTransparency,
                    increasedContrast: increasedContrast
                )
            )
            .overlay {
                if showsStroke || increasedContrast {
                    shape.stroke(role.resolvedStroke(increasedContrast: increasedContrast), lineWidth: 1)
                }
            }
    }
}

struct ShellLiquidGlassSurface<SurfaceShape: InsettableShape>: View {
    let shape: SurfaceShape
    var tint = ShellPalette.sidebarControl
    var tintOpacity: Double = 0.16
    var strokeOpacity: Double = 0.20
    var usesSystemGlassInLightMode = false
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        let increasedContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast

        if usesSystemGlassEffect(increasedContrast: increasedContrast) {
            baseFill(increasedContrast: increasedContrast)
                .glassEffect(.regular.interactive(), in: shape)
        } else {
            baseFill(increasedContrast: increasedContrast)
        }
    }

    private func usesSystemGlassEffect(increasedContrast: Bool) -> Bool {
        guard !reduceTransparency, !increasedContrast else { return false }
        return colorScheme == .dark || usesSystemGlassInLightMode
    }

    @ViewBuilder
    private func baseFill(increasedContrast: Bool) -> some View {
        if colorScheme == .light {
            lightInsetFill(increasedContrast: increasedContrast)
        } else {
            darkGlassFill(increasedContrast: increasedContrast)
        }
    }

    private func lightInsetFill(increasedContrast: Bool) -> some View {
        let effectiveTintOpacity = increasedContrast ? max(tintOpacity, 0.26) : tintOpacity
        let effectiveStrokeOpacity = increasedContrast ? 0.44 : max(strokeOpacity, 0.12)

        return ZStack {
            shape
                .fill(
                    tint.opacity(
                        increasedContrast ? max(effectiveTintOpacity, 0.34) : effectiveTintOpacity
                    )
                )

            shape
                .fill(
                    LinearGradient(
                        colors: [
                            Color.white.opacity(increasedContrast ? 0.18 : 0.13),
                            Color.white.opacity(0.02),
                            ShellPalette.sidebarInk.opacity(increasedContrast ? 0.018 : 0.012),
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )

            shape
                .strokeBorder(
                    ShellPalette.line.opacity(increasedContrast ? 0.50 : effectiveStrokeOpacity),
                    lineWidth: 0.55
                )

            shape
                .strokeBorder(Color.white.opacity(increasedContrast ? 0.22 : 0.16), lineWidth: 0.45)
                .mask {
                    shape.fill(
                        LinearGradient(
                            colors: [
                                Color.white,
                                Color.white.opacity(0),
                            ],
                            startPoint: .top,
                            endPoint: .center
                        )
                    )
                }
        }
    }

    private func darkGlassFill(increasedContrast: Bool) -> some View {
        return ZStack {
            shape
                .fill(
                    tint.opacity(
                        increasedContrast ? max(tintOpacity, 0.34) : tintOpacity
                    )
                )

            shape
                .fill(
                    LinearGradient(
                        colors: [
                            Color.white.opacity(0.045),
                            Color.white.opacity(0),
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )

            shape
                .strokeBorder(
                    ShellPalette.line.opacity(increasedContrast ? 0.50 : strokeOpacity),
                    lineWidth: 0.7
                )
        }
    }
}
#endif
