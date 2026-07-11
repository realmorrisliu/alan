## Context

Apple's public `Matter.framework` and `MTRDeviceController` make a direct macOS
controller spike possible without Apple Home, Home Assistant, vendor cloud APIs,
or an upstream Matter sidecar. A directly Matter-capable light is a bounded,
low-risk target that avoids bridge topology and high-risk actuators.

The canonical `alan-app-service-integration` capability permits Apple
frameworks, XPC, and host storage behind an aP adapter, but files remain the
Alan OS authority surface.
This spike therefore proves both the platform controller and the host-backed
file-server boundary.

## Goals / Non-Goals

**Goals:**

- Commission one direct Matter light into Alan's own fabric.
- Persist controller/fabric state across Alan for macOS restart.
- List the light, read current On/Off, and write On/Off.
- Expose the spike through a fakeable `/mnt/matter` file contract.
- Record inspectable physical-action results and exact failures.

**Non-Goals:**

- Apple Home/HomeKit, bridges, bridged endpoints, Home Assistant, vendor cloud,
  or high-risk device types.
- Raw cluster/endpoint commands visible to an LLM.
- Final device naming, home automation, UI, `home.*` Tools, or general
  physical-device governance.
- Matter types in Alan Kernel, Agent Execution Engine, `alan-tools`, or portable
  domain crates.
- A public XPC/RPC controller API.

## Decisions

### 1. Matter.framework stays behind a host-backed file server

Alan for macOS hosts a Matter adapter that speaks aP at the Alan OS boundary.
Internally it may call `MTRDeviceController` directly or through a signed XPC
helper, but clients see only files. Non-Apple builds use fake fixtures and never
link the framework.

Alternative considered: define a typed local RPC provider and later wrap it in
Tools. Rejected: RPC would become the real authority and prevent symmetric file,
remote, and agent clients.

### 2. The spike tree separates controller, commissioning, and devices

```text
/mnt/matter/
├── status
├── controller/
│   ├── status
│   └── fabric
├── commissioning/
│   ├── clone
│   ├── events
│   └── <attempt-id>/
│       ├── request
│       ├── status
│       ├── result
│       ├── events
│       └── ctl            # start/cancel/retry
└── devices/
    ├── events
    └── <node-id>/
        ├── info
        ├── status
        ├── onoff          # readable/writable whole document
        ├── actions/
        │   └── <action-id>/result
        └── events
```

The service posts `/srv/matter`; Service Manager mounts `/mnt/matter` only into
authorized namespaces. Setup payload is a whole request document committed on
clunk. Commissioning lifecycle uses its adjacent `ctl`. On/Off is a bounded
state document; a write commits only after validation and produces an action
record plus event.

### 3. One direct light is the only physical scope

The adapter rejects unsupported payloads and devices for the spike. Direct light
On/Off is the sole write. Bridge nodes, locks, garage doors, cameras, security
systems, appliances, and high-power devices are out of scope even if the
framework exposes them.

### 4. Persistence belongs to the Matter Service

The service owns fabric credentials, controller metadata, commissioned-node
records, and secure host storage. The file tree exposes safe status and references,
not operational secrets. Restart must reopen the same service backing and repost
the handle; Kernel persists nothing.

### 5. Physical action evidence is app/service data

Every On/Off write produces a result containing node reference, requested state,
execution status, observed state when read-back succeeds, timestamp, and error
details. These files may later support AgentFS action records or Tools, but
Evidence is not a Kernel primitive and the spike does not invent a global
evidence API.

### 6. Debug clients use the canonical tree

A narrow developer UI or CLI may collect a setup payload and display state, but
it allocates commissioning and reads/writes device files like any client. No
spike-only RPC route or direct framework call from the UI is allowed.

## Risks / Trade-offs

- [Risk] Commissioning depends on network/Thread/device state → Mitigation:
  preserve exact attempt status/result/events and record manual environment.
- [Risk] Framework storage is opaque → Mitigation: require restart reuse and
  expose safe controller readiness without leaking credentials.
- [Risk] File writes race device state → Mitigation: validate whole document,
  serialize per-node writes, and record read-back separately from requested state.
- [Risk] In-process host authority exceeds namespace rights → Mitigation: the
  adapter enforces fid rights and the host/service confinement remains a second
  security layer; do not overclaim mount-only confinement.
- [Risk] Debug path becomes product API → Mitigation: same file operations,
  spike-only name, and removal/review gate.

## Migration Plan

1. Implement the aP tree against a deterministic fake Matter backend.
2. Connect Apple `Matter.framework` behind the adapter.
3. Add secure controller/fabric persistence and restart verification.
4. Commission one real direct light and verify list/read/On/Off/action records.
5. Decide in a separate change whether to add product UI, `/bin` Tools, more
   device types, or broader governance.

## Open Questions

- Whether platform state uses framework-managed storage, protected Application
  Support, Keychain, or an XPC helper combination.
- Whether Thread commissioning requires a specific border-router setup for the
  first physical fixture.
