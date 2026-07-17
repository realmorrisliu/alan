## Context

`enforce-clean-code-architecture-gates` records 64 Rust source files above
1,000 lines, 15 Apple maintainability warnings, exact crate dependencies, and
the current lint baseline. Those numbers are ceilings, not accepted end state.
The most consequential debt is structural: Agent Execution Engine still knows
about Process namespace/AgentFS composition, while connection profile metadata
and selection are not yet exclusively owned by Connection Service.

This change starts only after the gate PR merges. Its first implementation PR
is the next PR, before unrelated feature work, and every slice remains
behavior-preserving and independently reviewable.

## Goals / Non-Goals

**Goals:**

- Reach an empty Rust oversized-source baseline through ownership-oriented
  extraction rather than cosmetic splitting.
- Reach zero Apple architecture warnings and make strict mode canonical.
- Make Agent Runtime Service the sole owner of Agent Process namespace and
  AgentFS lifecycle composition.
- Make Connection Service the sole owner of profile metadata and selection.
- Remove transitional crate dependency edges as their responsibilities move.
- Tighten executable debt budgets in every PR.

**Non-Goals:**

- Change Alan OS, AgentFS, aP, shell, model-provider, or macOS product behavior.
- Rename public concepts or introduce a second Process/agent ontology.
- Preserve displaced ownership through parallel adapter paths, dual writers,
  generic manager objects, or a new architecture framework.
- Split a file only to satisfy a line counter while leaving its responsibilities
  coupled.

## Decisions

### Decision: The immediate next PR fixes one Connection Service ownership seam

The first implementation PR after the gate moves one complete profile
metadata/selection responsibility out of Agent Execution Engine and into the
existing Connection Service owner. The engine consumes the mounted callable
connection handle selected by its Process launch context; it does not read or
mutate the profile store. The PR deletes the displaced engine path and tightens
the source/dependency baselines it reduces.

This is the first slice because it has a clear durable owner and narrows the
inputs needed by later Agent Runtime Service composition work.

Alternative considered: begin by splitting the largest file mechanically.
Rejected because file movement without an ownership correction would make the
same coupling harder to see.

### Decision: Agent Runtime Service owns assembly; the engine owns transitions

Subsequent runtime slices move Process clone inputs, mount selection, AgentFS
lifecycle wiring, and child namespace assembly behind Agent Runtime Service.
Agent Execution Engine retains tape/model/Tool/policy/memory transition logic
and receives an already assembled Process namespace plus file handles.

Each slice removes a complete responsibility and then removes the corresponding
normal dependency from `alan-agent-engine`; no new facade may preserve the old
direction.

Alternative considered: leave assembly in the engine behind traits. Rejected
because dependency inversion without moving lifecycle authority would conceal,
not fix, the ownership leak.

### Decision: File-size work follows responsibility movement

Oversized files are grouped by owner and split only where a cohesive behavior,
adapter, data model, or adjacent white-box test suite can stand alone. A slice
must improve locality and reduce the checked-in maximum; reaching 1,000 lines
removes the entry entirely. Test files are subject to the same ceiling, using
adjacent extracted white-box suites where private access is required.

Alternative considered: generate submodules by fixed line ranges. Rejected
because it optimizes the metric while preserving shallow modules and implicit
coupling.

### Decision: Apple warnings are removed by warning class

Apple work proceeds as focused ownership slices. Each PR selects a recorded
warning class, moves behavior to the target owner or deletes a shallow bridge,
updates `clients/apple/ARCHITECTURE.md`, and lowers the executable warning
ceiling. When the count reaches zero, report mode and strict mode converge and
the non-zero ledger is removed.

The live report was reconfirmed before Apple source changes began. Its exact
15-warning ledger is classified by durable owner below; the target is the
responsibility boundary, not merely a smaller file with the same coupling.

| Warning | Durable owner and removal boundary | Wave |
| --- | --- | --- |
| `bridge: ShellHostController.swift` | Narrow AppKit services own close confirmation, app activity, and pasteboard access; the controller remains Foundation/Combine orchestration. | 4.2 first slice |
| `large: Controllers/Shell/ShellHostControlCommandHandling.swift` | Control request routing, response projection, terminal delivery, diagnostics, and list projection become named controller/service collaborators. | 4.2 |
| `large: ShellHostController.swift` | The root keeps observable shell orchestration; selection, action, close, runtime projection, and persistence flows move to their existing or named collaborators. | 4.2 |
| `large: ShellModel.swift` | Sidebar tab projection, drag/drop, pane topology, and activity-notification presentation become separate presentation-model owners. | 4.2 |
| `large: Models/Shell/ShellSettingsSurfaceModel.swift` | Settings navigation DTOs, managed-user summary/creation flow, catalog storage, and diagnostics summaries split by settings domain owner. | 4.2 |
| `large: Models/Shell/ShellSnapshots.swift` | Terminal transcript, content, pane-tree, tab/space, and workspace snapshot DTO families become adjacent model modules without duplicating Rust-owned mutation behavior. | 4.2 |
| `large: Models/Shell/ShellValueTypes.swift` | Terminal Profile, privileged-helper, managed-account, and platform-effect value families move beside their owning adapters/services. | 4.2, then 4.4 adapter audit |
| `large: Views/Shell/ShellSidebarView.swift` | Space slider, tab list/drop handling, row chrome, activity rail, and topology indicator become focused SwiftUI presentation owners. | 4.2 |
| `large: TerminalPaneView.swift` | Pane-tree layout, bounded content renderers, settings surface, title bar, find bar, and terminal leaf presentation become focused SwiftUI owners. | 4.2 |
| `large: GhosttyLiveHost.swift` | Ghostty host lifecycle, renderer coordination, canvas view, and platform display lookup stay behind narrow terminal-host adapters. | 4.3 |
| `large: TerminalHostRuntime.swift` | Boot resolution, render coordination, publication policy, and runtime snapshot models split along terminal runtime responsibilities. | 4.3 |
| `large: TerminalHostView.swift` | AppKit terminal view lifecycle, text input, keyboard translation, and input tracing split into host-view adapters. | 4.3 |
| `large: TerminalRuntimeService.swift` | Native/helper PTY runtimes, surface lifecycle, process bootstrap, and test doubles become separate runtime and test-support owners. | 4.3 |
| `large: TerminalSurfaceController.swift` | Scrollback, keyboard/pointer routing, search, selection, metadata, and surface readiness become focused terminal-surface collaborators. | 4.3 |
| `large: Services/Shell/AlanPrivilegedHelperXPC.swift` | Wire DTO/codec, listener/client, managed-user account operations, and managed-user PTY session ownership split at the XPC adapter boundary. | 4.3, then 4.4 adapter audit |

After these warning-bearing owners are split, 4.4 audits the shell-core FFI and
platform adapters for shallow pass-throughs. It may delete debt without a
warning of its own, but it must not reintroduce Apple domain fallbacks or widen
the operation-owner allowlist.

The completed audit keeps reducer and managed-terminal-account operations in
dedicated operation-family adapters. Portable managed-account provisioning and
rollback planning now run in shell-core; Swift projects platform diagnosis and
Terminal Profile DTOs and fails closed when the facade is unavailable. The
shallow reducer command coordinator and optional action metadata fallbacks are
removed without increasing the operation-owner allowlist.

The managed-account diagnosis boundary carries requested-home existence and
configured-home equality as separate facts from the privileged helper through
the Swift adapter, so shell-core can plan repair without re-reading host state.

Alternative considered: keep a permanent warning allowlist. Rejected because
the 15 warnings describe known migration state, not supported architecture.

### Decision: Every PR proves behavior before and after

Before moving a responsibility, the slice identifies its existing public or
white-box contract tests. After the move it runs the canonical quality gate,
the focused owner tests, workspace tests appropriate to the slice, and strict
OpenSpec validation. Baseline reductions land in the same PR. Runtime or UI
behavior changes require a separate OpenSpec change.

Alternative considered: complete one repository-wide rewrite and validate at
the end. Rejected because failures would be difficult to localize and review.

### Decision: File-native generation carries typed content, not private overrides

The llmfs request DTO carries provider-neutral typed message parts through the
Generation `data` file. The llmfs boundary validates attachment capabilities,
and official provider adapters project those parts into their native image and
document representations. Agent Execution Engine does not rebuild the removed
`responses_input_items`, `chat_completions_messages`, or `anthropic_messages`
override paths.

Alternative considered: restore the old provider-specific `extra_params` from
the engine. Rejected because that would preserve behavior by reversing the
namespace ownership correction and would keep provider details above llmfs.

## Risks / Trade-offs

- [Behavior changes during moves] → Characterize the seam first, keep each PR
  single-owner, and delete the old path atomically.
- [Line-count gaming] → Require a named responsibility and narrower review
  surface for every extraction; the metric alone is not completion evidence.
- [Temporary dependency churn] → Never add a transitional edge without an
  explicit spec/ADR change; remove edges as soon as their responsibility moves.
- [Long-running debt program blocks delivery] → Only the first debt PR is an
  unconditional immediate successor; later slices stay independently mergable
  and explicitly prioritized.
- [Apple refactor regresses UI behavior] → Use focused source tests plus a fresh
  Alan Dev build and rendered smoke for touched UI/runtime surfaces.

## Migration Plan

1. Merge `enforce-clean-code-architecture-gates` and sync its specs.
2. Open the immediate Connection Service ownership PR from this change before
   unrelated feature work.
3. Move Agent Runtime Service assembly responsibilities in dependency-removing
   slices, tightening Rust size and graph budgets each time.
4. Split remaining oversized Rust production and test owners by responsibility.
5. Burn down Apple warning classes to zero with fresh Alan Dev verification.
6. Enable strict zero-debt expectations, run full validation, and archive this
   change only after all tasks are complete.

Rollback is per slice: revert the focused behavior-preserving PR and restore
its exact baseline. No persistent user data migration is introduced.

## Open Questions

None. The first slice and durable owners are fixed; exact later file order is
selected from the live tightened baseline after each merge.
