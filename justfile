# alan - Development Tasks

# List available commands
default:
    @just --list

# Run tests
test:
    cargo test --workspace

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

# Check daemon API route ownership guardrails
guard-daemon-api-contract:
    ./scripts/check-daemon-api-route-strings.sh

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

# Run the agent daemon
serve:
    cargo run -p alan -- daemon start

# Build release
build:
    cargo build --release

# Install release Alan.app plus CLI/TUI locally
install:
    ALAN_INSTALL_CHANNEL=stable ./scripts/install.sh

# Install local-only Alan Dev.app plus alan-dev/alan-dev-tui
install-dev:
    ./scripts/install-dev.sh

# Run local side-by-side smoke for stable Alan and Alan Dev
dev-channel-smoke:
    ./scripts/smoke-dev-channel-side-by-side.sh

# Run focused macOS shell automation command seam tests
apple-shell-automation-seams:
    bash clients/apple/scripts/test-shell-automation-command-seams.sh

# Run Ghostty-backed macOS shell integration checks when local artifacts are prepared
apple-shell-ghostty-integration:
    bash clients/apple/scripts/test-shell-ghostty-integration.sh

# Run repeatable macOS shell UI smoke against the installed Alan Dev app
apple-shell-ui-smoke:
    bash clients/apple/scripts/test-shell-ui-smoke.sh

# Check release signing and notarization configuration without building
release-check:
    ALAN_NOTARIZE=1 ./scripts/release-check.sh

# Build, sign, notarize, staple, and archive the public macOS release app
release:
    ALAN_INSTALL_CHANNEL=stable ALAN_NOTARIZE=1 ALAN_CREATE_RELEASE_ARCHIVE=1 ./scripts/assemble-release-app.sh

# Uninstall alan app and user-level CLI/TUI without removing ~/.alan data
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

# End-to-end smoke test (needs ~/.alan LLM config)
smoke-e2e:
    bash scripts/smoke-e2e.sh

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

# Full verification including real LLM
verify-full: verify smoke-e2e
    @echo "✅ Full verification passed (including E2E)"
