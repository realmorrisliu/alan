# alan - Development Tasks

# List available commands
default:
    @just --list

# Run tests
test:
    cargo test --workspace

# Run the platform-neutral shell workspace core tests
shell-core-test:
    cargo test -p alan-shell-core

# Run shell-core FFI facade and Swift adapter tests
shell-core-ffi-test:
    cargo test -p alan-shell-core-ffi
    bash clients/apple/scripts/test-shell-core-ffi-adapter.sh

# Check code (format + lint + test)
check: fmt lint test
    @echo "✅ All checks passed"

# Format code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Check agent-root layout ownership guardrails
guard-agent-root-layout:
    ./scripts/check-agent-root-layout-strings.sh

# Reject retired host-service architecture from current repository and CLI surfaces
guard-daemon-era-absence:
    cargo build -p alan --bin alan
    ./scripts/check-daemon-era-absence.sh target/debug/alan

# Reject retired macOS persistence, installer, and Managed User compatibility surfaces
guard-legacy-macos-absence:
    ./scripts/check-legacy-macos-absence.sh

# Check macOS Sparkle auto-update project metadata
guard-macos-auto-update:
    ./scripts/check-macos-auto-update-config.sh

# Check macOS shell design-token literals against the recorded baseline
guard-shell-design-tokens:
    ./scripts/check-shell-design-tokens.sh

# Check canonical specs, active changes, and OpenSpec schema instructions
guard-openspec-current-surfaces:
    bash scripts/check-openspec-current-surfaces.sh

# Run OpenSpec guard fixtures, current-surface checks, and strict validation
openspec-check:
    bash scripts/test-openspec-current-surfaces.sh
    bash scripts/check-openspec-current-surfaces.sh
    openspec validate --all --strict

# Run focused macOS auto-update tests and release appcast guards
apple-auto-update-tests:
    bash clients/apple/scripts/test-macos-auto-update-policy.sh
    ./scripts/test-app-bundle-paths.sh
    ./scripts/test-appcast-tools.sh
    ./scripts/check-macos-auto-update-config.sh

# Run clippy
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Show coverage summary in terminal
coverage:
    cargo llvm-cov --workspace --summary-only

# Show detailed coverage with uncovered lines
coverage-detail:
    cargo llvm-cov --workspace

# Generate HTML coverage report (target/coverage/html)
coverage-html:
    cargo llvm-cov --workspace --html --output-dir target/coverage

# Build release
build:
    cargo build --release

# Install release Alan.app plus CLI locally
install:
    ALAN_INSTALL_CHANNEL=stable ./scripts/install.sh

# Install local-only Alan Dev.app plus alan-dev
install-dev:
    ./scripts/install-dev.sh

# Run local side-by-side smoke for stable Alan and Alan Dev
dev-channel-smoke:
    ./scripts/smoke-dev-channel-side-by-side.sh

# Run focused macOS shell tests that do not require real Ghostty artifacts
apple-shell-focused-tests:
    bash clients/apple/scripts/test-shell-workspace-manifest.sh
    bash clients/apple/scripts/test-shell-performance-diagnostics.sh
    bash clients/apple/scripts/test-terminal-runtime-service.sh
    bash clients/apple/scripts/test-terminal-surface-controller.sh
    bash clients/apple/scripts/test-shell-automation-command-seams.sh
    bash clients/apple/scripts/test-shell-runtime-metadata.sh
    bash clients/apple/scripts/test-shell-settings-surface.sh
    bash clients/apple/scripts/test-terminal-account-dev-dry-run-smoke.sh
    bash clients/apple/scripts/test-macos-auto-update-policy.sh
    ./scripts/test-appcast-tools.sh

# Run focused macOS shell automation command seam tests
apple-shell-automation-seams:
    bash clients/apple/scripts/test-shell-automation-command-seams.sh

# Review generated App Intents metadata from a built alan-macos app
apple-shell-app-intents-metadata:
    bash clients/apple/scripts/check-shell-app-intents-metadata.sh

# Run Ghostty-backed macOS shell integration checks when local artifacts are prepared
apple-shell-ghostty-integration:
    bash clients/apple/scripts/test-shell-ghostty-integration.sh

# Run repeatable macOS shell UI smoke against the installed Alan Dev app
apple-shell-ui-smoke:
    bash clients/apple/scripts/test-shell-ui-smoke.sh

# Capture the macOS shell screenshot state matrix (semi-manual, dev channel)
apple-shell-screenshot-matrix out_dir="":
    bash clients/apple/scripts/capture-shell-state-matrix.sh {{out_dir}}

# Check release signing and notarization configuration without building
release-check:
    ALAN_NOTARIZE=1 ./scripts/release-check.sh

# Build, sign, notarize, staple, and archive the public macOS release app
release:
    ALAN_INSTALL_CHANNEL=stable ALAN_NOTARIZE=1 ALAN_CREATE_RELEASE_ARCHIVE=1 ./scripts/assemble-release-app.sh

# Uninstall alan app and user-level CLI without removing ~/.alan data
uninstall:
    ALAN_INSTALL_CHANNEL=stable ./scripts/uninstall.sh

# Uninstall Alan Dev.app and dev command links without removing ~/.alan-dev data
uninstall-dev:
    ./scripts/uninstall-dev.sh

# Clean artifacts
clean:
    cargo clean
    rm -rf target/

# Mock smoke tests (CI safe, no LLM needed)
smoke:
    cargo test -p alan --test smoke_test -- --nocapture

# Live provider protocol harness (ignored tests + explicit opt-in env)
live-providers:
    bash scripts/live-provider-harness.sh

# Live runtime smoke (ignored tests + explicit opt-in env)
live-runtime-smoke:
    bash scripts/live-runtime-smoke.sh

# Run autonomy harness scenarios (all)
harness-autonomy:
    bash scripts/harness/run_autonomy_suite.sh

# Run only CI-blocking autonomy harness scenarios
harness-autonomy-ci:
    bash scripts/harness/run_autonomy_suite.sh --ci-blocking

# Run self-eval profile regression in local mode
self-eval:
    bash scripts/harness/run_self_eval_suite.sh --mode local

# Run self-eval profile regression in CI gate mode
self-eval-ci:
    bash scripts/harness/run_self_eval_suite.sh --mode ci

# Run self-eval profile regression in nightly mode
self-eval-nightly:
    bash scripts/harness/run_self_eval_suite.sh --mode nightly

# Run repo-worker smoke loop
repo-worker-smoke:
    bash scripts/repo-worker/run_smoke.sh --mode local

# Run repo-worker harness scenarios (all)
harness-repo-worker:
    bash scripts/harness/run_repo_worker_suite.sh

# Run only CI-blocking repo-worker harness scenarios
harness-repo-worker-ci:
    bash scripts/harness/run_repo_worker_suite.sh --ci-blocking

# Run compaction harness scenarios (all)
harness-compaction:
    bash scripts/harness/run_compaction_suite.sh

# Run only CI-blocking compaction harness scenarios
harness-compaction-ci:
    bash scripts/harness/run_compaction_suite.sh --ci-blocking

# Coding agent verification loop (run after code changes)
verify: fmt lint test smoke
    @echo "✅ Core flows verified"

# Full local verification
verify-full: verify
    @echo "✅ Full verification passed"
