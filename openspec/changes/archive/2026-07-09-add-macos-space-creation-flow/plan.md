# Space Creation Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace casual one-click Space creation with a deliberate in-sidebar form (required name + icon + profile) for manual creation, while programmatic/CLI creation stays instant and self-names from its working directory.

**Architecture:** A pure `ShellSpaceDefaultName` helper derives names from working directories. `creatingSpace`/`createSpace` gain that derivation plus a `presentationIconSystemName` passthrough so a Space is born named and iconed. The titlebar `+` (in `MacShellRootView` chrome) flips a published `isPresentingSpaceCreation` flag on the shared `ShellHostController`; `ShellSidebarView` observes it and swaps its body for `ShellSpaceCreationForm`. The form reuses the shipped curated symbol list + monogram resolver; only an inline icon strip is new.

**Tech Stack:** SwiftUI/AppKit (macOS), swiftc script tests, just.

**Branch:** `paper-and-ink-design-system`. Build/type-check via `ALAN_APP_INSTALL_DIR="$HOME/Applications/AlanDevPaperInk" just install-dev` (the only real type-check; script tests are parse/logic level). Per-task: keep `test-shell-design-tokens.sh`, `check-shell-contracts.sh`, `apple-shell-focused-tests`, `check-shell-design-tokens.sh` green.

---

### Task 1: `ShellSpaceDefaultName` pure helper

**Files:**
- Modify: `clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift` (add the enum near `ShellSpacePresentationIcon`)
- Modify: `clients/apple/scripts/test-shell-space-icon.swift` + `.sh` (the existing space-icon test already compiles ShellSnapshots.swift — extend it)

- [ ] **Step 1: Add the failing assertions**

In `clients/apple/scripts/test-shell-space-icon.swift`, inside the test runner's `main()`, append:

```swift
expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/Users/x/univer") == "univer",
       "derive: plain leaf")
expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/Users/x/univer/") == "univer",
       "derive: trailing slash")
expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/Users/x/proj/.git") == "proj",
       "derive: strips .git")
expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/") == "",
       "derive: root is empty")
expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: nil) == "",
       "derive: nil is empty")
expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "  ") == "",
       "derive: blank is empty")
```

- [ ] **Step 2: Run it, expect compile failure**

Run: `bash clients/apple/scripts/test-shell-space-icon.sh`
Expected: FAIL — `cannot find 'ShellSpaceDefaultName' in scope`.

- [ ] **Step 3: Implement the helper**

In `ShellSnapshots.swift`, directly after the `enum ShellSpacePresentationIcon { ... }` block, add:

```swift
/// Derives a human default Space name from a working directory. Pure; the
/// "Space N" index fallback stays in `creatingSpace` and is used when this
/// returns "".
enum ShellSpaceDefaultName {
    static func derive(fromWorkingDirectory path: String?) -> String {
        guard let path else { return "" }
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }

        var components = trimmed.split(separator: "/", omittingEmptySubsequences: true)
            .map(String.init)
        if components.last == ".git" {
            components.removeLast()
        }
        return components.last ?? ""
    }
}
```

- [ ] **Step 4: Run it, expect pass**

Run: `bash clients/apple/scripts/test-shell-space-icon.sh`
Expected: all PASS including the 6 new `derive:` lines.

- [ ] **Step 5: Commit**

```bash
git add clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift \
        clients/apple/scripts/test-shell-space-icon.swift
git commit -m "feat(macos-shell): derive Space default name from working directory"
```

---

### Task 2: Apply derivation + icon passthrough in `creatingSpace`

**Files:**
- Modify: `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift:231-291` (`creatingSpace`) and `:293-308` (`creatingTerminalSpace`)

- [ ] **Step 1: Add `presentationIconSystemName` param + derived default to `creatingSpace`**

Change the `creatingSpace` signature to add the icon param (after `terminalProfileID`):

```swift
    func creatingSpace(
        launchTarget: ShellLaunchTarget,
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil,
        reservedPaneIDs: Set<String> = [],
        defaultWorkingDirectory: String = defaultShellWorkingDirectory(),
        now: Date = .now
    ) -> ShellStateMutationResult {
```

Replace the `spaceIndex`/title resolution. Currently `let spaceIndex = spaces.count + 1` and `title: title ?? "Space \(spaceIndex)"`. Change to compute a resolved title:

```swift
        let spaceIndex = spaces.count + 1
        let resolvedTitle: String = {
            if let title, !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return title
            }
            let derived = ShellSpaceDefaultName.derive(fromWorkingDirectory: workingDirectory)
            return derived.isEmpty ? "Space \(spaceIndex)" : derived
        }()
```

And in the `ShellSpace(...)` literal change `title: title ?? "Space \(spaceIndex)"` to `title: resolvedTitle`, and add `presentationIconSystemName: presentationIconSystemName` to that initializer (validate first — only store a supported symbol):

```swift
        let space = ShellSpace(
            spaceID: spaceID,
            title: resolvedTitle,
            attention: .active,
            tabs: [tab],
            selectedTabID: tabID,
            terminalProfileID: terminalProfileID,
            presentationIconSystemName: ShellSpacePresentationIcon
                .isSupportedSystemName(presentationIconSystemName) ? presentationIconSystemName : nil
        )
```

- [ ] **Step 2: Thread the param through `creatingTerminalSpace`**

Add `presentationIconSystemName: String? = nil` to `creatingTerminalSpace`'s signature (after `terminalProfileID`) and pass it into the `creatingSpace(...)` call it wraps.

- [ ] **Step 3: Verify focused tests + token tests still pass**

```bash
bash clients/apple/scripts/test-shell-space-icon.sh
bash clients/apple/scripts/check-shell-contracts.sh
just apple-shell-focused-tests
```
Expected: all green (no behavior change for callers not passing the new args; the derived-name path only triggers when `title` is nil and a `workingDirectory` is present).

- [ ] **Step 4: Commit**

```bash
git add clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift
git commit -m "feat(macos-shell): self-name and icon Spaces at creation"
```

---

### Task 3: `createSpace` passthrough + form controller entry

**Files:**
- Modify: `clients/apple/alan-macos/ShellHostController.swift:1187-1218` (`createSpace`, `createTerminalSpace`) and add a published flag + form entry

- [ ] **Step 1: Add the icon param to `createSpace`/`createTerminalSpace`**

In `createSpace` add `presentationIconSystemName: String? = nil` (after `terminalProfileID`) and pass it to `shellState.creatingSpace(...)`. In `createTerminalSpace` add the same param and forward it.

```swift
    @discardableResult
    func createSpace(
        launchTarget: ShellLaunchTarget = .shell,
        title: String? = nil,
        workingDirectory: String? = nil,
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil
    ) -> String? {
        let resolvedTerminalProfileID = terminalProfileID
            ?? globalDefaultTerminalProfileIDForPaneCapture()
        let result = shellState.creatingSpace(
            launchTarget: launchTarget,
            title: title,
            workingDirectory: workingDirectory,
            terminalProfileID: resolvedTerminalProfileID,
            presentationIconSystemName: presentationIconSystemName,
            reservedPaneIDs: terminalRuntimeRegistry.registeredPaneIDs
        )
        applyMutationResult(result)
        return result.spaceID
    }
```

(Forward the new arg through `createTerminalSpace` the same way.)

- [ ] **Step 2: Add the presentation flag + form-create entry**

Find where `ShellHostController`'s `@Published` UI properties live (search `@Published` in the file) and add:

```swift
    @Published var isPresentingSpaceCreation = false

    func beginSpaceCreation() {
        isPresentingSpaceCreation = true
    }

    func cancelSpaceCreation() {
        isPresentingSpaceCreation = false
    }

    @discardableResult
    func createSpaceFromForm(
        name: String,
        iconSystemName: String?,
        profileID: String?
    ) -> String? {
        let spaceID = createSpace(
            launchTarget: .shell,
            title: name,
            terminalProfileID: profileID,
            presentationIconSystemName: iconSystemName
        )
        isPresentingSpaceCreation = false
        if let spaceID {
            select(spaceID: spaceID)
        }
        return spaceID
    }
```

(Confirm `select(spaceID:)` is the existing selection method — grep `func select(spaceID`; if the name differs, use the real one.)

- [ ] **Step 3: Type-check via build**

```bash
ALAN_APP_INSTALL_DIR="$HOME/Applications/AlanDevPaperInk" just install-dev 2>&1 | tail -3
```
Expected: exit 0 (the `rm /usr/local/bin/alan-dev: Permission denied` tail line is a known root-owned-symlink issue, not a compile error — the "Release app assembled" line above it confirms the build+sign succeeded).

- [ ] **Step 4: Commit**

```bash
git add clients/apple/alan-macos/ShellHostController.swift
git commit -m "feat(macos-shell): add space-creation flag and form create entry"
```

---

### Task 4: `ShellSpaceCreationForm` view

**Files:**
- Create: `clients/apple/alan-macos/Views/Shell/ShellSpaceCreationForm.swift`
- Modify: `clients/apple/alan-macos.xcodeproj/project.pbxproj` — REQUIRED. This project uses explicit file references (`ShellSidebarView.swift` appears 4× in the pbxproj: a `PBXFileReference`, a `PBXBuildFile`, a `PBXGroup` child entry, and a `PBXSourcesBuildPhase` entry). The new file must be added the same way. The reliable approach: replicate each of the four `ShellSidebarView.swift` lines for `ShellSpaceCreationForm.swift` with two fresh unique 24-hex-char object IDs (one for the fileRef, one for the buildFile) not already present in the file. After editing, the Task 4 build step is the verification that the file is correctly wired (a missing/!malformed ref fails the build).

- [ ] **Step 1: Create the view**

Create `clients/apple/alan-macos/Views/Shell/ShellSpaceCreationForm.swift`:

```swift
import SwiftUI

#if os(macOS)

/// In-sidebar Space creation form. Reuses the shipped curated symbol list and
/// monogram resolver; the inline icon strip is the only new control. Name is
/// required (Create disabled while empty) — the source-level fix for
/// indistinguishable auto-named Spaces.
struct ShellSpaceCreationForm: View {
    /// Decoupled profile option so the form does not depend on
    /// `TerminalProfileDefinition` or ShellSidebarView's private name helper.
    struct ProfileOption: Identifiable, Equatable {
        let id: String
        let name: String
    }

    let profiles: [ProfileOption]
    let defaultProfileID: String?
    let onCreate: (_ name: String, _ iconSystemName: String?, _ profileID: String?) -> Void
    let onCancel: () -> Void

    @State private var name: String = ""
    @State private var selectedIcon: String? = nil
    @State private var selectedProfileID: String?
    @FocusState private var nameFieldFocused: Bool

    init(
        profiles: [ProfileOption],
        defaultProfileID: String?,
        onCreate: @escaping (String, String?, String?) -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.profiles = profiles
        self.defaultProfileID = defaultProfileID
        self.onCreate = onCreate
        self.onCancel = onCancel
        _selectedProfileID = State(initialValue: defaultProfileID)
    }

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canCreate: Bool { !trimmedName.isEmpty }

    private var previewIcon: ShellSpacePresentationIcon.Resolved {
        ShellSpacePresentationIcon.resolve(systemName: selectedIcon, title: trimmedName)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ShellSpacing.section) {
            previewTile
            nameField
            iconStrip
            profilePicker
            footer
        }
        .padding(.horizontal, ShellSpacing.control)
        .padding(.top, ShellSpacing.section)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .onAppear { nameFieldFocused = true }
        .onExitCommand { onCancel() }   // Esc
    }

    private var previewTile: some View {
        ZStack {
            RoundedRectangle(cornerRadius: ShellRadii.row, style: .continuous)
                .fill(ShellPalette.sidebarControl)
            iconGlyph
        }
        .frame(width: 44, height: 44)
        .frame(maxWidth: .infinity, alignment: .center)
    }

    @ViewBuilder
    private var iconGlyph: some View {
        switch previewIcon {
        case .symbol(let n), .fallbackSymbol(let n):
            Image(systemName: n)
                .font(ShellType.pro(20, weight: .semibold))
                .foregroundStyle(ShellPalette.sidebarInk)
        case .monogram(let m):
            Text(m)
                .font(ShellType.pro(20, weight: .semibold))
                .foregroundStyle(ShellPalette.sidebarInk)
        }
    }

    private var nameField: some View {
        TextField("Space name…", text: $name)
            .textFieldStyle(.plain)
            .font(ShellType.pro(ShellType.row))
            .foregroundStyle(ShellPalette.sidebarInk)
            .focused($nameFieldFocused)
            .onSubmit { if canCreate { submit() } }
            .padding(.horizontal, ShellSpacing.control)
            .frame(height: 30)
            .background {
                RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                    .fill(ShellPalette.sidebarControl)
                    .overlay {
                        RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                            .strokeBorder(ShellPalette.line.opacity(0.4), lineWidth: 0.7)
                    }
            }
    }

    private var iconStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: ShellSpacing.tight) {
                iconChip(symbol: nil)   // Default (monogram)
                ForEach(ShellSpaceIconCatalog.curatedSymbols, id: \.self) { symbol in
                    iconChip(symbol: symbol)
                }
            }
            .padding(.horizontal, ShellSpacing.hair)
        }
        .frame(height: 32)
    }

    private func iconChip(symbol: String?) -> some View {
        let isSelected = selectedIcon == symbol
        return Button {
            selectedIcon = symbol
        } label: {
            Group {
                if let symbol {
                    Image(systemName: symbol)
                        .font(ShellType.pro(ShellType.row, weight: .medium))
                } else {
                    Image(systemName: "textformat")
                        .font(ShellType.pro(ShellType.row, weight: .medium))
                }
            }
            .foregroundStyle(isSelected ? ShellPalette.sidebarInk : ShellPalette.sidebarMutedInk)
            .frame(width: 28, height: 28)
            .background {
                RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                    .fill(isSelected ? ShellPalette.sidebarSelection : .clear)
            }
        }
        .buttonStyle(.plain)
        .help(symbol == nil ? "Default (initial)" : symbol!)
    }

    private var profilePicker: some View {
        Menu {
            Button("Default") { selectedProfileID = nil }
            ForEach(profiles, id: \.id) { profile in
                Button(profile.name) { selectedProfileID = profile.id }
            }
        } label: {
            HStack(spacing: ShellSpacing.tight) {
                Text("Profile")
                    .font(ShellType.pro(ShellType.caption))
                    .foregroundStyle(ShellPalette.sidebarMutedInk)
                Spacer()
                Text(selectedProfileName)
                    .font(ShellType.pro(ShellType.caption, weight: .medium))
                    .foregroundStyle(ShellPalette.sidebarInk)
                Image(systemName: "chevron.up.chevron.down")
                    .font(ShellType.pro(ShellType.monoCaption))
                    .foregroundStyle(ShellPalette.sidebarMutedInk)
            }
        }
        .menuStyle(.borderlessButton)
        .padding(.horizontal, ShellSpacing.control)
        .frame(height: 28)
        .background {
            RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                .fill(ShellPalette.sidebarControl)
        }
    }

    private var selectedProfileName: String {
        guard let id = selectedProfileID,
              let match = profiles.first(where: { $0.id == id })
        else { return "Default" }
        return match.name
    }

    private var footer: some View {
        VStack(spacing: ShellSpacing.tight) {
            Button(action: submit) {
                Text("Create Space")
                    .font(ShellType.pro(ShellType.row, weight: .semibold))
                    .foregroundStyle(canCreate ? Color.white : ShellPalette.sidebarMutedInk)
                    .frame(maxWidth: .infinity)
                    .frame(height: 30)
                    .background {
                        RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                            .fill(canCreate ? ShellSignal.focus : ShellPalette.sidebarControl)
                    }
            }
            .buttonStyle(.plain)
            .disabled(!canCreate)

            Button("Cancel", action: onCancel)
                .buttonStyle(.plain)
                .font(ShellType.pro(ShellType.caption))
                .foregroundStyle(ShellPalette.sidebarMutedInk)
        }
    }

    private func submit() {
        guard canCreate else { return }
        onCreate(trimmedName, selectedIcon, selectedProfileID)
    }
}
#endif
```

Note: the form takes `[ProfileOption]` (id+name), NOT the app's
`TerminalProfileDefinition` — the caller (Task 5) maps profiles to options
using ShellSidebarView's existing `profileMenuTitle(_:)` helper, keeping the
form decoupled and previewable. Verify `ShellSignal.focus`,
`ShellPalette.sidebarControl/sidebarSelection/sidebarInk/sidebarMutedInk/line`
exist (they do, per ShellDesignTokens.swift).

- [ ] **Step 2: Type-check via build**

```bash
ALAN_APP_INSTALL_DIR="$HOME/Applications/AlanDevPaperInk" just install-dev 2>&1 | tail -3
```
Expected: build+sign succeeds ("Release app assembled").

- [ ] **Step 3: Guards green**

```bash
./scripts/check-shell-design-tokens.sh
```
Expected: pass. If the new file introduced raw `system(size:` literals (the `ShellType.pro(20...)` and `pro(ShellType.row...)` calls are NOT raw — they go through ShellType), the count should be 0 for the new file; if not, run `./scripts/check-shell-design-tokens.sh --update-baseline` only if it does not increase any existing entry (a brand-new file at >0 means you left a raw literal — fix it instead).

- [ ] **Step 4: Commit**

```bash
git add clients/apple/alan-macos/Views/Shell/ShellSpaceCreationForm.swift \
        clients/apple/alan-macos.xcodeproj/project.pbxproj
git commit -m "feat(macos-shell): add in-sidebar Space creation form view"
```
(Include project.pbxproj only if the project required an explicit file reference per Task 4 preamble.)

---

### Task 5: Sidebar takeover + titlebar `+` rewire

**Files:**
- Modify: `clients/apple/alan-macos/MacShellRootView.swift:448-450` (titlebar `+` action)
- Modify: `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift` (`sidebarContent` ~line 72 / `body`)

- [ ] **Step 1: Rewire the titlebar `+` to open the form**

In `MacShellRootView.swift`, change the `ShellSidebarNewSpaceControl` action:

```swift
                ShellSidebarNewSpaceControl {
                    host.beginSpaceCreation()
                }
```

- [ ] **Step 2: Swap the sidebar body when creating**

In `ShellSidebarView.swift`, find the top-level content composition (`sidebarContent` ~line 72, which currently shows `fixedSpaceSlider` + `spaceContentPager`). Wrap it so that when `host.isPresentingSpaceCreation` is true, the form replaces it. `host` is already an `@ObservedObject`/`@StateObject` in this view (it reads `host.selectedTab` etc.), so `host.isPresentingSpaceCreation` is observed automatically. Example:

```swift
    private var sidebarContent: some View {
        Group {
            if host.isPresentingSpaceCreation {
                ShellSpaceCreationForm(
                    profiles: TerminalProfileStore.defaultStore().load().profiles.map {
                        ShellSpaceCreationForm.ProfileOption(id: $0.id, name: profileMenuTitle($0))
                    },
                    defaultProfileID: nil,
                    onCreate: { name, icon, profileID in
                        _ = host.createSpaceFromForm(
                            name: name, iconSystemName: icon, profileID: profileID
                        )
                    },
                    onCancel: { host.cancelSpaceCreation() }
                )
                .transition(.opacity)
            } else {
                VStack(spacing: 0) {
                    fixedSpaceSlider
                    spaceContentPager
                }
            }
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.18), value: host.isPresentingSpaceCreation)
    }
```

Match the EXACT current structure of `sidebarContent` when you edit (read it first; it may already be a `VStack` — wrap that existing body in the `else`, do not guess its contents). `reduceMotion` is already an `@Environment(\.accessibilityReduceMotion)` in this view (it is used by the space pager). `TerminalProfileStore.defaultStore().load().profiles` is the same source the existing Space context-menu profile submenu uses — confirm by reading `spaceContextMenu`.

- [ ] **Step 3: Type-check via build + run focused tests**

```bash
ALAN_APP_INSTALL_DIR="$HOME/Applications/AlanDevPaperInk" just install-dev 2>&1 | tail -3
bash clients/apple/scripts/test-shell-sidebar-presentation.sh
bash clients/apple/scripts/check-shell-contracts.sh
```
Expected: build succeeds; tests green. If `test-shell-sidebar-presentation.sh` asserts the old instant-create behavior of the `+`, update that assertion to the new open-form behavior and note it.

- [ ] **Step 4: Commit**

```bash
git add clients/apple/alan-macos/MacShellRootView.swift \
        clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift
git commit -m "feat(macos-shell): open the Space creation form from the titlebar plus"
```

---

### Task 6: Spec delta

**Files:**
- Create: `openspec/changes/add-macos-space-creation-flow/specs/macos-shell-ui-ux-conformance/spec.md`

- [ ] **Step 1: Write the delta**

Create the file:

```markdown
## MODIFIED Requirements

### Requirement: Space slider supports adaptive density and scrub navigation
The default macOS shell Space slider SHALL use a continuous rounded track that
adapts Space target widths to available sidebar space, supports every Space
without an arbitrary count cap, and preserves preview-first scrub navigation
without hover-driven geometry changes or cover-flow motion. Manual Space
creation SHALL use a deliberate in-sidebar creation form so a Space is named
and given an identity at birth; programmatic creation SHALL remain instant with
a name derived from its working directory.

#### Scenario: Titlebar New Space opens the creation form
- **WHEN** the user activates the titlebar New Space control
- **THEN** the sidebar content switches to a creation form with a required name
  field, an inline curated icon selection (defaulting to the name monogram),
  and a terminal profile selector
- **AND** the form does not offer a menu of Space variants or types
- **AND** Create is unavailable until a non-empty name is entered
- **AND** Cancel or Escape returns the sidebar to its navigation content
  without creating a Space

#### Scenario: Programmatic creation stays instant and self-named
- **WHEN** a Space is created programmatically (CLI, worktree, or API) with a
  working directory and no explicit title
- **THEN** alan creates the Space immediately without showing the form
- **AND** the Space name is derived from the working directory leaf rather than
  a generic "Space N" label, falling back to the indexed label only when no
  working directory is available
```

- [ ] **Step 2: Check off tasks + commit**

Mark tasks 1-6 done in `openspec/changes/add-macos-space-creation-flow/tasks.md`.

```bash
git add openspec/changes/add-macos-space-creation-flow/specs \
        openspec/changes/add-macos-space-creation-flow/tasks.md
git commit -m "docs(openspec): add Space creation flow spec delta"
```

---

## Self-Review Notes

- Spec coverage: default-name helper (T1), derivation+icon at creation (T2),
  controller passthrough+flag+form-entry (T3), the form view (T4), sidebar
  takeover+`+` rewire (T5), spec delta (T6). All design.md components mapped.
- The shared trigger seam (titlebar `+` in `MacShellRootView`, form in
  `ShellSidebarView`) is resolved via a published flag on the shared
  `ShellHostController` — the only observable both views already hold.
- Type-checking depends on the real Xcode build (Tasks 3-5 run it) since the
  script tests are parse/logic level; the form view (T4) and takeover (T5) are
  not covered by any standalone compilable test, so the build is the gate.
- Verify-before-assume points flagged inline: `TerminalProfile` property names,
  `select(spaceID:)` method name, `sidebarContent`'s exact current body,
  pbxproj file-reference requirement, and whether the sidebar-presentation test
  pins the old `+` behavior.

---

## Correction: draft-in-slider rework (supersedes Tasks 4-5 presentation)

Tasks 1-3 and 6 stand. Tasks 4-5 shipped a whole-sidebar takeover; this rework
makes the slider stay visible with an appended draft target and only the
tab-list region become the form. Because the draft state is shared across the
form, `ShellSidebarView`, and `ShellSidebarSpaceSlider`, implement this as ONE
cohesive commit (intermediate splits would not compile).

### Slim the form (`ShellSpaceCreationForm.swift`)
Remove `previewTile` and `iconGlyph` and their use in `body`. New body order:
name field, icon strip, profile picker, footer. Everything else
(ProfileOption, required-name `canCreate`/`submit`, `.onExitCommand`, onSubmit)
unchanged.

### `ShellSidebarView` draft state + content-only swap
- Add `@State private var spaceDraftName = ""`, `@State private var
  spaceDraftIcon: String?`, `@State private var spaceDraftProfileID: String?`.
- Add `.onChange(of: host.isPresentingSpaceCreation) { _, presenting in
  if presenting { spaceDraftName = ""; spaceDraftIcon = nil;
  spaceDraftProfileID = nil } }`.
- In `sidebarContent`, REVERT the full if/else takeover. Always render the
  `VStack { fixedSpaceSlider …; <content> }`. `<content>` is:
  `if host.isPresentingSpaceCreation { ShellSpaceCreationForm(profiles:
  spaceCreationProfileOptions, draftName: $spaceDraftName, draftIcon:
  $spaceDraftIcon, draftProfileID: $spaceDraftProfileID, onCreate: { _ = host
  .createSpaceFromForm(name: spaceDraftName, iconSystemName: spaceDraftIcon,
  profileID: spaceDraftProfileID) }, onCancel: { host.cancelSpaceCreation() })
  } else { spaceContentPager … }`. (Change the form to take Bindings for the
  three draft fields instead of owning them, so the slider sees live edits;
  update the form's `@State` to `@Binding` accordingly and drop its private
  init defaults.)
- `fixedSpaceSlider` passes the draft to the slider:
  `creationDraft: host.isPresentingSpaceCreation ?
  ShellSpaceSliderDraft(name: spaceDraftName, iconSystemName: spaceDraftIcon)
  : nil`.

### `ShellSidebarSpaceSlider` draft target
- Define `struct ShellSpaceSliderDraft: Equatable { let name: String; let
  iconSystemName: String? }`.
- Add `var creationDraft: ShellSpaceSliderDraft? = nil` to the slider.
- When non-nil: the layout target count becomes `visibleSpaces.count + 1`, the
  selected index is forced to the last (draft) index, and the trailing target
  renders `ShellSpacePresentationIcon.resolve(systemName:
  creationDraft.iconSystemName, title: creationDraft.name)` with the draft name
  as label (monogram fallback when name is blank). The draft target is display
  only — tapping it is a no-op; scrub/selection commit is disabled while a
  draft is present (guard the scrub/click handlers with `creationDraft == nil`).
- Keep all existing real-space rendering unchanged when `creationDraft == nil`.

### Build + verify
`ALAN_APP_INSTALL_DIR="$HOME/Applications/AlanDevPaperInk" just install-dev`
must assemble; `test-shell-sidebar-presentation.sh`, `check-shell-contracts.sh`,
`./scripts/check-shell-design-tokens.sh`, `apple-shell-focused-tests` green.

### Spec delta update
Amend the "Titlebar New Space opens the creation form" scenario: the slider
stays visible with an appended selected draft target; the tab-list region (not
the whole sidebar) hosts the form; Cancel/Escape removes the draft and restores
the prior selection.
