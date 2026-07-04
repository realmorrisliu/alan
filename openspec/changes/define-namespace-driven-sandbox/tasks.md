# Tasks

This is an explore-stage framing change: its "tasks" are to spin the sequenced
proposals out as their own OpenSpec changes. No application code lands here.

## 1. Spin out downstream changes

- [x] 1.1 Create change `refactor-sandbox-spec-input` (P1): replace
  `Sandbox`'s `workspace_root: PathBuf` with `SandboxSpec { writable_roots,
  read_denylist, network }` seeded from a single-entry manifest (the workspace).
  Pure refactor, zero behavior change; welds the two-projection seam.
  Created 2026-07-02; implemented after `refactor-engine-namespace-native`
  Slice B settled the tool-execution seam.
- [x] 1.2 Create change `add-host-dir-file-server` (P2): `HostDirFs` (host-backed
  aP `FileServer`) + `mount_host` declaration entry point that records
  `(host_path, access)` into the manifest + multi-entry projection in `alan`.
  Created 2026-07-02; first implementation slice adds `alan-hostfs`,
  composition-root host mount declarations, and `SandboxSpec` projection.
- [ ] 1.3 Create change(s) for P3+ hardening: macOS Seatbelt sensitive-read
  denylist; agent-requestable `mount` via `PolicyEngine` escalation; (later,
  separately) Linux reification for full read isolation. Created
  `add-seatbelt-sensitive-read-denylist` for the macOS sensitive-read slice and
  `add-agent-mount-escalation` for the request/approval/grant-record slice;
  `apply-mount-grants-to-tool-sandbox` for applying approved read-write grants
  to the runtime tool sandbox projection; and
  `apply-mount-grants-to-live-namespace` for applying approved grants to the
  running Agent Process Alan OS namespace. Linux reification remains pending.

## 2. Carry the framing forward

- [x] 2.1 Ensure P1's proposal references this design's Decision D4 (layering) and
  D6 (P1 = zero behavior change). Verified: P1's proposal Impact cites D4/D6.
- [x] 2.2 Ensure P2's proposal references D5 (mounts human-declared only at
  landing; workspace = seed entry).
- [x] 2.3 Keep the "honest isolation narrative" (write+network now, sensitive-read
  macOS next, full read isolation with reification) consistent across P1/P2/P3
  proposal and user-facing docs.

## 3. Archive

- [ ] 3.1 Archive this framing change once P1 and P2 are proposed and the
  relationship it fixes is captured in their design docs.
