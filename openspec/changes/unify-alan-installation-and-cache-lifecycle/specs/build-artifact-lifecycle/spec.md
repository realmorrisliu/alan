## Purpose

Defines ownership, reuse and safe retirement of Alan repository build output and disposable verification artifacts across worktrees and concurrent development tasks.

## ADDED Requirements

### Requirement: Build outputs have explicit owners
Repository build workflows SHALL associate each output root with its checkout and purpose. Compatible ordinary commands SHALL reuse checkout-local output. Independent worktrees SHALL remain isolated. The quality gate SHALL retain its reproducible owned output boundary.

#### Scenario: Build and install use compatible inputs
- **WHEN** ordinary build and installation use the same checkout, profile and target
- **THEN** installation reuses that checkout's compatible Cargo output
- **AND** it does not create another full compiler tree solely for installation

#### Scenario: A task selects external output
- **WHEN** a managed workflow selects a target outside its checkout
- **THEN** it records the exact output root, owner and retirement boundary
- **AND** it refuses to claim an existing unowned directory implicitly

### Requirement: Disposable verification output retires with its task
One-shot verification SHALL disable incremental compilation and release its generated output after completion, evidence retention and consumer shutdown. Ordinary iterative development SHALL retain incremental reuse unless measurements justify another policy.

#### Scenario: Independent verification completes
- **WHEN** the task records its source revision, command, result and required diagnostics
- **THEN** its exclusively owned idle compiler output can be retired
- **AND** evidence and source changes remain available outside that output

### Requirement: Cache reporting is non-destructive
Repository maintenance SHALL report exact resolved artifact roots, allocated sizes, ownership and active or unknown state without deleting files. It SHALL account for configured external target and intermediate build directories.

#### Scenario: Cargo output is overridden
- **WHEN** a checkout uses an external target or separate intermediate directory
- **THEN** the report identifies the effective paths and ownership uncertainty
- **AND** it does not report the checkout-local target as the complete footprint

### Requirement: Cleanup coordinates with live producers and consumers
Artifact cleanup SHALL default to a dry run, require explicit application, and revalidate ownership and exclusive access at deletion. Managed builders and cleaners SHALL coordinate access. Unknown, active, changed or recreated paths MUST be skipped; cleanup SHALL use supported Cargo scopes or exact retired task roots.

#### Scenario: A build begins after inventory
- **WHEN** a producer acquires a managed output root before cleanup obtains exclusive access
- **THEN** cleanup skips or waits without deleting that producer's files

#### Scenario: Output reappears
- **WHEN** another task recreates a cleaned output directory
- **THEN** cleanup reports the new activity and does not repeatedly delete it

### Requirement: Cache deletion excludes authored and durable data
Cache maintenance MUST NOT delete source checkouts, patches, Git registrations or branches, credentials, installed packages, Memory Stores, rollouts or checkpoints. A cache-like directory name SHALL NOT establish deletion authority. Cleanup MUST NOT follow symlinks outside the owned root.

#### Scenario: Historical cache contains a dirty source checkout
- **WHEN** inventory discovers source or uncommitted work under a cache path
- **THEN** it reports the content as protected or unclassified
- **AND** bulk cache cleanup leaves it intact

### Requirement: Build tuning preserves verification meaning
Artifact-size tuning SHALL retain assertions, error checking and required tests. Changes to iterative-development defaults SHALL be supported by cold/repeated-build timing and allocated-size measurements, with a usable full-debug path.

#### Scenario: Reduced debug information is evaluated
- **WHEN** a lighter debug setting is proposed for the default workflow
- **THEN** acceptance verifies required gates, useful stack traces and measured repeated-build performance
- **AND** the change records the debugging capability trade-off
