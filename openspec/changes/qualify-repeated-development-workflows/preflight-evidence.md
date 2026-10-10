# Native preparation evidence — 2026-10-10

This evidence qualifies operator preparation only. At preflight capture all thirty
real-development slots were NOT_RUN; later actual outcomes are in `native-runs.md`.
Task 1.4 remains open. No model authored these preflight
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
access remains unavailable. Freeze all thirty inputs and measurement fields before
generation, then actual Host identities, grant namespaces, model and enforcing
backend before that Host's runs. Ready macOS execution can proceed while Linux
readiness remains open; the thirty-slot completion criterion is unchanged.
Published preparation head `f3985923` has sixteen passing CI checks; later local
documentation requires its own current-head checks and review. User adoption/merge of
#1046 and canonical UI closure #1045 remain separate final-delivery gates.

## Linux normal-CLI checker

The normal product CLI from `cace913c` ran in an ordinary native 80x24 PTY, without
a selected Connection profile. Its provider was unconfigured; no model request
or model/tool substitution occurred. CLI SHA-256:
`6dfeb29543ed050d116623d8d4bc3f2a2265c07b973c4ad04a740e6b0f512dd2`.
Public SDK observer source was `f3985923`, binary SHA-256:
`a679746322f5b9cb4a0e5f74b2cd83e521da37ca226017291899a11a477b151d`.
Host boot `4460152e-f319-456c-9850-524ba76f9021`, native PID 3457085, Root Process 8
and grants `request-1/2/3` had the same read-only source/assertions and writable
scratch arrangement as the macOS preflight.

The first command failed because the operator's Cargo manifest mistakenly named
Alan namespace paths in file contents. The current native runner preserves
granted Host paths internally; shell command projection does not rewrite arbitrary
manifest contents. Action `a1`, original driver/assertions and baseline remain
retained. Correcting this operator-only fixture to native paths required no
production change, additional grant or copied dependency.

Explicit `cargo test --offline --locked --target-dir target-corrected` actually
compiled source and protected checks. Action `a2`/Tool Process 10 exited with code
0 and one passed test. The audit records `linux_reified_namespace` and
`reified_namespace_paths`. The test first successfully read both real source
files, then required PermissionDenied or ReadOnlyFilesystem errors when opening
them for writing; absent paths cannot manufacture a write-denial pass. Original
source and corrected source/assertion/driver hashes remained unchanged. `/quit`
ended the native invocation and its supervised terminal command exited 0.

Owned cache: `/home/morris/.cache/Alan/qdev-10332030`.
`native-checker-preflight.receipt.json` SHA-256:
`dd5ca4a5ef00bb4b3f2860e72c4a6127416aba5a87f54c37bb92921be88fd642`.
Its native terminal capture SHA-256:
`c985024d9325f03012190fb3dec7f3b962cddac738aa2754145a22069007a94b`.
Owned durable rollout:
`services/agent-runtime/rollouts/rollout-20261010-224340-1a052713-178b-4d51-be40-7bc7970d4b24.jsonl`,
SHA-256 `a46ed939d1b6083797b68a69fd1744bdfd23bf53478922941c0ce40334a500ba`.
This bounded CLI/backend preparation adds zero matrix completions and proves no
Linux model/effort or general autonomous development readiness.
