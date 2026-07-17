import SwiftUI

struct ShellFindBarView: View {
    let searchState: AlanTerminalSearchState
    let onQueryChange: (String) -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onClose: () -> Void

    @State private var query: String
    @FocusState private var isFocused: Bool

    init(
        searchState: AlanTerminalSearchState,
        onQueryChange: @escaping (String) -> Void,
        onNext: @escaping () -> Void,
        onPrevious: @escaping () -> Void,
        onClose: @escaping () -> Void
    ) {
        self.searchState = searchState
        self.onQueryChange = onQueryChange
        self.onNext = onNext
        self.onPrevious = onPrevious
        self.onClose = onClose
        _query = State(initialValue: searchState.query)
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(ShellPalette.mutedInk)

            TextField("Find", text: $query)
                .textFieldStyle(.plain)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(ShellPalette.ink)
                .focused($isFocused)
                .frame(width: 180)
                .onChange(of: query) { _, nextQuery in
                    onQueryChange(nextQuery)
                }
                .onChange(of: searchState.query) { _, nextQuery in
                    guard nextQuery != query else { return }
                    query = nextQuery
                }
                .onChange(of: searchState.focusRequestID) { _, _ in
                    isFocused = true
                }
                .onSubmit {
                    onNext()
                }

            Text(resultLabel)
                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                .foregroundStyle(ShellPalette.mutedInk)
                .frame(minWidth: 48, alignment: .trailing)

            Button(action: onPrevious) {
                Image(systemName: "chevron.up")
                    .font(.system(size: 10, weight: .bold))
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .help("Previous match")
            .keyboardShortcut("g", modifiers: [.command, .shift])

            Button(action: onNext) {
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .bold))
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .help("Next match")
            .keyboardShortcut("g", modifiers: [.command])

            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .bold))
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .help("Close Find")
            .keyboardShortcut(.escape, modifiers: [])
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(
            ShellMaterialShape(
                role: .floatingInput,
                shape: RoundedRectangle(cornerRadius: ShellRadii.surface, style: .continuous)
            )
        )
        .overlay {
            RoundedRectangle(cornerRadius: ShellRadii.surface, style: .continuous)
                .stroke(ShellPalette.line.opacity(0.35), lineWidth: 1)
        }
        .shellShadow(ShellShadows.floatingInput)
        .onAppear {
            query = searchState.query
            isFocused = true
        }
        .onExitCommand {
            onClose()
        }
    }

    private var resultLabel: String {
        if let total = searchState.totalMatches,
           let selected = searchState.selectedIndex
        {
            guard total > 0 else { return "0" }
            return "\(selected + 1)/\(total)"
        }
        return query.isEmpty ? "" : "..."
    }
}

struct ShellInactivePaneDim: View {
    let isSelected: Bool
    let isEnabled: Bool

    var body: some View {
        Rectangle()
            .fill(Color.black.opacity(isSelected || !isEnabled ? 0 : 0.14))
            .allowsHitTesting(false)
    }
}
