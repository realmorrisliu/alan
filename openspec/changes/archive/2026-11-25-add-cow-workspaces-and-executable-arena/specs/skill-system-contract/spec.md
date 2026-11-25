# Spec Delta

## ADDED Requirements

### Requirement: Executable alan-arena runs confined CoW bakeoffs behind an OS sandbox gate
`alan-arena` SHALL keep read-only design comparison as its default and provide an opt-in `executable` mode that forks bounded CoW workspaces under an active OS-enforced sandbox, keeps network denied and contenders isolated to their own clones, and adjudicates via an independent `read_only` verifier.

#### Scenario: Executable mode activates only under OS-enforced confinement
- **WHEN** Root invokes `alan-arena` with `mode = "executable"` on a system running `Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`
- **THEN** each contender receives an isolated CoW clone mount, default-denied network, and a bounded task budget
- **AND** the independent verifier runs `read_only = true` over candidate diffs and test outcomes to score `/mnt/branch`

#### Scenario: Executable mode fails closed when no OS sandbox backend is active
- **WHEN** `alan-arena` is invoked with `mode = "executable"` under `HostMountPathGuard` without OS confinement
- **THEN** Alan rejects executable mode before creating CoW clones or spawning writable contenders
- **AND** suggests falling back to standard read-only `alan-arena` comparison
