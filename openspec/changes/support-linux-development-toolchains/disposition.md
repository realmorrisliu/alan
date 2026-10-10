# Disposition — 2026-10-10

Implementation is authorized by the user's ordered development-qualification
goal. This is the independent second delivery, following UI implementation
PR #1044. Post-merge UI/directory canonical closure is prepared separately and
still requires its own reviewed PR/CI/user merge.

Initial code/live Linux inventory and planning are complete. Native mount safety,
supported PATH and private per-command environment are published in draft PR #1046
at `80f8585`, with all 16 checks passing on that exact head. They are not merged
or a complete development qualification. Task-owned git extraction remains distinct from system
installation. Automatic standard-Rustup projection now passes actual Sandbox
build/test/fmt/Clippy and selection/refusal acceptance, including metadata/helper
containment revalidation. Tasks are 10/15 complete. Normal-commit full quality and
standalone distribution have passed, and the Rustup slice is published with
current-head CI passing. The live-service/Bash fixture slice is outside that earlier-head
CI evidence; its own final review/publication/current-head CI remain required. The native
development fixture now passes actual RED/GREEN/git, declaration-based dependency
denial, isolation/network and Cargo-descendant cancellation/timeout with private
runner cleanup. This closes task 3.5. The final engine suite has 1426 passes,
zero failures and one existing ignore. Complete installation inventory now verifies
379 nodes unchanged across native execution, retaining the earlier provisioning
metadata correction. Actual Bash Tool with a live selected read-only grant passes
two private-output builds, isolation/network assertions and selected-grant revocation
refusal; Linux Host tests pass 49 with two existing live-provider ignores. Tasks
3.1 and 3.4 are complete. Disjoint dependency/live-grant qualification and independent
repeated real-model work remain open.
The actual Bash Tool still uses the ADR-0058 single-active-grant shell adapter;
combined-grant Sandbox fixtures alone cannot qualify separate dependency grants
through that product entry. A narrow explicit read-only-grant exception awaits
the user's design decision before changing the accepted authority boundary.
Keep existing fallback/approval rules until supported execution is proved. Do
not enable automatic input routing or delegate implementation to Alan.
