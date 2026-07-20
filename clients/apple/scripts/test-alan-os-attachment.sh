#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-os-attachment-tests"
MODULE_CACHE_DIR="$BUILD_DIR/clang-module-cache"
TEST_BINARY="$BUILD_DIR/alan-os-attachment-tests"

mkdir -p "$MODULE_CACHE_DIR"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/scripts/support/AlanOSAttachmentTestSupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/AlanOS/AlanOSAttachmentService.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-alan-os-attachment.swift" \
    -o "$TEST_BINARY"

"$TEST_BINARY"

APP_SOURCE="$REPO_ROOT/clients/apple/alan-macos"
ATTACHMENT_SOURCE="$APP_SOURCE/Services/AlanOS/AlanOSAttachmentService.swift"
PANE_LAYOUT_SOURCE="$APP_SOURCE/Views/Shell/Terminal/ShellPaneTreeLayoutView.swift"
AGENT_CONTENT_SOURCE="$APP_SOURCE/Views/Shell/Content/ShellBoundedContentViews.swift"

if grep -ERq 'AlanKernel|AlanAgentEngine|AgentExecutionEngine|bootRootAgent|requestHostStop|stopAlanOSHost|terminateAlanOSHost' "$APP_SOURCE"; then
    echo "Alan for macOS must not own Alan OS or Agent execution lifecycle." >&2
    exit 1
fi
grep -q 'sendAttachRequest(descriptor: descriptor)' "$ATTACHMENT_SOURCE"
grep -q 'host.updateAgentRendererState' "$PANE_LAYOUT_SOURCE"
grep -Fq 'writeDocument("/proc/\(reference.pid)/ctl"' "$ATTACHMENT_SOURCE"
grep -Fq 'writeDocument("/agent/\(reference.pid)/machine/ctl"' "$ATTACHMENT_SOURCE"
grep -Fq '.alert("Stop Agent Process?"' "$AGENT_CONTENT_SOURCE"
grep -Fq 'Closing this view only detaches.' "$AGENT_CONTENT_SOURCE"
grep -Fq 'session.list("/mnt/host-mount/requests")' "$ATTACHMENT_SOURCE"
if grep -Fq 'session.cat("/mnt/host-mount/request")' "$ATTACHMENT_SOURCE"; then
    echo "Alan for macOS must not poll the retired Host Mount request file." >&2
    exit 1
fi
grep -q 'let panel = NSOpenPanel()' "$ATTACHMENT_SOURCE"
grep -q 'approveHostMount(' "$ATTACHMENT_SOURCE"
grep -q 'cancelHostMount(' "$ATTACHMENT_SOURCE"
if grep -q 'dismissedMountRequests' "$ATTACHMENT_SOURCE"; then
    echo "Dismissed native Host Mount panels must settle the service request." >&2
    exit 1
fi
grep -Fq 'session.cat("/mnt/connections/native-requests")' "$ATTACHMENT_SOURCE"
grep -Fq 'host-keychain:\(channel):\(credentialID)' "$ATTACHMENT_SOURCE"
grep -q 'ALAN_NATIVE_CONNECTION_REQUEST_ID' "$ATTACHMENT_SOURCE"
grep -q 'AlanOSAttachmentController.shared.detach()' \
    "$APP_SOURCE/App/AlanMacPrimaryShellOwner.swift"
grep -q 'agent_attachment_persists_only_reference_offsets_and_presentation' \
    "$REPO_ROOT/crates/shell-core/tests/manifest_contract.rs"

echo "Alan OS attachment architecture guards passed."
