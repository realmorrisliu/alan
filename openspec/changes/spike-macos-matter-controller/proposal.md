## Why

alan needs evidence before productizing physical-device control. The first
question is whether Alan for macOS can safely act as a local Matter controller
using Apple's public `Matter.framework`, commission a low-risk Matter light, and
perform basic state read/write operations without coupling the generic runtime
to Apple-specific APIs.

## What Changes

- Add a macOS-only Matter controller spike that uses Apple `Matter.framework`
  from the Apple client/service layer.
- Prove Alan can create or load a local Matter controller, commission a directly
  paired Matter light from a setup payload, persist controller/fabric state, list
  the commissioned node, read On/Off state, and set On/Off state.
- Keep Matter APIs out of `alan-runtime`; the runtime-facing boundary remains a
  future typed `home.*` tool/RPC surface.
- Capture physical-device action evidence and safety constraints for the later
  `add-home-control-tools` product change.
- Exclude Apple Home control, HomeKit-only devices, Home Assistant bridges,
  Aqara bridge endpoints, raw LLM-visible Matter cluster control, and high-risk
  device categories from this spike.

## Capabilities

### New Capabilities

- `macos-matter-controller-spike`: Defines the macOS-only Matter controller
  spike contract, including setup-payload commissioning, controller state
  persistence, low-risk light control, RPC boundary expectations, and manual
  verification evidence.

### Modified Capabilities

- None. The spike records constraints for future governance/tooling work but
  does not yet change stable `home.*` tools, policy semantics, or daemon APIs.

## Impact

- Affected Apple client areas: new macOS Matter controller service prototype,
  controller state storage, Matter setup-payload handling, commissioned device
  registry projection, and low-risk light command execution.
- Affected runtime/daemon areas: no runtime-core coupling in this spike; any
  temporary invocation path must stay behind a local service/RPC boundary that
  can later become the typed `home.*` tool provider.
- Affected dependencies: Apple public `Matter.framework` on macOS.
- Affected verification: focused unit/fake-service tests where possible plus
  manual verification against a real directly Matter-capable light.
