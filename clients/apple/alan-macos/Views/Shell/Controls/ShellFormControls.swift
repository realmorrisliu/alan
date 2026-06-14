import SwiftUI

#if os(macOS)

// MARK: - Metrics

/// Shared size constants for form controls so layouts compose without
/// embedding raw numeric literals.
enum ShellControlMetrics {
    static let fieldHeight: CGFloat = 34
    static let iconTile: CGFloat = 30
    static let iconGlyphSize: CGFloat = 15
    /// Fixed visible height of the icon well's internal scroll area:
    /// two full rows of tiles (iconTile * 2) plus the inter-row gaps
    /// (tight * 2) plus the well's inner padding on top and bottom
    /// (control * 2) plus one third of a tile so the cut lands in the
    /// gap/top-sliver of row 3, signalling that more icons are below.
    static let iconWellVisibleHeight: CGFloat =
        iconTile * 2 + ShellSpacing.tight * 2 + ShellSpacing.control * 2 + (iconTile / 3)
}

// MARK: - shellRaisedSurface ViewModifier

/// Gives any view a paper-card edge treatment: raised fill, hairline border
/// stroke, and a subtle lift shadow. An optional focused ring is drawn on top.
/// This is the canonical way to make a form control read as raised on the light
/// paper sidebar without relying on heavy material effects.
private struct ShellRaisedSurfaceModifier: ViewModifier {
    let cornerRadius: CGFloat
    let focused: Bool
    let disabled: Bool

    func body(content: Content) -> some View {
        content
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(disabled ? ShellPalette.panelSoft : ShellPalette.sidebarCard)
                    .shellShadow(ShellShadows.navigationSelection)
            )
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(ShellPalette.line.opacity(0.7), lineWidth: 1)
            )
            .overlay {
                if focused {
                    RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                        .strokeBorder(ShellSignal.focus.opacity(0.45), lineWidth: 1.5)
                }
            }
    }
}

extension View {
    /// Applies the shared raised-surface treatment used by all shell form
    /// controls: card fill, hairline border, lift shadow, and optional focus ring.
    func shellRaisedSurface(
        cornerRadius: CGFloat = ShellRadii.control,
        focused: Bool = false,
        disabled: Bool = false
    ) -> some View {
        modifier(
            ShellRaisedSurfaceModifier(
                cornerRadius: cornerRadius,
                focused: focused,
                disabled: disabled
            )
        )
    }
}

// MARK: - ShellFormSectionLabel

/// A muted caption label placed above a form control row.
/// The caller is responsible for the `ShellSpacing.tight` gap between this
/// label and its control.
struct ShellFormSectionLabel: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(ShellType.pro(ShellType.caption))
            .foregroundStyle(ShellPalette.sidebarMutedInk)
            .kerning(0.2)
    }
}

// MARK: - ShellTextField

/// A single-line plain text field styled with the raised-surface treatment and
/// a focus ring that activates when the bound `FocusState` binding is true.
struct ShellTextField: View {
    private let placeholder: String
    @Binding private var text: String
    private let focused: FocusState<Bool>.Binding
    private let onSubmit: () -> Void

    init(
        _ placeholder: String,
        text: Binding<String>,
        focused: FocusState<Bool>.Binding,
        onSubmit: @escaping () -> Void
    ) {
        self.placeholder = placeholder
        _text = text
        self.focused = focused
        self.onSubmit = onSubmit
    }

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(ShellType.pro(ShellType.row))
            .foregroundStyle(ShellPalette.sidebarInk)
            .focused(focused)
            .onSubmit(onSubmit)
            .padding(.horizontal, ShellSpacing.control)
            .frame(height: ShellControlMetrics.fieldHeight)
            .frame(maxWidth: .infinity)
            .shellRaisedSurface(focused: focused.wrappedValue)
    }
}

// MARK: - ShellSelectField

/// A full-width menu picker styled with the raised-surface treatment.
/// Pass the currently selected value label and supply menu items as a
/// `ViewBuilder` closure containing `Button` entries.
struct ShellSelectField<MenuContent: View>: View {
    private let value: String
    private let menu: () -> MenuContent

    init(value: String, @ViewBuilder menu: @escaping () -> MenuContent) {
        self.value = value
        self.menu = menu
    }

    var body: some View {
        Menu {
            menu()
        } label: {
            HStack(spacing: ShellSpacing.tight) {
                Text(value)
                    .font(ShellType.pro(ShellType.row))
                    .foregroundStyle(ShellPalette.sidebarInk)
                Spacer(minLength: 0)
                Image(systemName: "chevron.up.chevron.down")
                    .font(ShellType.pro(ShellType.monoCaption))
                    .foregroundStyle(ShellPalette.sidebarMutedInk)
            }
            .padding(.horizontal, ShellSpacing.control)
            .frame(maxWidth: .infinity)
            .frame(height: ShellControlMetrics.fieldHeight)
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity)
        .shellRaisedSurface()
    }
}

// MARK: - ShellIconPickerPanel

/// A padded raised panel used to house an icon grid. Uses `ShellRadii.row` to
/// visually group its contents as a container rather than a control.
/// The content is placed inside a fixed-height internal scroll region so the
/// well shows approximately two rows of tiles and scrolls vertically to reach
/// the rest without requiring an outer ScrollView on the form.
struct ShellIconPickerPanel<Content: View>: View {
    private let content: () -> Content

    init(@ViewBuilder content: @escaping () -> Content) {
        self.content = content
    }

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            content()
        }
        .frame(height: ShellControlMetrics.iconWellVisibleHeight)
        .padding(ShellSpacing.control)
        .frame(maxWidth: .infinity)
        .shellRaisedSurface(cornerRadius: ShellRadii.row)
    }
}

// MARK: - ShellIconTile

/// A square tap target for a single icon option inside a `ShellIconPickerPanel`.
/// Renders a selection ring and tinted fill when `isSelected` is true; a hover
/// fill when the pointer is over the tile.
struct ShellIconTile: View {
    private let systemName: String
    private let isSelected: Bool
    private let action: () -> Void

    @State private var isHovered: Bool = false

    init(systemName: String, isSelected: Bool, action: @escaping () -> Void) {
        self.systemName = systemName
        self.isSelected = isSelected
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(ShellType.pro(ShellControlMetrics.iconGlyphSize, weight: .medium))
                .foregroundStyle(glyphColor)
                .frame(width: ShellControlMetrics.iconTile, height: ShellControlMetrics.iconTile)
                .contentShape(Rectangle())
                .background(
                    RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                        .fill(tileFill)
                        .overlay {
                            if isSelected {
                                RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                                    .strokeBorder(ShellSignal.focus.opacity(0.6), lineWidth: 1)
                            }
                        }
                )
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var tileFill: Color {
        if isSelected {
            return ShellSignal.focus.opacity(0.14)
        } else if isHovered {
            return ShellPalette.sidebarHover
        } else {
            return .clear
        }
    }

    private var glyphColor: Color {
        if isSelected {
            return ShellSignal.focus
        } else if isHovered {
            return ShellPalette.sidebarInk
        } else {
            return ShellPalette.sidebarMutedInk
        }
    }
}

// MARK: - ShellButton

/// Role enum for the unified button component.
enum ShellButtonRole { case primary, ghost }

/// A full-width role-based button used in shell forms.
///
/// - `.primary` — focus-signal fill with white label and lift shadow when
///   enabled; muted surface with no shadow when disabled.
/// - `.ghost`   — transparent fill (hover tint on pointer-over) with muted
///   ink label; suitable for secondary actions such as Cancel.
///
/// Both roles render at `ShellControlMetrics.fieldHeight` and expand to fill
/// their container, forming a coherent primary + ghost pair when stacked.
struct ShellButton: View {
    private let title: String
    private let role: ShellButtonRole
    private let enabled: Bool
    private let action: () -> Void
    @State private var isHovered = false

    init(
        _ title: String,
        role: ShellButtonRole = .primary,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.role = role
        self.enabled = enabled
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(ShellType.pro(ShellType.row, weight: role == .primary ? .semibold : .medium))
                .foregroundStyle(labelColor)
                .frame(maxWidth: .infinity)
                .frame(height: ShellControlMetrics.fieldHeight)
                .contentShape(RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous))
                .background { background }
        }
        .buttonStyle(ShellButtonPressStyle())
        .disabled(!enabled)
        .onHover { isHovered = $0 }
    }

    @ViewBuilder private var background: some View {
        switch role {
        case .primary:
            RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                .fill(enabled ? ShellSignal.focus : ShellPalette.sidebarControl)
                .shellShadow(enabled ? ShellShadows.navigationSelection : ShellShadows.none)
        case .ghost:
            RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                .fill(isHovered ? ShellPalette.sidebarHover : Color.clear)
        }
    }

    private var labelColor: Color {
        switch role {
        case .primary: return enabled ? Color.white : ShellPalette.sidebarMutedInk
        case .ghost:   return isHovered ? ShellPalette.sidebarInk : ShellPalette.sidebarMutedInk
        }
    }
}

/// Subtle press-scale feedback shared by all `ShellButton` roles.
private struct ShellButtonPressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.98 : 1.0)
            .animation(.easeInOut(duration: 0.12), value: configuration.isPressed)
    }
}

#endif
