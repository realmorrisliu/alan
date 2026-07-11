# Testing Strategy

Alan tests each ownership boundary at the narrowest useful layer, then verifies
the assembled products.

## Rust layers

1. Unit tests cover local pure behavior.
2. Adjacent white-box suites cover large private modules.
3. Crate integration tests cover public boundaries.
4. Harness fixtures execute exact regression scenarios and emit evidence.
5. Release and CLI checks inspect the built binary.

The normative placement rules live in the OpenSpec
`rust-test-placement-contract` capability.

## Core commands

```bash
cargo test --workspace
cargo test -p alan-agent-engine
cargo test -p alan-agent-protocol
cargo test -p alan-terminal-ui
cargo test -p alan-kernel
cargo test -p alan-agentfs
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Use `alan-llm`'s mock feature for deterministic provider-independent tests.

## Execution alphabet and AgentFS

Event/Op tests prove only the records still used by Agent Execution Engine,
AgentFS projection, Tools, approvals, plans, and renderer-visible execution.
AgentFS and TUI tests additionally cover:

- initial file hydration;
- offset continuation and overlap deduplication;
- retained-data gaps;
- input and control writes;
- pending requests and answers;
- activity, thinking, plans, notices, and Tool presentation.

## Persistence and memory

Fresh-state tests prove:

- rollouts are self-identifying and Process-associated;
- recovery creates a new Process record;
- checkpoints link to current tape roots;
- Working Memory is Process-local;
- Episodic Memory and handoff preserve cross-Process continuity;
- only channel-scoped current paths are read and written.

## Harness

Harness runners live under `scripts/harness/` and write evidence to
`target/harness/<suite>/latest/`. Current suites cover effect deduplication,
recovery governance, compaction, repo-coding, coding-steward behavior, and
self-evaluation. See [the harness guide](harness/README.md).

## Live providers

Provider and runtime live tests are ignored by default and require
`ALAN_LIVE_PROVIDER_TESTS=1`. They validate real upstream auth and request
shaping without becoming part of normal CI.

## Apple verification

For Alan for macOS changes:

```bash
cargo test -p alan-shell-core -p alan-shell-core-ffi
bash clients/apple/scripts/test-shell-core-ffi-adapter.sh
just apple-shell-focused-tests
xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos build
just install-dev
just apple-shell-ui-smoke
```

Use a fresh Alan Dev launch and inspect the rendered result when UI or shell
behavior changes.
