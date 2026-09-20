# alan - Development Tasks

# List available commands
default:
    @just --list

# Run tests
test:
    cargo test --workspace

# Run the canonical non-mutating quality gate and workspace tests
check: quality test
    @echo "✅ All checks passed"

# Format code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Reject the retired workspace runtime model and implicit Host-directory sources
guard-workspace-runtime-absence:
    cargo build -p alan --bin alan
    ./scripts/check-workspace-runtime-absence.sh . target/debug/alan

# Reject retired host-service architecture from current repository and CLI surfaces
guard-daemon-era-absence:
    cargo build -p alan --bin alan
    ./scripts/check-daemon-era-absence.sh target/debug/alan

# Reject retired macOS persistence, installer, and Managed User compatibility surfaces
guard-legacy-macos-absence:
    ./scripts/check-legacy-macos-absence.sh

# Check canonical specs, active changes, and OpenSpec schema instructions
guard-openspec-current-surfaces:
    bash scripts/check-openspec-current-surfaces.sh

# Run OpenSpec guard fixtures, current-surface checks, and strict validation
openspec-check:
    bash scripts/test-openspec-current-surfaces.sh
    bash scripts/check-openspec-current-surfaces.sh
    openspec validate --all --strict

# Run the canonical non-mutating repository quality gate
quality:
    ./scripts/check-quality.sh

# Keep the familiar lint entry point aligned with the canonical quality gate
lint: quality

# Install the versioned pre-commit hook for this checkout
install-hooks:
    git config core.hooksPath .githooks
    @echo "Installed repository hooks from .githooks"

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

# Install standalone release CLI and Alan OS Host binaries locally
install:
    ALAN_INSTALL_CHANNEL=stable ./scripts/install-cli.sh

# Install the local development CLI channel plus Alan OS Host binaries
install-dev:
    ALAN_INSTALL_CHANNEL=dev ./scripts/install-cli.sh

# Validate the standalone CLI/Host installer and archive contract
standalone-distribution-test:
    ./scripts/test-standalone-cli-distribution.sh

# Check standalone release inputs without assembling an app bundle
release-check:
    cargo check --locked -p alan --bin alan
    cargo check --locked -p alan-os-host --bins

# Build and archive the standalone CLI/Host release
release:
    ./scripts/assemble-cli-release.sh

# Uninstall owned standalone CLI/Host binaries without removing stores
uninstall:
    ALAN_INSTALL_CHANNEL=stable ./scripts/uninstall-cli.sh

# Uninstall owned development CLI/Host binaries without removing stores
uninstall-dev:
    ALAN_INSTALL_CHANNEL=dev ./scripts/uninstall-cli.sh

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
verify: quality test smoke
    @echo "✅ Core flows verified"

# Full local verification
verify-full: verify
    @echo "✅ Full verification passed"
