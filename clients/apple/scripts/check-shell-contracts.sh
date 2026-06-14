#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

require_pattern() {
    local file="$1"
    local pattern="$2"
    local message="$3"

    if ! grep -Eq "$pattern" "$REPO_ROOT/$file"; then
        printf 'error: %s\n' "$message" >&2
        printf '       expected pattern %s in %s\n' "$pattern" "$file" >&2
        exit 1
    fi
}

reject_pattern() {
    local file="$1"
    local pattern="$2"
    local message="$3"

    if grep -ERq "$pattern" "$REPO_ROOT/$file"; then
        printf 'error: %s\n' "$message" >&2
        printf '       rejected pattern %s in %s\n' "$pattern" "$file" >&2
        exit 1
    fi
}

reject_active_shell_radius_drift() {
    local matched=0
    local file

    for file in \
        "clients/apple/alan-macos/MacShellRootView.swift" \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "clients/apple/alan-macos/TerminalHostView.swift"
    do
        if grep -En 'RoundedRectangle\(cornerRadius: (1[4-9]|[2-9][0-9])|cornerRadius = (1[4-9]|[2-9][0-9])' "$REPO_ROOT/$file" >&2; then
            matched=1
        fi

        if grep -En 'Capsule\(style: \.continuous\)' "$REPO_ROOT/$file" >&2; then
            matched=1
        fi
    done

    if [[ "$matched" -ne 0 ]]; then
        printf 'error: active macOS shell chrome must use ShellRadii tokens and avoid default Capsule chrome\n' >&2
        exit 1
    fi
}

reject_ghosttykit_umbrella_modulemap() {
    local modulemap
    while IFS= read -r -d '' modulemap; do
        if grep -q 'umbrella header "ghostty\.h"' "$modulemap"; then
            printf 'error: GhosttyKit modulemap must use header "ghostty.h" instead of umbrella header "ghostty.h"\n' >&2
            printf '       offending modulemap: %s\n' "$modulemap" >&2
            exit 1
        fi
    done < <(find -L "$REPO_ROOT/clients/apple/GhosttyKit.xcframework" -name module.modulemap -type f -print0)
}

reject_keydown_programmatic_text_delivery() {
    local file="$REPO_ROOT/clients/apple/alan-macos/TerminalHostView.swift"

    if ! awk '
        /override func keyDown\(with event: NSEvent\) \{/ {
            in_keydown = 1
            depth = 1
            next
        }

        in_keydown {
            if ($0 ~ /sendProgrammaticText[[:space:]]*\(/) {
                print FILENAME ":" FNR ":" $0 > "/dev/stderr"
                matched = 1
            }

            opens = gsub(/\{/, "{")
            closes = gsub(/\}/, "}")
            depth += opens - closes
            if (depth <= 0) {
                in_keydown = 0
            }
        }

        END {
            exit matched ? 1 : 0
        }
    ' "$file"; then
        printf 'error: TerminalHostView.keyDown must not use programmatic text delivery\n' >&2
        exit 1
    fi
}

require_quick_terminal_peak_nonactivating_panel() {
    local file="clients/apple/alan-macos/App/AlanMacPrimaryShellOwner.swift"

    reject_pattern \
        "$file" \
        "NSApp\\.activate\\(ignoringOtherApps:" \
        "quick terminal Peak must not activate the whole app when surfacing its detached panel"

    require_pattern \
        "$file" \
        "\\.nonactivatingPanel" \
        "quick terminal Peak panel must be non-activating so it can surface without raising Alan workspace UI"

    require_pattern \
        "$file" \
        "orderFrontRegardless\\(\\)" \
        "quick terminal Peak panel must order itself forward without depending on app activation"
}

require_title_bar_full_width_hit_area() {
    local file="$REPO_ROOT/clients/apple/alan-macos/TerminalPaneView.swift"

    if ! awk '
        /private struct ShellPaneTitleBarView: View/ {
            in_view = 1
        }

        in_view && /var body: some View/ {
            in_body = 1
        }

        in_body && /ViewThatFits\(in: \.horizontal\)/ {
            saw_responsive_layout = 1
        }

        in_body && /frame\(maxWidth: \.infinity, alignment: \.leading\)/ {
            saw_full_width_frame = 1
        }

        in_body && /background\(ShellPalette\.terminal\)/ {
            if (saw_responsive_layout && saw_full_width_frame) {
                found = 1
            }
        }

        in_body && /private func titleBarContent/ {
            in_body = 0
        }

        END {
            exit found ? 0 : 1
        }
    ' "$file"; then
        printf 'error: pane title-bar background and focus hit area must span full pane width\n' >&2
        exit 1
    fi
}

require_pane_title_bar_trailing_close() {
    local file="$REPO_ROOT/clients/apple/alan-macos/TerminalPaneView.swift"

    if ! awk '
        /private func titleBarContent\(presentation: ShellPaneTitleBarPresentation\) -> some View/ {
            in_content = 1
        }

        in_content && /Spacer\(minLength: 0\)/ {
            saw_spacer = 1
        }

        in_content && /closeButton/ {
            saw_close = 1
            if (saw_spacer) {
                found = 1
            }
        }

        in_content && /^    private var titleView/ {
            in_content = 0
        }

        END {
            exit found ? 0 : 1
        }
    ' "$file"; then
        printf 'error: pane title-bar close button must stay pinned to the trailing edge\n' >&2
        exit 1
    fi
}

require_restored_transcript_full_width_layout() {
    local file="$REPO_ROOT/clients/apple/alan-macos/TerminalPaneView.swift"

    if ! awk '
        /private struct RestoredTerminalTranscriptView: View/ {
            in_view = 1
        }

        in_view && /GeometryReader/ {
            saw_geometry_reader = 1
        }

        in_view && /ScrollView\(\[\.vertical, \.horizontal\]\)/ {
            saw_two_axis_scroll = 1
        }

        in_view && /frame\(minWidth: proxy\.size\.width, alignment: \.topLeading\)/ {
            saw_viewport_min_width = 1
        }

        in_view && /private func paneMoveActionID/ {
            in_view = 0
        }

        END {
            exit saw_geometry_reader && saw_two_axis_scroll && saw_viewport_min_width ? 0 : 1
        }
    ' "$file"; then
        printf 'error: restored transcript scroll content must fill the pane viewport before horizontal scrolling\n' >&2
        exit 1
    fi
}

require_split_terminal_full_pane_layout() {
    local file="$REPO_ROOT/clients/apple/alan-macos/TerminalPaneView.swift"

    if ! awk '
        /private struct ShellSplitLayoutView: View/ {
            in_split_layout = 1
        }

        in_split_layout && /frame\(maxHeight: \.infinity, alignment: \.topLeading\)/ {
            split_max_height += 1
        }

        in_split_layout && /frame\(maxWidth: \.infinity, alignment: \.topLeading\)/ {
            split_max_width += 1
        }

        in_split_layout && /private struct ShellSplitDividerView: View/ {
            in_split_layout = 0
        }

        /private struct ShellTerminalLeafView: View/ {
            in_terminal_leaf = 1
        }

        in_terminal_leaf && /frame\(maxWidth: \.infinity, maxHeight: \.infinity, alignment: \.topLeading\)/ {
            saw_terminal_leaf_full_frame = 1
        }

        in_terminal_leaf && /private struct RestoredTerminalTranscriptView: View/ {
            in_terminal_leaf = 0
        }

        END {
            exit split_max_height >= 2 && split_max_width >= 2 && saw_terminal_leaf_full_frame ? 0 : 1
        }
    ' "$file"; then
        printf 'error: split terminal panes must keep child terminals expanded and top-leading aligned\n' >&2
        exit 1
    fi
}

require_workspace_color_ownership_contract() {
    local pane_file="$REPO_ROOT/clients/apple/alan-macos/TerminalPaneView.swift"

    require_pattern \
        "clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
        "static let rootBacking = Color\\.shellAdaptive\\(" \
        "shell root backing must have a dedicated adaptive opaque token"

    require_pattern \
        "clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
        "light: \\(1\\.0, 1\\.0, 1\\.0\\)" \
        "shell root backing light color must resolve to rgb(1,1,1)"

    reject_pattern \
        "clients/apple/alan-macos/MacShellRootView.swift" \
        "ShellMaterialBackgroundView\\(\\.windowBackdrop\\)" \
        "mac shell root must use the opaque root backing instead of root-window material"

    require_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "ShellEmptyWorkspacePlaceholder" \
        "empty Space must render through a workspace placeholder instead of inline terminal chrome"

    reject_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "terminalTreeOwnsOuterSurface" \
        "terminal-only pane trees must not need special outer terminal surface ownership"

    reject_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "terminalLeafOwnsSurfaceFrame" \
        "mixed content trees must not let terminal leaves own rounded frame chrome"

    require_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "ShellWorkspacePanelFrame" \
        "workspace panes must define a generic panel frame for all content kinds"

    reject_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "ShellTerminalSurfaceFrame" \
        "terminal content leaves must not own rounded frame chrome"

    if awk '
        /private var paneCanvas: some View/ {
            in_canvas = 1
        }

        in_canvas && /ShellTerminalSurfaceFrame/ {
            found = 1
        }

        in_canvas && /private var displayTab/ {
            in_canvas = 0
        }

        END {
            exit found ? 0 : 1
        }
    ' "$pane_file"; then
        printf 'error: workspace paneCanvas must not own ShellTerminalSurfaceFrame\n' >&2
        exit 1
    fi

    if ! awk '
        /private var paneCanvas: some View/ {
            in_canvas = 1
        }

        in_canvas && /shellWorkspacePanelFrame/ {
            found = 1
        }

        in_canvas && /private var displayTab/ {
            in_canvas = 0
        }

        END {
            exit found ? 0 : 1
        }
    ' "$pane_file"; then
        printf 'error: workspace paneCanvas must own ShellWorkspacePanelFrame\n' >&2
        exit 1
    fi
}

require_active_complex_split_count_contrast() {
    local file="$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift"

    if ! awk '
        /private var complexCountOverlay: some View/ {
            in_overlay = 1
        }

        in_overlay && /foregroundStyle\(complexCountForeground\)/ {
            overlay_uses_helper = 1
        }

        in_overlay && /^    private func segmentButton/ {
            in_overlay = 0
        }

        /private var complexCountForeground: Color/ {
            has_helper = 1
        }

        END {
            exit overlay_uses_helper && has_helper ? 0 : 1
        }
    ' "$file"; then
        printf 'error: active complex split indicator count must use a light foreground on selected accent fill\n' >&2
        exit 1
    fi
}

require_tab_organization_sidebar_contract() {
    local file="$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "ShellSidebarTabDragState\\.dragThreshold" \
        "tab rows must use an explicit drag threshold so short clicks keep selecting tabs"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "ShellSidebarTabInsertionLine" \
        "tab row drag/drop must expose a direct insertion preview"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "static let hitHeight: CGFloat = 16" \
        "temporary tab divider hover target must stay compact at 16pt high"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "ShellSidebarTabControlMetrics\\.horizontalInset" \
        "temporary tab divider must be inset from tab-row edges instead of spanning the full row"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "ShellSidebarTabListMetrics\\.itemSpacing" \
        "sidebar tab list must use a shared spacing token for section rhythm"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "static let height: CGFloat = 36" \
        "sidebar tab rows must stay compact at 36pt high"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "static let horizontalInset: CGFloat = 8" \
        "sidebar tab rows must use compact 8pt internal horizontal padding"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "static let titleSize: CGFloat = 14" \
        "sidebar tab row titles must stay readable at 14pt"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "VStack\\(alignment: \\.leading, spacing: subtitle == nil \\? 0 : 1\\)" \
        "sidebar tab title and subtitle spacing must stay tight at 1pt"

    if ! awk '
        /case \.hover:/ {
            in_hover = 1
            next
        }

        in_hover && /return ShellRadii\.control/ {
            hover_found = 1
            in_hover = 0
        }

        /case \.selected:/ {
            in_selected = 1
            next
        }

        in_selected && /return ShellRadii\.row/ {
            selected_found = 1
            in_selected = 0
        }

        END {
            exit hover_found && selected_found ? 0 : 1
        }
    ' "$file"; then
        printf 'error: sidebar tab row hover and selected corners must use compact ShellRadii.control/row treatment\n' >&2
        exit 1
    fi

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "static let sliderToListLift: CGFloat = 12" \
        "sidebar tab list must leave a compact 12pt lift below the space slider"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "\\.padding\\(\\.bottom, ShellSidebarTabListMetrics\\.itemSpacing\\)" \
        "space slider bottom spacing must match tab-list rhythm"

    if ! awk '
        /private struct ShellCompactEmptyAction: View/ {
            in_action = 1
        }

        in_action && /\.contentShape\(Rectangle\(\)\)/ {
            found = 1
        }

        in_action && /private var visualState:/ {
            in_action = 0
        }

        END {
            exit found ? 0 : 1
        }
    ' "$file"; then
        printf 'error: New Tab row must expose a full-row rectangular hover and click hit area\n' >&2
        exit 1
    fi

    require_pattern \
        "clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
        "static let edgeInset: CGFloat = 8" \
        "sidebar edge inset must keep Arc-style tab rows compact at 8pt"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "\\.tabToSpace\\(" \
        "tab context menus must target the clicked tab when moving to another space"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
        "mutationIndex\\(for: insertionTarget, activeDrag: activeDrag\\)" \
        "same-section downward tab drops must convert preview index to mutation index"

    require_pattern \
        "clients/apple/alan-macos/Services/Shell/ShellSocketServer.swift" \
        "\\.tabReorder" \
        "tab.reorder socket commands must route through the host so pin snapshots persist"

    if grep -Eq 'Text\("(Pinned|Unpinned)"\)' "$file"; then
        printf 'error: tab organization sections must avoid heavy visible section headers\n' >&2
        exit 1
    fi
}

require_semantic_terminal_actions_contract() {
    require_pattern \
        "clients/apple/alan-macos/TerminalSurfaceController.swift" \
        "struct AlanTerminalCommandSegment" \
        "semantic terminal command metadata must have pane-scoped command segment storage"

    require_pattern \
        "clients/apple/alan-macos/TerminalSurfaceController.swift" \
        "enum AlanTerminalCommandBoundaryState" \
        "semantic terminal command actions must model boundary reliability explicitly"

    require_pattern \
        "clients/apple/alan-macos/TerminalSurfaceController.swift" \
        "protocol AlanTerminalCommandBufferEngine" \
        "copy-last-output must read from a pane-owned command buffer range"

    require_pattern \
        "clients/apple/alan-macos/TerminalRuntimeService.swift" \
        "AlanTerminalCommandBufferEngine" \
        "live terminal surface handles must provide command-buffer range reads when reliable ranges exist"

    require_pattern \
        "clients/apple/alan-macos/GhosttyLiveHost.swift" \
        "ghostty_surface_read_text" \
        "live Ghostty surfaces must use Ghostty range text reads for command output copying"

    require_pattern \
        "clients/apple/alan-macos/TerminalSurfaceController.swift" \
        "scrollbackAdapter\\.state\\.metrics\\.mode == \\.normalBuffer" \
        "semantic prompt/output actions must be gated to the normal terminal buffer"

    require_pattern \
        "clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift" \
        "case \\.copyLastCommandOutput, \\.searchLastCommandOutput:" \
        "command-aware terminal actions must stay gated behind reliable command boundaries"

    require_pattern \
        "clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift" \
        "runtime\\.hasReliableSemanticCommands" \
        "semantic terminal commands must resolve only for reliable focused panes"

    require_pattern \
        "clients/apple/scripts/test-terminal-surface-controller.swift" \
        "verifiesSemanticCommandActionsUseReliablePaneBoundaries" \
        "surface controller tests must prove semantic command actions use reliable pane boundaries"

    require_pattern \
        "clients/apple/scripts/test-terminal-surface-controller.swift" \
        "verifiesSemanticCommandFallbacksAndInvalidation" \
        "surface controller tests must prove semantic command fallback and invalidation behavior"

    reject_pattern \
        "clients/apple/alan-macos" \
        "CommandBrowser|CommandBlock|CommandOutputSegment|commandBrowser|commandBlocks|outputSegmentation|visibleCommandBlocks" \
        "semantic terminal MVP must not add command browsers, visible command blocks, or persistent output segmentation"
}

"$SCRIPT_DIR/setup-local-ghosttykit.sh" --check >/dev/null
"$SCRIPT_DIR/check-architecture-maintainability.sh" >/dev/null
reject_active_shell_radius_drift
reject_ghosttykit_umbrella_modulemap
reject_keydown_programmatic_text_delivery
require_quick_terminal_peak_nonactivating_panel
require_title_bar_full_width_hit_area
require_pane_title_bar_trailing_close
require_restored_transcript_full_width_layout
require_split_terminal_full_pane_layout
require_workspace_color_ownership_contract
require_active_complex_split_count_contrast
require_tab_organization_sidebar_contract
require_semantic_terminal_actions_contract

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "hostViewsByContentID" \
    "terminal runtimes must be owned by a content-keyed registry"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "protocol TerminalRuntimeHandle" \
    "terminal runtimes must expose a handle protocol"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "protocol AlanGhosttyProcessBootstrap: AnyObject" \
    "Ghostty initialization must have an injectable process bootstrap boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "final class AlanWindowTerminalRuntimeService" \
    "terminal runtime services must be window-scoped production owners"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "protocol AlanTerminalSurfaceHandle: AnyObject" \
    "terminal panes must be represented by stable service-owned surface handles"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "final class FakeAlanTerminalSurfaceHandle" \
    "runtime service tests must have fake pane surface handles"

require_pattern \
    "clients/apple/scripts/test-terminal-runtime-service.sh" \
    "TerminalRuntimeService.swift" \
    "runtime service behavior tests must compile the service boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "private let runtimeService: AlanTerminalRuntimeService" \
    "terminal runtime registry must delegate runtime authority to the service"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "forTerminalContentID: mount\\.contentID" \
    "terminal runtime registry must resolve service-owned handles by content ID"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "private var handlesByContentID: \\[String: AlanTerminalSurfaceHandle\\]" \
    "terminal runtime service must keep runtime identity content-keyed"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "var registeredContentIDs: Set<String>" \
    "terminal runtime service must expose content-keyed registration state"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "final class AlanTerminalSurfaceController" \
    "terminal surface behavior must be owned by a controller boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "enum AlanTerminalTextCompositionPolicy" \
    "terminal IME composing control-character policy must have an explicit owner"

reject_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "final class AlanTerminalInputAdapter" \
    "terminal input routing must not keep a stale input adapter boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "final class AlanTerminalInputRouter" \
    "terminal focus and pointer sequence policy must be owned by a single input router"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "final class AlanTerminalPointerAdapter" \
    "terminal mouse and pointer behavior must be normalized through a pointer adapter"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "final class AlanTerminalScrollbackAdapter" \
    "terminal scrollback behavior must be normalized through a scrollback adapter"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalNativeScrollViewAdapter.swift" \
    "final class AlanTerminalNativeScrollViewAdapter" \
    "terminal scrollback must have an AppKit scroll view adapter"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "protocol AlanTerminalScrollbackEngine" \
    "terminal scrollback must delegate native row scrolls to a surface engine"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "final class AlanTerminalSearchAdapter" \
    "terminal search state must be pane scoped and adapter-owned"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "protocol AlanTerminalSearchEngine" \
    "terminal search queries must be delegated to a real surface search engine"

require_pattern \
    "clients/apple/scripts/test-terminal-surface-controller.swift" \
    "verifiesSearchActionsReachSurfaceEngine" \
    "surface controller tests must prove search actions reach the surface engine"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellFindBarView" \
    "pane-scoped Find must render as a real SwiftUI find bar"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell" \
    "alanShellShowsInspector|showsInspector|ShellInspectorView|ShellInspectorSection|InspectorCard|toggleInspector|Show Inspector|Hide Inspector|right-side shell inspector" \
    "default macOS shell must not expose the removed inspector product surface"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell" \
    "show inspector|hide inspector|open inspector|close inspector|toggle inspector" \
    "legacy shell voice commands must not expose inspector commands"

reject_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "handleSearchKeyIfNeeded|current \\+ characters|dropLast\\(\\)" \
    "Find query editing must be owned by the SwiftUI Find bar instead of terminal key capture"

reject_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "Search terminal|Find text in this pane|Type to search this pane" \
    "Find UI must render through ShellFindBarView instead of the passive terminal overlay card"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "ShellSidebarSpaceSlider" \
    "sidebar Space navigation must render through the top Space slider"

reject_pattern \
    "clients/apple/alan-macos" \
    "maximumVisibleSpaces|ShellSidebarSpaceSliderLayout\\.maximumVisibleSpaces" \
    "continuous Space slider must not retain the old 9-Space cap"

reject_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "case low|case medium|case high|visualScale|opacity\\(" \
    "continuous Space slider must not retain count-density tiers or hover scale/fade geometry"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "case fullTitle|case truncatedTitle|case iconOnly" \
    "continuous Space slider must define full, truncated, and icon-only collapse modes"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "distributedItemWidth" \
    "continuous Space slider must distribute Space targets across the full track before minimum-width overflow"

reject_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "fullTitleItemWidth|truncatedTitleItemWidth|width\\(for: mode\\)" \
    "continuous Space slider must not retain fixed maximum Space target widths"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "isHorizontallyScrollable|contentWidth > availableWidth" \
    "continuous Space slider must expose horizontal overflow sizing"

require_pattern \
    "clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
    "sidebarSpaceSliderTrack" \
    "continuous Space slider track must use a dedicated Safari-like gray track token"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "ShellPalette\\.sidebarSpaceSliderTrack" \
    "continuous Space slider track must render with the dedicated gray track token"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "sidebarControl\\.opacity\\(0\\.46\\)" \
    "continuous Space slider track must not use the old too-light sidebar control fill"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift" \
    "resolvedPresentationIconSystemName|presentation_icon" \
    "ShellSpace projection must expose optional Space presentation icon metadata"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "ShellSidebarSpaceSliderWheelIntentState" \
    "adaptive Space slider must route wheel input through a focused horizontal-intent model"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "dragThreshold" \
    "adaptive Space slider must keep press-drag scrub behind an explicit horizontal threshold"

require_pattern \
    "clients/apple/scripts/test-shell-runtime-metadata.swift" \
    "verifiesSpaceCreateAllowsMoreThanNineSpacesAcrossCommandPaths" \
    "runtime tests must prove Space creation can exceed the old slider cap"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "spaceContextMenu\\(" \
    "Space profile selection must be exposed through the Space context menu"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "host\\.setTerminalProfile\\([^\\n]+forSpaceID: space\\.spaceID\\)" \
    "Space context-menu profile actions must target the Space whose menu was opened"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "onContextMenuIntent: cancelScrubPreview" \
    "Space context-menu opening must cancel active scrub preview before settings actions"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderWheelMonitor.swift" \
    "rightMouseDown|modifierFlags\\.contains\\(\\.control\\)" \
    "Space slider context-menu intent must detect right-click and Control-click without stealing the event"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderWheelMonitor.swift" \
    "ShellSidebarSpaceSliderWheelPhaseLessResetScheduler" \
    "Space slider wheel monitor must reset phase-less wheel intent after idle"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesPhaseLessWheelResetSchedulerResetsAfterIdle" \
    "Space slider layout tests must cover phase-less wheel intent reset"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceSliderWheelMonitor.swift" \
    "ShellSidebarTabListWheelForwardingAnchor" \
    "Space slider vertical pass-through wheel input must be forwarded to the active tab list"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "locationX: value\\.location\\.x - ShellSidebarMetrics\\.edgeInset \\+ trackScrollOffsetX" \
    "Space slider drag scrub must account for the horizontal track scroll offset"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesPassThroughWheelForwardingDecision" \
    "Space slider layout tests must cover pass-through wheel forwarding to the tab list"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesReadableSpacesDistributeAcrossTheFullTrack|verifiesTruncatedSpacesDistributeAcrossTheFullTrack|verifiesIconOnlySpacesDistributeUntilMinimumWidth" \
    "Space slider layout tests must prove targets distribute across the full track before minimum-width overflow"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "onMoveCommand|onKeyPress\\(\\.return\\)|onExitCommand" \
    "Space slider must preserve keyboard preview, commit, and Escape cancel entry points"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "spaceDock|createSpaceFromDock|ShellSidebarSpaceHeader" \
    "default sidebar must not keep the old bottom Space dock or header profile surface"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "ShellSidebarNewSpaceControl" \
    "New Space must live in the sidebar titlebar tool group"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "Spacer\\(minLength: ShellSidebarMetrics\\.titlebarToolSpacing\\)" \
    "New Space must be separated from leading titlebar tools and right-aligned"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "\\.padding\\(\\.trailing, ShellSidebarMetrics\\.edgeInset\\)" \
    "right-aligned titlebar New Space must keep the sidebar trailing inset"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "\\.frame\\(width: sidebarPresentation\\.surfaceWidth, alignment: \\.topLeading\\)" \
    "sidebar titlebar controls must align within the sidebar surface width"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "New Space with alan|createAlanSpace|menuIndicator\\(\\.hidden\\)" \
    "Space creation must not expose the removed New Space with alan menu path"

require_pattern \
    "clients/apple/scripts/test-terminal-surface-controller.swift" \
    "verifiesScrollbackActionsReachSurfaceEngine" \
    "surface controller tests must prove scrollback actions reach the surface engine"

require_pattern \
    "clients/apple/scripts/test-terminal-surface-controller.swift" \
    "verifiesPointerRoutingFollowsTerminalMouseModes" \
    "surface controller tests must prove pointer routing follows terminal mouse modes"

require_pattern \
    "clients/apple/scripts/test-terminal-surface-controller.swift" \
    "verifiesPointerButtonMappingMatchesGhostty" \
    "surface controller tests must prove other-button mapping matches Ghostty"

require_pattern \
    "clients/apple/scripts/test-terminal-surface-controller.swift" \
    "verifiesSelectionCopyAndPasteUseController" \
    "surface controller tests must prove copy and paste use controller-owned clipboard paths"

require_pattern \
    "clients/apple/scripts/test-shell-runtime-metadata.swift" \
    "verifiesRuntimeProjectsTerminalStatusIntoPaneMetadata" \
    "shell runtime tests must prove terminal status projects into pane metadata"

require_pattern \
    "clients/apple/scripts/test-shell-runtime-metadata.swift" \
    "verifiesTerminalStatusSummaryPrioritizesExitAndRendererHealth" \
    "shell runtime tests must prove sidebar status prioritizes exit and renderer health"

require_pattern \
    "clients/apple/scripts/test-shell-runtime-metadata.swift" \
    "verifiesSpaceCreateAllowsMoreThanNineSpacesAcrossCommandPaths" \
    "shell runtime tests must prove space.create can exceed the old sidebar Space cap"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellWorkspaceManifestStore.swift" \
    "shell-workspace-" \
    "workspace restore authority must use the ShellWorkspaceManifest store filename"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellWorkspaceManifestStore.swift" \
    "ShellContentWorkspaceManifest\\.self" \
    "workspace manifest restore must prefer content-container manifests"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "makeWorkspaceManifestFromShellState\\(now: Date\\) -> ShellContentWorkspaceManifest" \
    "workspace manifest writes must produce content-container manifests"

require_pattern \
    "clients/apple/scripts/test-shell-runtime-metadata.swift" \
    "persisted content manifest must not dual-write terminal-only panes" \
    "workspace manifest tests must reject terminal-only snapshot dual-write"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "case \\.workspaceManifest:" \
    "shell host startup must have a workspace-manifest restore path"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "ShellWorkspaceMaterializer\\.materialize" \
    "workspace-manifest startup must materialize shell state from the manifest"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacPrimaryShellOwner.swift" \
    "startupMode: \\.workspaceManifest" \
    "primary macOS shell must start from the workspace manifest"

reject_pattern \
    "clients/apple/alan-macos/App/AlanMacPrimaryShellOwner.swift" \
    "startupMode: \\.fresh|startupMode: \\.restorePrevious|restoreShellState|ShellStatePersistenceStore" \
    "primary macOS shell must not restore workspace identity from ShellStateSnapshot"

require_pattern \
    "clients/apple/scripts/test-shell-workspace-manifest.swift" \
    "verifiesMissingManifestCreatesDefaultWithoutMigratingShellState" \
    "workspace manifest tests must prove legacy ShellStateSnapshot is not migrated"

require_pattern \
    "clients/apple/scripts/test-terminal-surface-controller.sh" \
    "TerminalSurfaceController.swift" \
    "surface controller behavior tests must compile the controller boundary"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "struct AlanShellControlCommand: Codable" \
    "shell control-plane protocol DTOs must live in the shell model boundary"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "deliveryCode: String?" \
    "terminal.send_text responses must expose service delivery state"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "case paneSlotID = \"pane_slot_id\"" \
    "terminal.send_text commands must accept PaneSlot convenience targets"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "paneSlots: \\[ShellPaneSlot\\]?" \
    "shell control-plane responses must expose PaneSlot descriptors"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "contentCapabilities: \\[ShellContentCapability\\]?" \
    "shell control-plane responses must expose content capabilities"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellEventStore.swift" \
    "pane_slot.created" \
    "shell events must expose PaneSlot lifecycle creation"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellEventStore.swift" \
    "content.command_rejected" \
    "shell events must expose rejected content-specific commands"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellLocalCommandExecutor.swift" \
    "enum AlanShellLocalCommandExecutor" \
    "shell local command execution must live outside the socket server boundary"

reject_pattern \
    "clients/apple/alan-macos/ShellControlPlane.swift" \
    "enum AlanShellLocalCommandExecutor|struct AlanShellLocalCommandResult" \
    "shell control plane transport must not own local command execution"

require_pattern \
    "clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift" \
    "runtimePhase: delivery.runtimePhase" \
    "terminal.send_text responses must expose the service runtime phase"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "final class MockTerminalRuntimeHandle" \
    "terminal runtime delivery must have a mock handle for contract tests"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "toTerminalContentID contentID: String" \
    "terminal text delivery must go through the runtime registry"

require_pattern \
    "clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift" \
    "terminalContentID: target\\.content\\.contentID" \
    "terminal.send_text must preserve the resolved terminal content target"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "toTerminalContentID: terminalContentID" \
    "shared terminal.send_text commands must use explicit content targets when present"

require_pattern \
    "clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift" \
    "command\\.paneSlotID \\?\\? command\\.paneID" \
    "terminal.send_text must resolve PaneSlot targets before terminal delivery"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "let isCommandSubmission = isCommandSubmissionText\\(text\\)" \
    "text-delivered terminal commands must start foreground command duration tracking"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "private func isCommandSubmissionText\\(_ text: String\\)" \
    "foreground command detection must include pasted/control text submissions"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "if foregroundCommandStartedAt == nil" \
    "foreground command duration tracking must preserve the original command start time"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "queuesWhileActive: true" \
    "text-delivered queued commands must extend foreground command duration tracking while another command is active"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "queuedForegroundCommandSubmissions \\+= commandCount" \
    "foreground command duration tracking must preserve split-submission queued command counts"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "private var queuedForegroundCommandSubmissions = 0" \
    "foreground command duration tracking must retain queued pasted command submissions"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "private func commandSubmissionCount\\(in text: String\\)" \
    "foreground command duration tracking must count newline-delimited pasted commands"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "advanceForegroundCommandTracking" \
    "foreground command duration tracking must re-arm after queued command completion"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "hasQueuedForegroundCommand \\? \\.foregroundCommand : \\.inactive" \
    "queued pasted commands must keep tab activity protected until the final completion"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "\\.id\\(pane\\.paneID\\)" \
    "terminal host views must be keyed by stable pane identity"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellSplitDividerView" \
    "split panes must use an explicit divider instead of visual spacing gaps"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellSplitDividerTint" \
    "split divider tint must stay subtle instead of rendering as a hard line"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellSplitDividerMetrics\\.thickness" \
    "split divider must use an intentional seam thickness instead of a hard 1px line"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellSplitDividerTint\\.shadow" \
    "split divider must use a subtle bevel seam rather than a single flat line"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "dragPreviewRatio" \
    "split divider drag must track the live preview ratio until drag end"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "resizeSplit\\(splitNodeID: node\\.nodeID, ratio: nextRatio, persist: false\\)" \
    "split divider drag previews must not persist every pointer sample"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "resizeSplit\\(splitNodeID: node\\.nodeID, ratio: finalRatio, persist: true\\)" \
    "split divider drag end must persist the final ratio"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "func resizeSplit\\(splitNodeID: String, ratio: Double, persist: Bool = true\\)" \
    "shell split resize must expose a non-persisting preview path"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "applyMutationResult\\(result, publish: persist\\)" \
    "split resize preview persistence must be controlled at mutation application"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "enum ShellPaneSplitDirection" \
    "split commands must model left/right/up/down placement separately from split axis"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "enum ShellSpatialFocusDirection" \
    "spatial focus commands must use explicit left/right/up/down directions"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "enum ShellWorkspaceCommand: String, CaseIterable, Identifiable" \
    "shell workspace commands must remain a centralized shared vocabulary"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "func performShellWorkspaceCommand\\(_ command: ShellWorkspaceCommand\\)" \
    "command UI and terminal-local workspace actions must keep the shared shell command entry point"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "func performShellAction\\(" \
    "native menu, context menu, and shell keyboard actions must enter the shell action registry"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "case \\.splitRight:" \
    "shared shell workspace command routing must be exhaustively owned by ShellHostController"

require_pattern \
    "clients/apple/alan-macos/AlanApp.swift" \
    "AlanMacShellCommands\\(host: primaryShellOwner\\.host, updateController: updateController\\)" \
    "native menu commands must receive the primary shell host"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "CommandMenu\\(\"Shell\"\\)" \
    "split workspace actions must be exposed through a native Shell menu"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "\\.shellActionKeyboardShortcut\\(host\\.shellActionShortcut\\(\\.paneSplitRight\\)\\)" \
    "split right menu shortcut must come from the shell action registry"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "\\.shellActionKeyboardShortcut\\(host\\.shellActionShortcut\\(\\.paneSplitDown\\)\\)" \
    "split down menu shortcut must come from the shell action registry"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "host\\.performShellAction\\(\\.tabClose\\)" \
    "native menu close actions must use the shell action registry"

reject_pattern \
    "clients/apple/alan-macos.xcodeproj/project.pbxproj" \
    "ShellCommandTabView" \
    "deleted floating Ask alan view must stay out of the Xcode project"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "Ask alan\\.\\.\\.|Go to or Command|New alan tab|newAlanTab|openAlanTab" \
    "sidebar must not expose Ask alan or first-party alan tab creation"

reject_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "Ask alan\\.\\.\\.|New alan tab|requestCommandInput|newAlanTab|Command-P" \
    "native menu commands must not expose Ask alan, Command-P command input, or New alan Tab"

reject_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "ShellCommandTabView|commandInputRequestID|toggleCommandInput|isCommandTabPresented|commandInputOpacity|requestCommandInput" \
    "root shell view must not retain floating Ask alan command input plumbing"

reject_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "openAlanTab|requestCommandInput|setCommandInputActive|commandInputActive|newAlanTab" \
    "shell controller must not retain Ask alan or first-party alan tab command paths"

reject_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift" \
    "newAlanTab|commandInputOpen|Ask alan|New alan tab|shell\\.command_input\\.open|shell\\.tab\\.new_alan" \
    "shell action registry must not register removed Ask alan or New alan Tab actions"

reject_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift" \
    "openingAlanTab|creatingAlanSpace|launchTarget: \\.alan|ShellLaunchTarget\\.alan" \
    "shell state mutations must not create first-party alan tabs"

reject_pattern \
    "clients/apple/alan-macos/TerminalHostRuntime.swift" \
    "resolveAlan|alan chat|launchTarget: \\.alan|ShellLaunchTarget\\.alan|case \\.alan" \
    "terminal runtime must not auto-launch first-party alan tabs"

reject_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellAutomationIntents.swift" \
    "AlanCreateAlanTabIntent|createAlanTab|Create Alan Tab" \
    "App Intents must not expose Create Alan Tab"

reject_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellAutomationCommand.swift" \
    "launchTarget: \\.alan|ShellLaunchTarget\\.alan" \
    "automation command models must not encode first-party alan tab creation"

reject_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "spaceOpenAlan" \
    "shell control plane DTOs must not expose first-party alan space creation"

require_pattern \
    "clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
    "collapsedRevealEdgeWidth" \
    "collapsed sidebar reveal must use a narrow edge hot zone token"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "sidebarPanelRevealAnimation" \
    "collapsed sidebar reveal must use a dedicated spring reveal animation"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "sidebarPanelHideAnimation" \
    "collapsed sidebar hide must use a dedicated fast exit animation"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "handleCollapsedSidebarToolbarHover" \
    "collapsed sidebar toolbar controls must keep the floating panel revealed while hovered"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "windowChromeSurface" \
    "collapsed sidebar chrome must publish its floating surface state to AppKit"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarPresentation.swift" \
    "isVisible: false" \
    "traffic lights must hide when the collapsed sidebar surface is hidden"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "floatingSidebarTrafficLightRevealDelay" \
    "floating sidebar traffic lights must not appear ahead of panel reveal timing"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "scheduleFloatingSidebarTrafficLightReveal" \
    "floating sidebar traffic-light visibility must be delayed separately from panel insertion"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "guard !isSidebarPanelRevealed else" \
    "repeated floating sidebar hover enters must not reset visible traffic lights"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "floatingSidebarTrafficLightRevealToken" \
    "floating sidebar traffic-light reveal timing must not share the hover-retention token"

reject_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "frame\\(width: sidebarWidth, height: windowChromeMetrics\\.collapsedRevealHeaderHeight\\)" \
    "collapsed sidebar reveal must not use the full titlebar/header width as a hover zone"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "func routeShellAction\\(_ input: AlanTerminalKeyInput\\) -> ShellKeyboardAction\\?" \
    "terminal input routing must recognize registered shell actions before terminal bindings"

require_pattern \
    "clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "ShellActionRegistry\\.standard\\.keyboardAction" \
    "terminal keyboard shortcuts must map through the shared shell action registry"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "routeShellActionKeyIfNeeded\\(event\\)" \
    "terminal host key equivalents must give alan shell actions priority over Ghostty bindings"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "private let runtimeReporter = TerminalHostRuntimeReporter\\(\\)" \
    "terminal host runtime snapshot publication must be owned by a focused collaborator"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostRuntimeReporter.swift" \
    "snapshotsEqualIgnoringTimestamp" \
    "terminal runtime reporter must preserve timestamp-insensitive snapshot deduplication"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "private let windowObserver = TerminalHostWindowObserver\\(\\)" \
    "terminal host window notifications must be owned by a focused collaborator"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostWindowObserver.swift" \
    "NSWindow\\.didChangeOcclusionStateNotification" \
    "terminal host window observer must keep occlusion changes connected to surface/runtime refresh"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "host\\.performShellAction\\(actionID, target: target, source: \\.terminalHost\\)" \
    "terminal shortcut routing must enter the shared shell action registry handler"

require_pattern \
    "clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift" \
    "func handleControlPlaneCommand\\(_ command: AlanShellControlCommand\\)" \
    "control-plane protocol commands must stay separate from UI command vocabulary while sharing shell mutation authority"

    require_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "ShellWorkspacePanelFrame" \
        "workspace panes must share one outer rounded workspace panel frame"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "pinnedSidebarPresentationProgress" \
    "pinned sidebar collapse must be driven by continuous presentation progress"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarPresentation.swift" \
    "morphingFloatingToPinned" \
    "sidebar presentation must model the floating-to-pinned morph explicitly"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarPresentation.swift" \
    "visibleSurfaceCount" \
    "sidebar presentation model must expose single-surface invariants for pin morph coverage"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "sidebarPresentation\\.chromeSurface" \
    "mac shell root must derive window chrome from the unified sidebar presentation snapshot"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "morphFloatingSidebarToPinned" \
    "pinning a revealed floating sidebar must use a dedicated morph path"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "ShellWindowPlacementAnimationSyncView: View, Animatable" \
    "window chrome placement must receive animated pinned-sidebar progress instead of only final state"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "AnimatablePair<CGFloat, CGFloat>" \
    "window chrome placement must animate both pinned layout progress and floating-to-pinned morph progress"

    require_pattern \
        "clients/apple/alan-macos/Views/Shell/ShellWorkspaceView.swift" \
        "expandedSidebarProgress" \
        "workspace view must expose continuous sidebar progress for workspace panel spacing"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "frame\\(width: sidebarPinnedVisibleWidth" \
    "pinned sidebar must stay mounted while visible width animates"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-presentation.swift" \
    "verifiesFloatingToPinnedMorphKeepsOneVisibleSurface" \
    "sidebar presentation tests must cover floating-to-pinned morph single-surface behavior"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-presentation.sh" \
    "ShellSidebarPresentation\\.swift" \
    "sidebar presentation tests must compile the shared presentation model"

reject_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "if !isSidebarCollapsed \\{" \
    "pinned sidebar must not be conditionally inserted or removed"

    require_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "workspacePanelInsets: EdgeInsets" \
        "terminal pane must receive semantic workspace panel edge insets"

    require_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "padding\\(workspacePanelInsets\\)" \
        "terminal pane must apply state-aware workspace panel edge insets"

    reject_pattern \
        "clients/apple/alan-macos/TerminalHostView.swift" \
        "cornerRadius = ShellRadii\\.workspacePanel" \
        "terminal host view must not apply an inner rounded corner inside the outer workspace panel"

    require_pattern \
        "clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
        "workspacePanelInsets\\(expandedSidebarProgress" \
        "workspace panel insets must support continuous sidebar progress"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSwipeMonitor.swift" \
    "struct ShellSidebarSwipeMonitor" \
    "sidebar swipe monitor must remain the input adapter"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSwipeMonitor.swift" \
    "struct ShellSidebarSwipeUpdate" \
    "sidebar swipe monitor must emit swipe input updates"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceContentPager.swift" \
    "struct ShellSidebarSpaceContentPagerState" \
    "space swipes must use sidebar content pager state"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceContentPager.swift" \
    "sourceIndex" \
    "space pager state must track the authoritative source space index"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceContentPager.swift" \
    "targetIndex" \
    "space pager state must track the adjacent target space index"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceContentPager.swift" \
    "settlementPhase" \
    "space pager state must model drag, commit, and cancel settlement phases"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceContentPager.swift" \
    "renderRadius = 2" \
    "space pager must keep a five-page rendering window around the source"

require_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSpaceContentPager.swift" \
    "clampedDragOffset" \
    "space pager must clamp one gesture to one page plus overdrag"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "spacePager" \
    "sidebar view must own sidebar-local space pager state"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "ShellSidebarSwipeMonitor\\(onUpdate: handleSpaceSwipe\\)" \
    "sidebar view must install the sidebar swipe monitor as its input adapter"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "spaceContentPager" \
    "sidebar view must render sidebar-local space content pager pages"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "commandLauncher" \
    "sidebar-local pager motion must keep the command launcher owned by the sidebar"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "fixedSpaceSlider" \
    "sidebar-local pager motion must keep Space identity and switching as a fixed sidebar control"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "spaceSlider\\(for: spaceID\\(forSpaceAt:|ShellSidebarSpaceSlider\\([^)]*spaceID\\(forSpaceAt:" \
    "Space slider must not be rendered inside per-Space pager pages"

reject_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "spacePager|spacePagerPages|spacePage\\(index:" \
    "mac shell root must not reintroduce root full-window space pager semantics"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "HStack\\(spacing: 0\\)" \
    "mac shell root must keep a stable sidebar/workspace HStack layout"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "ShellWorkspaceView\\(" \
    "mac shell root must render the committed workspace surface"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellWorkspaceView.swift" \
    "tab: host\\.selectedTab" \
    "workspace view must render committed host tab selection"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellWorkspaceView.swift" \
    "spaceID: host\\.selectedSpace\\?\\.spaceID" \
    "workspace view must render committed host space selection"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellWorkspaceView.swift" \
    "selectedPaneID: contentState\\.focusedPaneSlotID" \
    "workspace view must render committed content PaneSlot selection"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "selectedPaneID: String\\?" \
    "terminal pane view must render preview pages without borrowing selected-pane state"

reject_pattern \
    "clients/apple/alan-macos/Support/ShellSidebarSwipeMonitor.swift" \
    "ShellSpaceTransition" \
    "space swipe support must not reintroduce the sidebar-only transition model"

reject_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "ShellSpaceTransition|spaceTransition" \
    "mac shell root must use shared space pager state instead of sidebar-only transition state"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "ShellSidebarSpaceHeaderPager|activeTransition|sourceOffset\\(|targetOffset\\(" \
    "sidebar view must not own independent header/tab-list pager semantics"

require_pattern \
    "clients/apple/scripts/test-shell-window-placement.swift" \
    "verifiesSystemModeClearsExplicitWindowAppearanceImmediately" \
    "window-placement tests must cover immediate reset to system appearance mode"

require_pattern \
    "clients/apple/scripts/test-shell-window-placement.sh" \
    "ShellWindowPlacement\\.swift" \
    "window-placement tests must compile the macOS shell appearance bridge"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "resolvedAppearanceColorScheme" \
    "mac shell root must resolve system appearance into an explicit SwiftUI colorScheme environment"

reject_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "preferredColorScheme" \
    "mac shell appearance switching must not rely on clearing a stale SwiftUI preferredColorScheme preference"

    reject_pattern \
        "clients/apple/alan-macos/TerminalPaneView.swift" \
        "padding\\(ShellWorkspaceMetrics\\.workspacePanelInset\\)" \
        "workspace panel must not apply equal workspace inset when the sidebar is expanded"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellPaneTitleBarView" \
    "visible terminal panes must render a compact pane title bar"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "shellPaneTitleBarTitle" \
    "pane title bars must use a dedicated title helper with terminal-title-first priority"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "func closePaneByID\\(_ paneID: String\\) -> Bool" \
    "pane title-bar close must route through a controller-owned targeted pane close path"

require_pattern \
    "clients/apple/scripts/test-shell-split-model.swift" \
    "verifiesPaneScopedCloseKeepsInactivePaneTargeting" \
    "split model tests must cover pane-scoped close targeting"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellInactivePaneDim" \
    "inactive split panes must use a lightweight dim treatment"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "allowsHitTesting\\(false\\)" \
    "inactive pane dimming must not intercept terminal pointer input"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "@AppStorage\\(\"alanShellDimsInactiveSplitPanes\"\\)" \
    "inactive pane dimming must be backed by a user-default preference"

reject_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "splitChildren" \
    "split panes must not leave a fixed gap between adjacent terminal panes"

reject_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "paneSelectorStrip" \
    "split panes must not show a bottom pane tab strip by default"

reject_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "Color\\.primary\\.opacity\\(0\\.16\\)" \
    "split divider must not render as a high-contrast primary-color line"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "hasTornDownRuntime" \
    "terminal teardown must be idempotent"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostViewSupport.swift" \
    "let isSelected: Bool" \
    "terminal hosts must know whether their pane is selected"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "guard isSelected, pane != nil else \\{ return \\}" \
    "terminal auto-focus must be gated to the selected pane"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostViewSupport.swift" \
    "terminalHostShouldAutoFocusAfterConfigure" \
    "terminal auto-focus must only be requested on initial attachment or selected-pane transitions"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostViewSupport.swift" \
    "previousPaneID != paneID \\|\\| !wasSelected" \
    "terminal auto-focus must not refocus the same selected pane on every SwiftUI update"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "guard !pendingFocusRequest else \\{ return \\}" \
    "terminal auto-focus must coalesce pending first-responder requests"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostViewSupport.swift" \
    "protocol TerminalHostActivationDelegate: AnyObject" \
    "terminal activation must use a narrow class-bound delegate"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "TerminalHostActivationDelegate" \
    "shell host controller must own terminal activation requests"

require_pattern \
    "clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "activationDelegate: TerminalHostActivationDelegate\\?" \
    "terminal runtime registry must thread the weak activation boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "weak var activationDelegate" \
    "registry-owned terminal host views must not strongly retain activation owners"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "targetPaneID\\(forSpaceID: spaceID\\)" \
    "sidebar space selection must resolve a target pane before committing selection"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "targetPaneID\\(forTabID: tabID, in: selectedSpace\\)" \
    "sidebar tab selection must resolve a target pane before committing selection"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "terminalRuntimeRegistry\\.requestFocus\\(for: paneID\\)" \
    "committed sidebar selection must request terminal focus through the runtime registry"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "activityFreshnessNow" \
    "sidebar activity freshness must be driven by state that can invalidate idle rows"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "task\\(id: activityFreshnessRefreshID\\)" \
    "sidebar activity freshness must schedule refreshes for stale/expires deadlines"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "shellEffectiveAttention\\(for: .*now: activityFreshnessNow\\)" \
    "Space slider attention must use the state-driven freshness clock"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "activityFreshnessNow" \
    "pane title activity freshness must be driven by state that can invalidate idle title bars"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "task\\(id: activityFreshnessRefreshID\\)" \
    "pane title activity freshness must schedule refreshes for stale/expires deadlines"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "shellPaneTitleBarDetailProjection\\(" \
    "pane title detail projection must use the state-driven freshness clock"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "now: activityFreshnessNow" \
    "pane title detail projection must pass the state-driven freshness clock"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "private enum ShellSidebarTypography" \
    "sidebar tab typography must use role-based typography tokens"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "private enum ShellPaneTitleTypography" \
    "pane title typography must use role-based typography tokens"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ViewThatFits\\(in: \\.horizontal\\)" \
    "pane title bars must use staged responsive fallback instead of fixed-width accessory columns"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "private enum ShellPaneTitleBarPresentation" \
    "pane title bars must encode full, compact, and minimal presentation tiers"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "private var titleView" \
    "pane title bars must keep title text as a persistent title view"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "Text\\(title\\)" \
    "pane title bars must render the title as text instead of icon-only content"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "minimumTitleWidth" \
    "pane title bars must preserve a minimum text title width before accessory fallback"

reject_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "terminalChromeSelected\\.fill|terminalChrome\\.fill" \
    "pane title bars must not reintroduce selected/unselected overlay fills"

reject_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "accessoryMaxWidth|maxWidth: accessory\\.maxWidth" \
    "pane title-bar accessories must not use fixed max-width columns"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "onFocusSplitPane: \\{ paneID in" \
    "sidebar split indicators must preserve direct clicked-pane targeting"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "segmentButton\\(paneID: paneID\\)" \
    "two-pane sidebar split indicators must render pane-specific actions"

require_pattern \
    "clients/apple/alan-macos/ShellModel.swift" \
    "enum ShellSidebarPaneTopologyKind" \
    "sidebar split topology classification must live in the testable shell model layer"

require_pattern \
    "clients/apple/scripts/test-shell-split-model.swift" \
    "verifiesSidebarSplitTopologyProjection" \
    "split model tests must cover sidebar topology projection"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "complexCountOverlay" \
    "complex split indicators must overlay count on the pane-shaped topology base"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "accessibilityHidden\\(!showsCloseButton\\)" \
    "hidden sidebar close buttons must not remain exposed to accessibility"

reject_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "rectangle\\.split\\.3x1" \
    "complex split indicators must not render icon and count side by side"

reject_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "selectedSpaceID = spaceID" \
    "sidebar space selection must not be view-local-only"

reject_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "selectedTabID = tabID" \
    "sidebar tab selection must not be view-local-only"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "terminalHostDidRequestActivation\\(paneID:" \
    "terminal host mouse events must request pane activation through the delegate"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "override func mouseDown\\(with event: NSEvent\\)" \
    "terminal pointer down events must remain owned by the AppKit terminal host"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "private func routePointer\\(_ input: AlanTerminalPointerInput\\)" \
    "terminal pointer routing must stay behind the AppKit terminal host boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "private func routeScrollWheel\\(_ event: NSEvent\\)" \
    "terminal scroll routing must stay behind the AppKit terminal host boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "override func scrollWheel\\(with event: NSEvent\\)" \
    "terminal scroll wheel events must remain owned by the AppKit terminal host"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "override func keyDown\\(with event: NSEvent\\)" \
    "terminal key events must remain owned by the AppKit terminal host"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "override func performKeyEquivalent\\(with event: NSEvent\\)" \
    "terminal key equivalents must stay behind the AppKit terminal host boundary"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "override func doCommand\\(by selector: Selector\\)" \
    "terminal key-equivalent doCommand redispatch must stay in the AppKit terminal host"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "NSEvent\\.addLocalMonitorForEvents" \
    "terminal host must own Ghostty-style local keyUp and focus-click event monitoring"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "private var terminalInputIsActive: Bool" \
    "terminal focus-transfer routing must combine shell selection with AppKit first-responder state"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "terminal-input-trace\\.log" \
    "terminal input trace logs must write to a stable file by default"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "ALAN_TERMINAL_INPUT_TRACE" \
    "terminal input trace logs must be opt-in by environment"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "ALAN_TERMINAL_INPUT_TRACE_PATH" \
    "terminal input trace logs must support a file path override"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "AlanTerminalInputTraceEnabled" \
    "terminal input trace logs must be opt-in by user defaults"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "synchronize\\(\\)" \
    "terminal input trace defaults must refresh without restarting alan"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "configRefreshInterval" \
    "terminal input trace must bound live default refresh overhead"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "local-leftMouseDown" \
    "terminal focus-click diagnostics must log local monitor routing"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "isFirstResponder: terminalInputIsActive" \
    "focus-only mouse routing must use active terminal input, not raw first-responder state"

require_pattern \
    "clients/apple/scripts/test-terminal-surface-controller.swift" \
    "verifiesTerminalInputRouterOwnsFocusOnlyPointerSequence" \
    "surface controller tests must prove the terminal input router owns focus-only pointer sequences"

reject_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "FocusClickAdapter|focusClickAdapter|consumeSuppressedLeftMouseUp|shouldSuppressLeftMouseDrag" \
    "focus-only pointer sequence state must not live in TerminalHostView"

reject_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "surfaceController\\.sendText" \
    "physical terminal keyboard code must not use a generic text delivery path"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "func sendProgrammaticText" \
    "Ghostty text injection must be named as programmatic text, not physical keyboard input"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "func insertText\\(_ string: Any, replacementRange: NSRange\\)" \
    "terminal IME text insertion must remain owned by the AppKit terminal host"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "shellActionHandler\\?\\(actionID, target\\)" \
    "terminal workspace shortcuts must leave the AppKit host through the shared shell action callback"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostOverlayPresenter.swift" \
    "let overlayCard = AlanTerminalPassiveOverlayView\\(\\)" \
    "passive terminal overlays must use a non-interactive overlay view"

reject_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "onTapGesture\\(perform: onSelect\\)" \
    "terminal leaf selection must not be owned by a SwiftUI tap wrapper"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "window\\.isMovableByWindowBackground = false" \
    "hidden-titlebar shell windows must keep tab and space controls out of the implicit window drag region"

    require_pattern \
        "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
        "contentInteractionTopInset" \
        "window double-click zoom overlay must not cover workspace panel title-bar controls"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "isPointInSidebarChromeBand" \
    "window double-click zoom overlay must include blank sidebar chrome outside real controls"

require_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "width: sidebarWidth" \
    "window double-click zoom hit testing must know the sidebar chrome width"

    require_pattern \
        "clients/apple/scripts/test-shell-window-placement.swift" \
        "verifiesTitlebarOverlayRejectsWorkspacePanelTitleBarHit" \
        "shell window placement tests must prove workspace panel title-bar controls are not intercepted by zoom overlay"

require_pattern \
    "clients/apple/scripts/test-shell-window-placement.swift" \
    "verifiesTitlebarOverlayAcceptsSidebarChromeBlankHit" \
    "shell window placement tests must prove blank sidebar chrome remains a double-click zoom target"

require_pattern \
    "clients/apple/scripts/test-shell-window-placement.swift" \
    "verifiesSidebarSpaceSliderDoesNotTriggerWindowDrag" \
    "shell window placement tests must prove Space slider controls are not intercepted by zoom overlay"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesMoreThanNineSpacesParticipate" \
    "shell sidebar Space slider tests must verify every Space participates"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesIconOnlyCollapseAndOverflowSizing" \
    "shell sidebar Space slider tests must verify icon-only overflow sizing"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesHoverDoesNotChangeGeometry" \
    "shell sidebar Space slider tests must verify hover keeps stable geometry"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesWheelIntentRoutingProtectsVerticalScroll" \
    "shell sidebar Space slider tests must verify horizontal wheel scrub and vertical scroll protection"

require_pattern \
    "clients/apple/scripts/test-shell-sidebar-space-slider-layout.swift" \
    "verifiesScrubCancelRestoresTheSelectedSource" \
    "shell sidebar Space slider tests must verify scrub cancel restores the selected Space"

require_pattern \
    "clients/apple/scripts/test-shell-workspace-manifest.swift" \
    "verifiesContentSpaceIconMetadataRoundTripsSeparatelyFromTerminalProfile" \
    "workspace manifest tests must verify explicit Space icon metadata round-trips outside Terminal Profiles"

require_pattern \
    "clients/apple/scripts/test-shell-workspace-manifest.swift" \
    "verifiesInvalidSpaceIconFallsBackButPreservesManifestEvidence" \
    "workspace manifest tests must verify invalid Space icon metadata falls back without data loss"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "sidebarTrailingTitlebarToolControlFrame" \
    "hidden-titlebar hit testing must model the right-aligned New Space button separately"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "NSWindow\\.didResizeNotification" \
    "hidden-titlebar shell windows must resynchronize traffic-light placement after resize"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "NSWindow\\.willStartLiveResizeNotification" \
    "hidden-titlebar shell windows must start continuous traffic-light sync during live resize"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "liveResizeChromeSyncTimer" \
    "hidden-titlebar shell windows must keep a scoped live-resize chrome sync timer"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "RunLoop\\.main\\.add\\(timer, forMode: \\.eventTracking\\)" \
    "live-resize chrome sync must run in the event-tracking run loop mode"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "NSWindow\\.didEnterFullScreenNotification" \
    "hidden-titlebar shell windows must publish native fullscreen chrome state"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "standardTrafficLightsVisible = false" \
    "native fullscreen must stop reserving titlebar space for hidden traffic lights"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "ShellWindowChromeSurface" \
    "window chrome sync must accept sidebar surface visibility and origin"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "setStandardWindowButtons\\(in: window, hidden: true\\)" \
    "standard traffic lights must hide with a hidden sidebar surface"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "chromeSurfaceOrigin" \
    "standard traffic lights must follow the visible floating sidebar surface origin"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "showsStandardTrafficLights" \
    "window chrome sync must distinguish surface layout from actual traffic-light visibility"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "shouldPrimeInvisibleTrafficLights" \
    "floating sidebar traffic lights must only be made invisible before repositioning when revealing from a hidden state"

reject_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "^        if chromeSurface\\.showsStandardTrafficLights \\{$" \
    "visible pinned sidebar traffic-light movement must not unconditionally prime standard buttons to alpha zero"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "localTrafficLightGroupFrame" \
    "floating sidebar traffic lights must be rechecked after standard button visibility changes"

require_pattern \
    "clients/apple/alan-macos/TerminalHostView.swift" \
    "override var mouseDownCanMoveWindow: Bool \\{ false \\}" \
    "terminal host views must not allow terminal pane clicks to drag the shell window"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostViewSupport.swift" \
    "final class AlanTerminalFallbackCanvasView" \
    "fallback terminal canvas views must explicitly opt out of background window dragging"

require_pattern \
    "clients/apple/alan-macos/Services/Terminal/TerminalHostViewSupport.swift" \
    "override func hitTest\\(_ point: NSPoint\\) -> NSView\\? \\{ nil \\}" \
    "fallback terminal canvas views must be transparent to AppKit hit-testing"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "override var mouseDownCanMoveWindow: Bool \\{ false \\}" \
    "Ghostty canvas views must not allow terminal pane clicks to drag the shell window"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "override func hitTest\\(_ point: NSPoint\\) -> NSView\\? \\{ nil \\}" \
    "Ghostty canvas views must be transparent to AppKit hit-testing"

reject_pattern \
    "clients/apple/alan-macos" \
    "WindowDragGesture\\(\\)" \
    "shell window dragging should rely on movable background regions, not transparent SwiftUI drag overlays"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "let visible = priority\\.isVisible" \
    "Ghostty occlusion bridge must include render-priority visibility in the visible flag"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "canvasView\\.window\\?\\.occlusionState\\.contains\\(\\.visible\\) \\?\\? false" \
    "Ghostty occlusion bridge must derive the visible flag from NSWindow occlusion state"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "ghostty_surface_set_occlusion\\(surface, visible\\)" \
    "GhosttyKit bridge must pass the observed visible state used by this linked Ghostty build"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "windowIsVisible: shellWindowIsVisibleForRendering" \
    "terminal render priorities must use observed shell window visibility instead of a hardcoded visible window"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "NSWindow\\.didChangeOcclusionStateNotification" \
    "shell window visibility must observe occlusion changes for terminal render-priority throttling"

require_pattern \
    "clients/apple/alan-macos/Support/ShellWindowPlacement.swift" \
    "NSWindow\\.didMiniaturizeNotification" \
    "shell window visibility must observe minimization for terminal render-priority throttling"

reject_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "let isOccluded =|isSurfaceOccluded|!isVisible" \
    "GhosttyKit bridge must not invert NSWindow visible state for this linked Ghostty build"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "if let surface = self\\.surface" \
    "Ghostty wakeup ticks must look up the current surface before refreshing"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "private var tickScheduled = false" \
    "Ghostty wakeup ticks must be coalesced so repeated wakeups do not flood the main queue"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "guard markTickScheduledIfNeeded\\(\\) else \\{ return \\}" \
    "Ghostty wakeup ticks must skip scheduling when a tick is already pending"

require_pattern \
    "clients/apple/alan-macos/GhosttyLiveHost.swift" \
    "clearScheduledTick\\(\\)" \
    "Ghostty wakeup ticks must clear their pending marker when the scheduled tick begins"

require_pattern \
    "clients/apple/alan-macos/ShellHostController.swift" \
    "struct ShellWindowContext" \
    "shell host must expose a shell context type for the singleton primary window"

reject_pattern \
    "clients/apple/alan-macos/MacShellRootView.swift" \
    "ShellWindowContext\\.make\\(\\)" \
    "macOS root view must use the app-scoped primary shell owner instead of creating a fresh context"

require_pattern \
    "clients/apple/alan-macos/AlanApp.swift" \
    "Window\\(\"alan\", id: \"main\"\\)" \
    "macOS app scene must use a launch-presented singleton primary Window"

reject_pattern \
    "clients/apple/alan-macos/AlanApp.swift" \
    "WindowGroup\\(\"alan\", id: \"main\"\\)" \
    "macOS primary shell scene must not use a WindowGroup that can miss first-launch presentation"

require_pattern \
    "clients/apple/alan-macos/AlanApp.swift" \
    "defaultLaunchBehavior\\(\\.presented\\)" \
    "macOS primary window must be presented when the app launches without restoration"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "CommandGroup\\(replacing: \\.newItem\\)" \
    "macOS app must replace New Window with a focus/reopen command"

require_pattern \
    "clients/apple/alan-macos/AlanApp.swift" \
    "AlanMacAppStartup\\.acquireSingletonOrTerminate\\(\\)" \
    "macOS app startup must acquire the singleton guard before creating shell state"

require_pattern \
    "clients/apple/alan-macos/AlanAppSingletonGuard.swift" \
    "flock\\(descriptor, LOCK_EX \\| LOCK_NB\\)" \
    "macOS app singleton guard must use an OS-backed exclusive lock"

reject_pattern \
    "justfile" \
    "^app:" \
    "just app must not be reintroduced as the local macOS app workflow"

reject_pattern \
    "justfile" \
    "app-debug-run" \
    "debug app runner recipes must not replace the removed just app workflow"

require_pattern \
    "justfile" \
    "^install:" \
    "just install must remain the local release install workflow"

reject_pattern \
    "scripts/install.sh" \
    "\\.alan/bin" \
    "local install must not write CLI entries under ~/.alan/bin"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "Install Command Line Tools" \
    "direct app installs must expose an explicit command-line tools install action"

require_pattern \
    "clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "defaultInstallDirectory = URL\\(fileURLWithPath: \"/usr/local/bin\"" \
    "direct app command-line tool installer must use a conventional PATH directory instead of ~/.alan/bin"

require_pattern \
    "clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "homebrewManagedCommandLinks" \
    "direct app command-line tool installer must detect existing Homebrew-managed links before creating alternate PATH links"

require_pattern \
    "scripts/install.sh" \
    "has_homebrew_managed_tool_links" \
    "local install must detect existing Homebrew-managed links before creating alternate PATH links"

require_pattern \
    "clients/apple/alan-macos/TerminalHostRuntime.swift" \
    "case \\.shell:" \
    "terminal launch resolution must keep normal shell launch support"

reject_pattern \
    "clients/apple/alan-macos/TerminalHostRuntime.swift" \
    "bundled_resource_binary|repo_debug_binary|repo_release_binary|path_binary|shell_lookup" \
    "terminal launch resolution must not retain first-party alan tab CLI boot strategies"

reject_pattern \
    "clients/apple/alan-macos/TerminalHostRuntime.swift" \
    "\\.alan/bin" \
    "alan launch resolution must not use ~/.alan/bin"

require_pattern \
    "clients/apple/scripts/test-command-line-tool-installer.sh" \
    "AlanCommandLineToolInstaller.swift" \
    "command-line tool installer behavior must have a focused test"

require_pattern \
    "scripts/validate-release-app.sh" \
    "Developer ID Application" \
    "release app validation must require Developer ID signatures"

require_pattern \
    "scripts/validate-release-app.sh" \
    "require_manifest_checksum" \
    "release app validation must compare manifest checksums with embedded binaries"

require_pattern \
    "justfile" \
    "^guard-macos-auto-update:" \
    "macOS auto-update metadata must have a focused guard target"

require_pattern \
    "scripts/check-macos-auto-update-config.sh" \
    "SUPublicEDKey" \
    "macOS auto-update guard must verify the Sparkle public key metadata"

require_pattern \
    "scripts/check-macos-auto-update-config.sh" \
    "https://alanworks\\.app/appcast\\.xml" \
    "macOS auto-update guard must pin the stable appcast URL"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacUpdateController.swift" \
    "SPUStandardUpdaterController" \
    "macOS app must initialize Sparkle through an updater owner"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacUpdateController.swift" \
    "mayPerform updateCheck" \
    "macOS app must prevent Sparkle checks for Homebrew-managed installs"

require_pattern \
    "clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "Check for Updates\\.\\.\\." \
    "macOS app menu must expose Check for Updates..."

require_pattern \
    "clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "brew upgrade --cask alan" \
    "Homebrew-managed update message must point at brew upgrade --cask alan"

require_pattern \
    "clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "resolvingSymlinksInPath" \
    "Homebrew-managed update policy must inspect resolved app bundle paths"

require_pattern \
    "clients/apple/scripts/test-macos-auto-update-policy.swift" \
    "testDirectCommandLinkDoesNotDisableSparkle" \
    "auto-update policy tests must cover direct app command links"

require_pattern \
    "clients/apple/alan-macos/Info.plist" \
    "SUEnableAutomaticChecks" \
    "first Sparkle version must explicitly disable automatic checks"

require_pattern \
    "clients/apple/alan-macos/Info.plist" \
    "SUAutomaticallyUpdate" \
    "first Sparkle version must explicitly disable silent automatic installation"

require_pattern \
    "justfile" \
    "^apple-auto-update-tests:" \
    "macOS auto-update behavior must have a focused test target"

require_pattern \
    "scripts/generate-appcast.sh" \
    "sparkle:edSignature" \
    "appcast generation must include Sparkle EdDSA signature metadata"

require_pattern \
    "scripts/validate-appcast.sh" \
    "ALAN_EXPECTED_ARCHIVE_PATH" \
    "appcast validation must compare archive length and checksum"

require_pattern \
    "scripts/check-deployed-appcast.sh" \
    "Cache-Control" \
    "deployed appcast validation must check cache headers"

require_pattern \
    "scripts/smoke-macos-auto-update.sh" \
    "ALAN_OLD_APP" \
    "auto-update smoke must support older-app verification input"

require_pattern \
    "scripts/smoke-macos-auto-update.sh" \
    "ALAN_EXPECTED_BUILD" \
    "auto-update smoke must validate the appcast build against the new app"

require_pattern \
    "scripts/smoke-macos-auto-update.sh" \
    "ALAN_EXPECTED_VERSION" \
    "auto-update smoke must validate the appcast short version against the new app"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "Recording signed embedded binary checksums" \
    "release assembly must record manifest checksums after embedded binaries are signed"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "CARGO_BUILD_TARGET=\"aarch64-apple-darwin\"" \
    "release assembly must pin the embedded CLI to the Apple Silicon Rust target"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "cargo build .*--target \"\\\$CARGO_BUILD_TARGET\"" \
    "release assembly must build the embedded CLI for the explicit Apple Silicon target"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "CARGO_RELEASE_BIN" \
    "release assembly must copy the target-specific Cargo release binary"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "thin_macho_to_arm64 \"\\\$EMBEDDED_BIN_DIR/\\\$ALAN_CLI_NAME\"" \
    "release assembly must verify the embedded CLI is arm64-only before signing"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "mktemp \"\\\${path}\\.arm64\\.XXXXXX\"" \
    "release assembly must thin universal inputs into a temporary sibling binary"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "mv \"\\\$output_path\" \"\\\$path\"" \
    "release assembly must atomically replace binaries after lipo succeeds"

reject_pattern \
    "scripts/assemble-release-app.sh" \
    "lipo .* -output \"\\\$path\"" \
    "release assembly must not ask lipo to write over its input path"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "Signing Sparkle framework and helper" \
    "release assembly must sign Sparkle nested code before the final app bundle"

require_pattern \
    "scripts/app-bundle-paths.sh" \
    "alan_sparkle_version_dir" \
    "release scripts must resolve the active Sparkle framework version dynamically"

require_pattern \
    "scripts/assemble-release-app.sh" \
    "alan_sparkle_version_dir" \
    "release assembly must not hard-code the Sparkle framework version directory"

reject_pattern \
    "scripts/assemble-release-app.sh" \
    "Versions/B" \
    "release assembly must not hard-code Sparkle.framework/Versions/B"

require_pattern \
    "scripts/validate-release-app.sh" \
    "SPARKLE_AUTOUPDATE" \
    "release app validation must verify Sparkle Autoupdate helper signatures"

require_pattern \
    "scripts/validate-release-app.sh" \
    "require_arm64_macho \"\\\$ALAN_BIN\"" \
    "release app validation must reject non-arm64 embedded CLI binaries"

reject_pattern \
    "scripts/validate-release-app.sh" \
    "Versions/B" \
    "release app validation must not hard-code Sparkle.framework/Versions/B"

reject_pattern \
    "scripts/validate-release-app.sh" \
    "allow-jit|alan-tui\\.entitlements" \
    "release app validation must not require standalone TUI launch entitlements"

require_pattern \
    "scripts/release-env.sh" \
    "ALAN_DEVELOPER_ID_APPLICATION" \
    "release env loader must accept canonical alan signing identity variables"

reject_pattern \
    "scripts/release-env.sh" \
    "APPLE_API_KEY" \
    "release env loader must expose only the Apple ID app-specific-password notarization path"

reject_pattern \
    "scripts/assemble-release-app.sh" \
    "key-id" \
    "release assembly must submit notarization through the keychain profile only"

reject_pattern \
    "scripts/ensure-notary-profile.sh" \
    "key-id" \
    "notary profile setup must use only Apple ID app-specific-password credentials"

require_pattern \
    "scripts/ensure-notary-profile.sh" \
    "notarytool store-credentials" \
    "release automation must be able to create or refresh the notary keychain profile"

require_pattern \
    "scripts/release-check.sh" \
    "ensure-notary-profile.sh" \
    "release-check must validate notarization setup before building"

require_pattern \
    "justfile" \
    "^release:" \
    "just release must provide the public signed/notarized release workflow"

require_pattern \
    "scripts/validate-homebrew-cask.sh" \
    "Contents/Resources/bin/alan" \
    "Homebrew cask validation must check the embedded alan binary link"

require_pattern \
    "scripts/install-channel.sh" \
    "Alan Dev\\.app" \
    "dev channel install contract checks must cover the dev app bundle"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    'DEFAULT_APP_HOME="\$\{HOME:-/Users/\$\{USER:-\$\(id -un\)\}\}"' \
    "UI smoke must derive the installed Alan Dev app root from HOME like install-dev"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    'DEFAULT_APP_PATH="\$DEFAULT_APP_HOME/Applications/Alan Dev\.app"' \
    "UI smoke must default to the installed Alan Dev app for local repeatability"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    "DEFAULT_APP_EXECUTABLE=.*Alan Dev" \
    "UI smoke must default to the Alan Dev executable instead of the temporary release executable"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    'DEFAULT_BUILT_APP_PATH="\$DERIVED_DATA/Build/Products/Debug/Alan Dev\.app"' \
    "UI smoke build mode must default to a repo-local Alan Dev app bundle"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    'SMOKE_BUNDLE_ID="\$\{SMOKE_BUNDLE_ID:-app\.alanworks\.macos\.dev\}"' \
    "UI smoke build mode must default to the Alan Dev bundle id"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    'SMOKE_DISPLAY_NAME="\$\{ALAN_UI_SMOKE_DISPLAY_NAME:-Alan Dev\}"' \
    "UI smoke build mode must default to the Alan Dev display name"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    'ALAN_UI_SMOKE_APP_PATH:-\$DEFAULT_APP_PATH' \
    "UI smoke must keep an app bundle override for CI and one-off built app validation"

require_pattern \
    "clients/apple/scripts/capture-alan-window.swift" \
    'var bundleID = "app\.alanworks\.macos\.dev"' \
    "window capture helper must default to Alan Dev"

require_pattern \
    "clients/apple/scripts/capture-alan-window.swift" \
    "Default: dev" \
    "window capture usage must document Alan Dev as the default channel"

require_pattern \
    "clients/apple/scripts/check-shell-app-intents-metadata.sh" \
    'Build/Products/Debug/Alan Dev\.app' \
    "App Intents metadata check must default to a built Alan Dev app"

require_pattern \
    "clients/apple/scripts/capture-performance-diagnostics-workload.sh" \
    'Build/Products/Debug/Alan Dev\.app' \
    "performance diagnostics workload must default to a built Alan Dev app"

require_pattern \
    "clients/apple/scripts/capture-performance-diagnostics-workload.sh" \
    'Contents/MacOS/Alan Dev' \
    "performance diagnostics workload must default to the Alan Dev executable"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    "ALAN_UI_SMOKE_RESTART_RESTORE_STEPS" \
    "UI smoke must expose an opt-in restart transcript restore path"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    "shell-workspace-window_main\\.json" \
    "UI smoke restart restore must inspect the persisted workspace manifest"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    'frame\(maxWidth: \.infinity, minHeight: 1, maxHeight: 1\)' \
    "temporary tab divider must render as a full-width visible 1pt rule"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "ShellPalette\\.sidebarDivider" \
    "temporary tab divider must use a sidebar-specific adaptive divider color"

require_pattern \
    "clients/apple/alan-macos/Support/ShellDesignTokens.swift" \
    "static let sidebarDivider = Color\\.shellAdaptive" \
    "sidebar divider color must be adaptive for light and dark mode"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    "@State private var isControlHovered = false" \
    "temporary tab divider row must track hover to reveal Clear"

require_pattern \
    "clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift" \
    '\.onHover \{ isControlHovered = \$0 \}' \
    "temporary tab divider row hover must update Clear reveal state"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    "restart-restore" \
    "UI smoke restart restore must capture relaunch evidence"

require_pattern \
    "clients/apple/scripts/test-shell-ui-smoke.sh" \
    'RESTART_RESTORE_CWD="\$OUTPUT_DIR/restart-restore-cwd"' \
    "UI smoke restart restore cwd must follow the selected output directory"

require_pattern \
    "justfile" \
    "^apple-shell-focused-tests:" \
    "focused Apple shell tests must have a single everyday just target"

require_pattern \
    "justfile" \
    "test-shell-runtime-metadata\\.sh" \
    "focused Apple shell tests must include runtime and control-plane metadata coverage"

require_pattern \
    "justfile" \
    "test-shell-settings-surface\\.sh" \
    "focused Apple shell tests must include settings surface coverage"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "@State private var selectedGroup = ShellSettingsNavigationGroup\\.general" \
    "Settings content must default internal navigation selection to General"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift" \
    "case agent" \
    "Settings navigation model must include Agent as a first-class group"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift" \
    "case system" \
    "Settings navigation model must include System as a first-class group"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift" \
    "ShellSettingsGroupSectionModel" \
    "Settings navigation model must support task-oriented group sections"

require_pattern \
    "clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift" \
    "agentSelector" \
    "Settings Agent group must expose the supported Alan agent affordance"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ShellSettingsNavigationView\\(" \
    "Settings content must render a compact internal navigation view"

require_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "selectedGroupModel" \
    "Settings content must render only the selected navigation group"

reject_pattern \
    "clients/apple/alan-macos/TerminalPaneView.swift" \
    "ForEach\\(snapshot\\.sections\\)" \
    "Settings content must not render every section in one continuous scroll"

require_pattern \
    "clients/apple/scripts/check-shell-app-intents-metadata.sh" \
    "AlanCreateTerminalTabIntent" \
    "App Intent metadata review must cover generated Shortcuts action names"

require_pattern \
    "clients/apple/README.md" \
    "just apple-shell-focused-tests" \
    "Apple README must document the aggregate focused shell test target"

require_pattern \
    "clients/apple/README.md" \
    "stable .*window_main.* identity" \
    "Apple client docs must describe the singleton primary shell identity"

reject_pattern \
    "clients/apple/README.md" \
    "Each macOS window creates its own shell context" \
    "Apple client docs must not describe each macOS window as an independent shell context"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellSocketServer.swift" \
    "private static let maxRequestBytes" \
    "socket server must enforce a bounded request size"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellSocketServer.swift" \
    "command_timeout" \
    "socket server must return a stable timeout error"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellSocketServer.swift" \
    "private static let maxConcurrentClients" \
    "socket server must keep concurrency limits in the transport owner"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellPublishedStateMerger.swift" \
    "enum AlanShellPublishedStateMerger" \
    "published state merging must live in a dedicated shell service owner"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellControlFilePoller.swift" \
    "final class AlanShellControlFilePoller" \
    "file-polling control plane must live in a dedicated shell service owner"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellEventStore.swift" \
    "final class AlanShellEventStore" \
    "shell event persistence must live in a dedicated shell service owner"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellDiagnostics.swift" \
    "final class AlanShellDiagnostics" \
    "shell diagnostics routing must live in a dedicated shell service owner"

require_pattern \
    "clients/apple/alan-macos/Services/Shell/ShellPerformanceDiagnostics.swift" \
    "mach_port_deallocate\\(mach_task_self_, threadList\\[index\\]\\)" \
    "performance diagnostics task_threads sampling must deallocate each returned thread port"

reject_pattern \
    "clients/apple/alan-macos/ShellControlPlane.swift" \
    "final class AlanShellSocketServer|SO_RCVTIMEO|SO_SNDTIMEO|maxRequestBytes|command_timeout|enum AlanShellPublishedStateMerger|pollCommands\\(|pollBindings\\(|handleCommandFile\\(|appendEvent\\(|readEvents\\(|recordEvents\\(|recordDiagnostic\\(" \
    "shell control-plane coordinator must not own socket transport bounds, state merging, file polling, event persistence, or diagnostic routing"

reject_pattern \
    "clients/apple/alan-macos" \
    "NotificationCenter\\.default\\.post" \
    "control-plane text delivery must not rely on NotificationCenter broadcast success"

printf 'Shell contract checks passed.\n'
