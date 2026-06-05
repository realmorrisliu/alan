# Settings Navigation IA Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current six-group Settings IA with the approved General / Terminal / Agent / System model and polish the Settings layout, spacing, typography, and row alignment.

**Architecture:** `ShellSettingsSurfaceSnapshot` remains the source of row content. A new group-section model derives task-oriented sections from existing row IDs, with only the Alan agent selector introduced as a new Settings affordance. `TerminalPaneView.swift` keeps the two-column Settings surface but tightens navigation, section, and row composition for a calmer native layout.

**Tech Stack:** SwiftUI macOS client, shell settings model tests, shell contract grep checks, OpenSpec validation, Xcode Debug build, Alan Dev visual verification.

---

## File Structure

- Modify `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift`
  - Replace six navigation groups with General, Terminal, Agent, System.
  - Add group-section models and row-ID based grouping.
  - Rename `Public skills` row to `Skill Packages`.
  - Add an Alan-only agent selector row or model affordance.
- Modify `clients/apple/alan-macos/TerminalPaneView.swift`
  - Render group sections from the new model.
  - Add Alan-only selector treatment in Agent.
  - Refine navigation, content width, section spacing, row min heights, typography, and value alignment.
- Modify `clients/apple/scripts/test-shell-settings-surface.swift`
  - Update tests to the four-group IA and exact row membership.
- Modify `clients/apple/scripts/check-shell-contracts.sh`
  - Update structural checks from six-group assumptions to Agent/System assumptions.
- Modify `openspec/changes/add-macos-settings-navigation/tasks.md`
  - Mark tasks complete only after the corresponding implementation and verification passes.

## Task 1: Model Tests First

**Files:**
- Modify: `clients/apple/scripts/test-shell-settings-surface.swift`

- [ ] **Step 1: Replace old navigation tests with failing four-group tests**

Add or update focused tests so the expected order is:

```swift
try expect(
    snapshot.navigationGroups.map(\.id) == [.general, .terminal, .agent, .system],
    "settings navigation groups must use General, Terminal, Agent, and System"
)
```

Add row membership expectations:

```swift
try expect(
    terminal.rows.map(\.id).contains("terminalProfilesDefault"),
    "Terminal must contain the default Terminal Profile row"
)
try expect(
    terminal.rows.map(\.id).contains("terminalAccountProvision")
        || terminal.rows.contains { $0.id.hasPrefix("terminalAccount.") },
    "Terminal must contain Managed Terminal Account rows"
)
try expect(
    terminal.rows.map(\.id).contains("terminalAccountLoginBoundary"),
    "Terminal must contain the Mac login session boundary row"
)
try expect(
    terminal.rows.map(\.id).contains("terminalProfilesSudoGuidance"),
    "Terminal must contain sudo behavior guidance"
)
```

Add Agent membership expectations:

```swift
try expect(
    agent.rows.map(\.id).contains("agentSelector"),
    "Agent must expose the currently configurable Alan agent"
)
try expect(
    agent.rows.map(\.id).contains("selectedProfile")
        || agent.rows.map(\.id).contains("accountsUnavailable"),
    "Agent must contain provider connection state"
)
try expect(
    ["governance", "reasoningEffort", "streamingMode", "recoveryMode"].allSatisfy(agent.rows.map(\.id).contains),
    "Agent must contain session runtime defaults"
)
try expect(
    agent.rows.map(\.id).contains("enabledSkills")
        || agent.rows.map(\.id).contains("capabilitiesUnavailable"),
    "Agent must contain skill catalog state"
)
try expect(
    agent.rows.map(\.id).contains("publicSkills"),
    "Agent must contain the skill package path row"
)
try expect(
    agent.rows.map(\.id).contains("cliTool"),
    "Agent must contain the command line tool entry point"
)
```

Add System membership expectations:

```swift
try expect(
    ["appIdentity", "installChannel", "daemonEndpoint", "updates", "dataRoot",
     "applicationSupport", "shellControl", "performanceDiagnostics",
     "performanceDiagnosticsExport"].allSatisfy(system.rows.map(\.id).contains),
    "System must contain app, runtime, storage, shell control, and diagnostics rows"
)
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash clients/apple/scripts/test-shell-settings-surface.sh
```

Expected: FAIL because `.agent` and `.system` groups and `agentSelector` do not exist yet.

## Task 2: Group Model Implementation

**Files:**
- Modify: `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift`

- [ ] **Step 1: Replace navigation group cases**

Use:

```swift
enum ShellSettingsNavigationGroup: String, CaseIterable, Equatable, Identifiable {
    case general
    case terminal
    case agent
    case system
}
```

Default order:

```swift
static let defaultOrder: [ShellSettingsNavigationGroup] = [
    .general,
    .terminal,
    .agent,
    .system,
]
```

Titles/icons:

```swift
case .general: "General" / "slider.horizontal.3"
case .terminal: "Terminal" / "terminal"
case .agent: "Agent" / "sparkles"
case .system: "System" / "gearshape.2"
```

- [ ] **Step 2: Add group-section model**

Add:

```swift
enum ShellSettingsGroupSectionID: String, Equatable, Identifiable {
    case interface
    case profiles
    case localIdentity
    case agent
    case connection
    case runtimeDefaults
    case skills
    case skillSources
    case entryPoints
    case app
    case localRuntime
    case storage
    case diagnostics
}

struct ShellSettingsGroupSectionModel: Identifiable, Equatable {
    let id: ShellSettingsGroupSectionID
    let rows: [ShellSettingsRowModel]
}
```

Add user-facing section titles on `ShellSettingsGroupSectionID`.

- [ ] **Step 3: Build group sections by row ID**

Add helper methods on `ShellSettingsSurfaceSnapshot`:

```swift
private var rowsByID: [String: ShellSettingsRowModel] {
    Dictionary(uniqueKeysWithValues: sections.flatMap(\.rows).map { ($0.id, $0) })
}

private func section(
    _ id: ShellSettingsGroupSectionID,
    rowIDs: [String],
    rowsByID: [String: ShellSettingsRowModel]
) -> ShellSettingsGroupSectionModel? {
    let rows = rowIDs.compactMap { rowsByID[$0] }
    guard !rows.isEmpty else { return nil }
    return ShellSettingsGroupSectionModel(id: id, rows: rows)
}
```

The only new row is:

```swift
private static func agentSelectorRow() -> ShellSettingsRowModel {
    ShellSettingsRowModel(
        id: "agentSelector",
        systemName: "sparkles",
        title: "Agent",
        detail: "Alan is the currently configurable agent.",
        value: "Alan"
    )
}
```

- [ ] **Step 4: Map the four groups**

General:

```swift
.interface: ["appearance", "sidebar", "inactiveSplitDimming"]
```

Terminal:

```swift
.profiles: ["terminalProfilesDefault", "terminalProfilesCreate", dynamic terminalProfile.* rows, "terminalProfilesRecovery"]
.localIdentity: terminalAccount* rows + ["terminalAccountProvision", "terminalAccountLoginBoundary", "terminalProfilesSudoGuidance"]
```

Agent:

```swift
.agent: ["agentSelector"]
.connection: ["accountsUnavailable"] or ["selectedProfile", "provider", "model", "credential", "accountActions"]
.runtimeDefaults: ["governance", "reasoningEffort", "streamingMode", "recoveryMode"]
.skills: ["capabilitiesUnavailable"] or ["enabledSkills", "implicitInvocation", "unavailableSkills"]
.skillSources: ["publicSkills"]
.entryPoints: ["cliTool"]
```

System:

```swift
.app: ["appIdentity", "installChannel", "updates"]
.localRuntime: ["daemonEndpoint", "applicationSupport", "shellControl"]
.storage: ["dataRoot"]
.diagnostics: ["performanceDiagnostics", "performanceDiagnosticsExport"]
```

- [ ] **Step 5: Rename row title**

Change the `publicSkills` row title from `Public skills` to `Skill Packages`.

- [ ] **Step 6: Run model tests**

Run:

```bash
bash clients/apple/scripts/test-shell-settings-surface.sh
```

Expected: PASS.

## Task 3: UI Layout and Typography Polish

**Files:**
- Modify: `clients/apple/alan-macos/TerminalPaneView.swift`

- [ ] **Step 1: Update group view types**

Change `ShellSettingsGroupView` and `ShellSettingsSectionView` to consume `ShellSettingsGroupSectionModel` instead of old storage sections.

- [ ] **Step 2: Add Alan-only selector accessory**

In `rowView(_:)`, special-case `agentSelector`:

```swift
case "agentSelector":
    ShellSettingsRow(row: row) {
        ShellSettingsAgentSelector()
    }
```

Add `ShellSettingsAgentSelector` as a compact single-option segmented control:

```swift
private struct ShellSettingsAgentSelector: View {
    var body: some View {
        Text("Alan")
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(ShellPalette.ink)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                    .fill(ShellPalette.panel.opacity(0.86))
            )
            .overlay(
                RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                    .stroke(ShellPalette.line.opacity(0.24), lineWidth: 0.8)
            )
    }
}
```

- [ ] **Step 3: Tighten navigation layout**

Use stable dimensions:

```swift
.frame(width: 152, alignment: .topLeading)
.padding(.leading, 22)
.padding(.trailing, 12)
.padding(.vertical, 22)
```

Navigation rows:

```swift
.font(.system(size: 12, weight: .medium))
.padding(.horizontal, 8)
.padding(.vertical, 6)
```

- [ ] **Step 4: Tighten content hierarchy**

Group title:

```swift
.font(.system(size: 17, weight: .semibold))
```

Section spacing:

```swift
VStack(alignment: .leading, spacing: 16)
VStack(alignment: .leading, spacing: 14)
```

Content width:

```swift
.frame(maxWidth: 760, alignment: .leading)
.padding(.horizontal, 30)
.padding(.vertical, 24)
```

- [ ] **Step 5: Improve row alignment**

In `ShellSettingsRow`, keep icons, text, and accessories aligned:

```swift
.frame(minHeight: 58)
```

Use fixed accessory/value area:

```swift
.frame(width: 180, alignment: .trailing)
```

Keep title/detail fonts compact:

```swift
title: .system(size: 13, weight: .semibold)
detail: .system(size: 11, weight: .medium)
value: .system(size: 12, weight: .semibold)
```

- [ ] **Step 6: Run compile-oriented tests**

Run:

```bash
bash clients/apple/scripts/test-shell-settings-surface.sh
bash clients/apple/scripts/test-shell-runtime-metadata.sh
```

Expected: both PASS.

## Task 4: Contract Checks and OpenSpec Tasks

**Files:**
- Modify: `clients/apple/scripts/check-shell-contracts.sh`
- Modify: `openspec/changes/add-macos-settings-navigation/tasks.md`

- [ ] **Step 1: Update structural contract checks**

Keep checks for default `selectedGroup`, internal navigation, selected group rendering, and no `ForEach(snapshot.sections)`.

Add checks for `.agent`, `.system`, `ShellSettingsGroupSectionModel`, and `agentSelector`.

- [ ] **Step 2: Run contract and OpenSpec validation**

Run:

```bash
bash clients/apple/scripts/check-shell-contracts.sh
openspec validate add-macos-settings-navigation --strict
```

Expected: both PASS.

- [ ] **Step 3: Mark completed tasks**

Mark 1.1-1.6, 2.4-2.6, 3.1-3.7, 4.1-4.4 complete after their commands pass. Leave 4.5 open until visual verification. Leave 5.2 and 5.3 open because they are merge/archive tasks.

## Task 5: Build and Visual Verification

**Files:**
- No code changes expected.

- [ ] **Step 1: Build Debug**

Run:

```bash
xcodebuild \
  -project clients/apple/alan-macos.xcodeproj \
  -scheme alan-macos \
  -configuration Debug \
  -destination 'generic/platform=macOS' \
  -derivedDataPath debug/DerivedData/add-macos-settings-navigation \
  build
```

Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 2: Relaunch Alan Dev fresh**

Use the dev channel, not stable. Avoid `/tmp` build locations. If `just install-dev` cannot overwrite a root-owned app bundle, launch the built app bundle with an isolated `ALAN_MACOS_APPLICATION_SUPPORT_DIR`.

- [ ] **Step 3: Verify UI by inspection**

Verify:

- Left nav shows General, Terminal, Agent, System.
- General opens by default.
- Agent group contains Alan selector, connection, runtime defaults, skills, skill package path, command line tool.
- System group contains app/update, daemon, storage, shell state/control, diagnostics.
- Text baselines, icon columns, row values, and switches are aligned.
- Long Agent/System content scrolls without overlapping or cramped typography.

- [ ] **Step 4: Update tasks**

After visual verification, mark 4.5 and 5.1 complete. Keep 5.2/5.3 open until merge/archive.

## Self-Review

- Spec coverage: Tasks cover the four-group IA, Agent selector, Terminal independence, Agent row ownership, System row ownership, skill path rename, UI polish, tests, OpenSpec validation, build, and fresh Alan Dev visual verification.
- Placeholder scan: no unresolved placeholder markers remain.
- Type consistency: plan consistently uses `ShellSettingsNavigationGroup`, `ShellSettingsGroupSectionID`, `ShellSettingsGroupSectionModel`, and `ShellSettingsNavigationGroupModel`.
