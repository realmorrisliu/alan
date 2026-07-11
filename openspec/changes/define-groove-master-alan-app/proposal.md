## Why

Groove Master is a serious bass-practice product whose Alan integration must be
defined directly as an Alan App with an app-owned domain core, a mountable
service tree, and a producer Agent Process created through bounded descriptors
and spawn. Native host work must wait for direct file-client attachment rather
than creating another authority boundary.

## What Changes

- Rename the change and capability from `environment-app` to `alan-app`.
- Preserve the V1 daily practice loop: Today Plan, practice blocks, metronome or
  drum loop, room capture, markers, reflection, Groove Journal, Groove Stream,
  Pocket Tracker, and a low-presence producer agent.
- Preserve the 12-month route, inspiration, non-graded reflection, audio
  progression, local loop library, and product visual identity.
- Define a Groove Master domain core that owns musical/practice semantics and an
  audio backend that owns capture/playback.
- Add a Groove Master aP adapter that posts `/srv/groove-master` and serves the
  app-owned tree at `/mnt/groove-master`.
- Make Alan for macOS a direct file client over that tree. Native UI work has an
  entry criterion that the host can open, watch, and write the mounted files.
- Define producer-agent work as Agent Executable spawn with bounded plan,
  journal, marker, recording-metadata, and writable proposal descriptors.
- Replace the old line-by-line implementation plan with a file-tree-first phased
  plan.

## Capabilities

### New Capabilities

- `groove-master-alan-app`: Defines Groove Master's product boundary, practice
  and audio model, app-owned file tree, Alan for macOS client boundary, producer
  Agent Process, and phased implementation contract.

### Modified Capabilities

None. The obsolete `groove-master-environment-app` delta was never synced into
canonical specs and is replaced by this capability.

## Impact

- Future code is split into Groove Master domain core, audio backend, aP
  file-server adapter, and Alan for macOS renderer/client.
- Service Manager starts the adapter, posts its handle, and mounts its tree;
  Alan Kernel gains no Groove Master types.
- Earlier host-owned integration plans are discarded.
- Producer-agent planning/reflection uses normal Agent Process, Tool, Skill,
  Memory Store, request/action, and file semantics.
