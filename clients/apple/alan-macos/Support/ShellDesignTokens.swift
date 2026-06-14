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

/// Raw light/dark RGB values for the core paper and ink surfaces.
/// Kept as plain tuples so script tests can assert the Paper & Ink
/// luminance hierarchy (see docs/design/design-language.md).
enum ShellSurfaceValues {
    static let paperRootLight: (Double, Double, Double) = (1.0, 1.0, 1.0)
    static let paperRootDark: (Double, Double, Double) = (0.036, 0.040, 0.050)
    static let paperCanvasLight: (Double, Double, Double) = (0.94, 0.94, 0.965)
    static let paperCanvasDark: (Double, Double, Double) = (0.032, 0.036, 0.046)
    static let paperWindowLight: (Double, Double, Double) = (0.972, 0.973, 0.985)
    static let paperWindowDark: (Double, Double, Double) = (0.036, 0.040, 0.050)
    static let paperSidebarLight: (Double, Double, Double) = (0.902, 0.906, 0.940)
    static let paperSidebarDark: (Double, Double, Double) = (0.042, 0.047, 0.060)
    static let paperWorkspaceLight: (Double, Double, Double) = (0.979, 0.98, 0.989)
    static let paperWorkspaceDark: (Double, Double, Double) = (0.038, 0.042, 0.053)

    static let inkSurfaceLight: (Double, Double, Double) = (0.10, 0.12, 0.16)
    static let inkSurfaceDark: (Double, Double, Double) = (0.105, 0.118, 0.142)
    static let inkRaisedLight: (Double, Double, Double) = (0.16, 0.18, 0.24)
    static let inkRaisedDark: (Double, Double, Double) = (0.150, 0.168, 0.200)

    static var lightPaperSurfaces: [(String, (Double, Double, Double))] {
        [
            ("paperRoot", paperRootLight),
            ("paperCanvas", paperCanvasLight),
            ("paperWindow", paperWindowLight),
            ("paperSidebar", paperSidebarLight),
            ("paperWorkspace", paperWorkspaceLight),
        ]
    }

    static var darkPaperSurfaces: [(String, (Double, Double, Double))] {
        [
            ("paperRoot", paperRootDark),
            ("paperCanvas", paperCanvasDark),
            ("paperWindow", paperWindowDark),
            ("paperSidebar", paperSidebarDark),
            ("paperWorkspace", paperWorkspaceDark),
        ]
    }

    static func luminance(_ rgb: (Double, Double, Double)) -> Double {
        0.2126 * rgb.0 + 0.7152 * rgb.1 + 0.0722 * rgb.2
    }
}

/// Role-based type scale. Two tracks, integer sizes only; weights stay
/// per-context. See docs/design/design-language.md.
enum ShellType {
    static let display: CGFloat = 17
    static let heading: CGFloat = 13
    static let row: CGFloat = 12
    static let caption: CGFloat = 11
    static let monoLabel: CGFloat = 11
    static let monoCaption: CGFloat = 10

    static func pro(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight)
    }

    static func mono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }
}

/// Semantic 4pt spacing scale. New layout code uses these names instead of
/// raw numeric paddings.
enum ShellSpacing {
    static let hair: CGFloat = 2
    static let tight: CGFloat = 4
    static let control: CGFloat = 8
    static let row: CGFloat = 12
    static let section: CGFloat = 16
    static let panel: CGFloat = 24
}

/// Paper domain: chrome surfaces. Calm, cool, low-saturation; always recedes
/// behind the ink surface.
enum ShellPaper {
    static let root = Color.shellAdaptive(
        light: ShellSurfaceValues.paperRootLight,
        dark: ShellSurfaceValues.paperRootDark
    )
    static let canvas = Color.shellAdaptive(
        light: ShellSurfaceValues.paperCanvasLight,
        dark: ShellSurfaceValues.paperCanvasDark
    )
    static let window = Color.shellAdaptive(
        light: ShellSurfaceValues.paperWindowLight,
        dark: ShellSurfaceValues.paperWindowDark
    )
    static let sidebar = Color.shellAdaptive(
        light: ShellSurfaceValues.paperSidebarLight,
        dark: ShellSurfaceValues.paperSidebarDark
    )
    static let workspace = Color.shellAdaptive(
        light: ShellSurfaceValues.paperWorkspaceLight,
        dark: ShellSurfaceValues.paperWorkspaceDark
    )
}

/// Ink domain: the terminal surface family and the well rim.
enum ShellInk {
    static let surface = Color.shellAdaptive(
        light: ShellSurfaceValues.inkSurfaceLight,
        dark: ShellSurfaceValues.inkSurfaceDark
    )
    static let raised = Color.shellAdaptive(
        light: ShellSurfaceValues.inkRaisedLight,
        dark: ShellSurfaceValues.inkRaisedDark
    )
    /// Top inner edge of the terminal surround ("the well rim").
    static let rimHighlight = Color.shellAdaptive(
        light: (1.0, 1.0, 1.0),
        lightAlpha: 0.18,
        dark: (1.0, 1.0, 1.0),
        darkAlpha: 0.07
    )
    /// Bottom outer contact line of the terminal surround.
    static let rimShadowLine = Color.shellAdaptive(
        light: (0.0, 0.0, 0.0),
        lightAlpha: 0.14,
        dark: (0.0, 0.0, 0.0),
        darkAlpha: 0.32
    )
}

/// Signal domain: meaning-bearing color. Governed by the signal semantics
/// table in docs/design/design-language.md — anything not listed there is
/// silent.
enum ShellSignal {
    /// The only orange: the user must act (input, approval, intervention).
    static let action = Color.shellAdaptive(
        light: (0.82, 0.55, 0.24),
        dark: (0.94, 0.68, 0.34)
    )
    /// Keyboard focus and scrub preview only; never a status color.
    static let focus = Color.shellAdaptive(
        light: (0.31, 0.39, 0.71),
        dark: (0.50, 0.60, 0.94)
    )
    static let focusSoft = Color.shellAdaptive(
        light: (0.90, 0.92, 0.98),
        dark: (0.18, 0.22, 0.34)
    )
    /// Reserved phase-2 agent-activity interface: maximum luminance delta a
    /// breathing surface may add over its resting value.
    static let breathLuminanceDelta: Double = 0.06
}

enum ShellPalette {
    // Deprecated aliases — use ShellPaper / ShellInk / ShellSignal in new
    // code. Views migrate per-file in follow-up changes.
    static let rootBacking = ShellPaper.root
    static let canvas = ShellPaper.canvas
    static let window = ShellPaper.window
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
    static let sidebar = ShellPaper.sidebar
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
        lightAlpha: 0.88,
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
    static let sidebarSpaceSliderTrack = Color.shellAdaptive(
        light: (0.770, 0.775, 0.805),
        lightAlpha: 0.54,
        dark: (0.185, 0.205, 0.245),
        darkAlpha: 0.74
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
    static let workspace = ShellPaper.workspace
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
    static let terminal = ShellInk.surface
    static let terminalSoft = ShellInk.raised
    static let accent = ShellSignal.focus
    static let accentSoft = ShellSignal.focusSoft
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
    static let sidebarDivider = Color.shellAdaptive(
        light: (0.50, 0.51, 0.60),
        lightAlpha: 0.52,
        dark: (0.42, 0.46, 0.54),
        darkAlpha: 0.60
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
        light: (0.745, 0.755, 0.845),
        lightAlpha: 0.50,
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
    static let attention = ShellSignal.action
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
    static let workspacePanel: CGFloat = 10
}

enum ShellSidebarMetrics {
    static let edgeInset: CGFloat = 8
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
    static let workspacePanelInset: CGFloat = 8

    static func workspacePanelInsets(expandedSidebarProgress: CGFloat) -> EdgeInsets {
        let progress = min(max(expandedSidebarProgress, 0), 1)
        return EdgeInsets(
            top: workspacePanelInset,
            leading: workspacePanelInset * (1 - progress),
            bottom: workspacePanelInset,
            trailing: workspacePanelInset
        )
    }

    static func workspacePanelInsets(hasExpandedSidebar: Bool) -> EdgeInsets {
        workspacePanelInsets(expandedSidebarProgress: hasExpandedSidebar ? 1 : 0)
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
    static let workspacePanel = ShellShadowStyle(
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
    static let workspacePanelRim = ShellShadowStyle(
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

// MARK: - Space icon catalog

/// Curated SF Symbol names available for Space icon overrides.
/// Source of truth: openspec/changes/add-macos-space-icon-identity/design.md
enum ShellSpaceIconCatalog {
    /// The ~24 workspace-relevant SF Symbols a user can assign to a Space.
    /// Every name must be a valid SF Symbol and pass
    /// `ShellSpacePresentationIcon.isSupportedSystemName`.
    static let curatedSymbols: [String] = [
        "terminal",
        "chevron.left.forwardslash.chevron.right",
        "hammer",
        "wrench.and.screwdriver",
        "ant",
        "flask",
        "cube.box",
        "shippingbox",
        "server.rack",
        "externaldrive",
        "doc.text",
        "book",
        "paintbrush",
        "paintpalette",
        "globe",
        "network",
        "lock",
        "key",
        "leaf",
        "bolt",
        "sparkles",
        "star",
        "flag",
        "folder",
    ]
}
#endif
