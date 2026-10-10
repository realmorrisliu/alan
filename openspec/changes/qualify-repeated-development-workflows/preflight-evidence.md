# Native preparation evidence — 2026-10-10

This evidence qualifies operator preparation only. All thirty real-development
slots remain NOT_RUN and task 1.4 remains open. No model authored these preflight
fixtures; no production Runtime or policy behavior was changed by this slice.

## macOS independent checker

The normal production CLI was built from #1046 `cace913c`, SHA-256
`b66362691d83e8f63867058b3a15afd9ddbbbc721ef151a23b93b2ef53c8d6fd`.
It ran in owned Herdr session `alan-development-10332030`, pane `w4:p1`, native
PID 52416, Host boot `a925a8d1-3b47-4725-ac90-875dd08e6afb`, Root Process 8.
Actual stdin PTY and Host pane geometry both measured 80x24. Earlier mismatched
calibration attempts remain retained; the user's session/configuration was untouched.

Existing public Host SDK controls granted `request-1` (source) and `request-2`
(assertions) read-only, and `request-3` (checker scratch) read-write to this Root.
The scratch driver manifest referenced the other two directories. Explicit
`cargo test --offline --locked --target-dir target-fresh` actually compiled both
packages, passed the protected behavior test and denied attempts to open both
original source and assertion source for writing. Action `a5` correlated the
retained result to exited Tool Process 12. Its audit records `seatbelt` and
`projected_host_paths`; source/assertion/driver hashes matched their baselines.
This is bounded write-denial evidence, not general macOS Host read isolation.

Two earlier commands failed before execution and remain retained: `a1` tried
Cargo from a read-only cwd; `a3` supplied an explicit read-only `--manifest-path`
from writable scratch. The normal scratch driver arrangement uses already
granted dependency reads and does not relax those path/cwd guards. It is recorded
as fixture preparation, not corrective model steering or a successful matrix task.

The SDK observer attaches to the existing instance without booting an Agent. Its
source SHA-256 is `86152b7dd9f645a478fb1a95c020b80c4e15c0c198c5c318496fbd31cf25dc47`;
binary SHA-256 is `5645e6b43b0c16401c6ae51f5fcbb7cc4507ae723fdb3597395c286af8d9bf4e`.
Two validation tests pass and warnings-denied Clippy passes. A wrong boot was
refused before a mount intent, and repeating operation
`688f75a5-3be0-477e-bd8f-4003e6140ba8` returned the original `request-2` grant.
`/quit` ended native PID 52416; the pane foreground returned to its owned fish
PID 52289 before the exact named test session was stopped. Both successful Tools
were already exited; no test Alan instance was left running.

## Retained ownership and limits

Owned cache: `~/Library/Caches/Alan/qualification/dev-20261010-10332030`.
`native-checker-preflight.receipt.json` SHA-256:
`ce8879505d4b39da0b9ea2ea002a4fb349c926d30481f6dbf0197d8232bf9261`.
It references Action results/output, original failed commands, exact file hashes,
boot/status, grant acknowledgements, terminal captures and post-exit checks.
The owning System Store rollout is
`services/agent-runtime/rollouts/rollout-20261010-221026-2683a6d1-32e8-4a22-ba2d-c6dd4f273c14.jsonl`,
SHA-256 `dc7b6d7fe8df379dfadae7e05ffdc01dd977e0c545c99f4abb483ec9e5ebad6b`.
No credentials or personal memory bodies are part of these receipts.

`fixtures-v3/manifest.json` is a new thirty-slot preparation snapshot; v1/v2 are
retained, not overwritten. Its SHA-256 is
`b64ad0126be948a69d46f4936cf25caacc6ff8d69b61bcda6e2b37229c32b25e`.
Six offline fixture self-checks pass, including fifteen seeded failure/corrected
behavior pairs and checker manifest/hash integrity. Authored self-check solutions
remain outside model grants and are never counted as real-model development.

The separate actual generation preflight confirmed `gpt-6.1-sol`/medium; it does
not turn the explicit-command checker into a generation run. Linux generation
access remains unavailable, and the reified checker driver has not yet received
its own native CLI preflight. Both Host identities, grant namespaces, complete
measurement collection and enforcing backends must be frozen before the matrix.
Published preparation head `ef11495e` has sixteen passing CI checks; later local
changes require their own current-head checks and review. User adoption/merge of
#1046 and canonical UI closure #1045 remain separate final-delivery gates.
