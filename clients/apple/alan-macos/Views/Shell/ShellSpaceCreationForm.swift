import SwiftUI

#if os(macOS)

/// In-sidebar Space creation form. Reuses the shipped curated symbol list and
/// monogram resolver; the icon grid replaces the old horizontal strip. Name is
/// required (Create disabled while empty) — the source-level fix for
/// indistinguishable auto-named Spaces.
struct ShellSpaceCreationForm: View {
    /// Decoupled profile option so the form does not depend on
    /// `TerminalProfileDefinition` or ShellSidebarView's private name helper.
    struct ProfileOption: Identifiable, Equatable {
        let id: String
        let name: String
        let isEnabled: Bool
        let guidance: String?

        init(
            id: String,
            name: String,
            isEnabled: Bool = true,
            guidance: String? = nil
        ) {
            self.id = id
            self.name = name
            self.isEnabled = isEnabled
            self.guidance = guidance
        }
    }

    let profiles: [ProfileOption]
    let onCreate: () -> Void
    let onCancel: () -> Void

    @Binding private var name: String
    @Binding private var selectedIcon: String?
    @Binding private var selectedProfileID: String?
    @FocusState private var nameFieldFocused: Bool

    init(
        profiles: [ProfileOption],
        draftName: Binding<String>,
        draftIcon: Binding<String?>,
        draftProfileID: Binding<String?>,
        onCreate: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.profiles = profiles
        _name = draftName
        _selectedIcon = draftIcon
        _selectedProfileID = draftProfileID
        self.onCreate = onCreate
        self.onCancel = onCancel
    }

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canCreate: Bool { !trimmedName.isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: ShellSpacing.section) {
            // Name
            VStack(alignment: .leading, spacing: ShellSpacing.tight) {
                ShellFormSectionLabel("Name")
                ShellTextField(
                    "Space name…",
                    text: $name,
                    focused: $nameFieldFocused,
                    onSubmit: { if canCreate { submit() } }
                )
            }

            // Icon
            VStack(alignment: .leading, spacing: ShellSpacing.tight) {
                ShellFormSectionLabel("Icon")
                ShellIconPickerPanel {
                    let columns = Array(
                        repeating: GridItem(.flexible(), spacing: ShellSpacing.tight),
                        count: 6
                    )
                    LazyVGrid(columns: columns, spacing: ShellSpacing.tight) {
                        ShellIconTile(
                            systemName: "textformat",
                            isSelected: selectedIcon == nil
                        ) {
                            selectedIcon = nil
                        }
                        ForEach(ShellSpaceIconCatalog.curatedSymbols, id: \.self) { symbol in
                            ShellIconTile(
                                systemName: symbol,
                                isSelected: selectedIcon == symbol
                            ) {
                                selectedIcon = symbol
                            }
                        }
                    }
                }
            }

            // Profile
            VStack(alignment: .leading, spacing: ShellSpacing.tight) {
                ShellFormSectionLabel("Profile")
                ShellSelectField(value: selectedProfileName) {
                    Button("Login shell") { selectedProfileID = nil }
                    ForEach(profiles) { profile in
                        Button(profile.name) { selectedProfileID = profile.id }
                            .disabled(!profile.isEnabled)
                            .help(profile.guidance ?? profile.name)
                    }
                }
            }

            // Footer
            VStack(spacing: ShellSpacing.control) {
                ShellButton("Create Space", role: .primary, enabled: canCreate) { submit() }
                ShellButton("Cancel", role: .ghost) { onCancel() }
            }
            .padding(.top, ShellSpacing.control)
        }
        .padding(.horizontal, ShellSpacing.section)
        .padding(.top, ShellSpacing.section)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .onExitCommand { onCancel() }
        .onAppear { nameFieldFocused = true }
    }

    // MARK: - Helpers

    private var selectedProfileName: String {
        guard let id = selectedProfileID,
              let match = profiles.first(where: { $0.id == id })
        else { return "Login shell" }
        return match.name
    }

    private func submit() {
        guard canCreate else { return }
        onCreate()
    }
}
#endif
