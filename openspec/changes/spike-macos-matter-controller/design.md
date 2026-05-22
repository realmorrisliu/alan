## Context

alan currently treats tools as side-effect boundaries and keeps platform-specific
client code out of `alan-runtime`. Smart-home control raises the stakes because
it affects physical devices. The first step should therefore prove the platform
controller path with a low-risk device before defining product-level `home.*`
tools or broader governance policy.

Apple exposes a public `Matter.framework` on macOS with `MTRDeviceController`.
That makes a macOS-only spike possible without bridging Apple Home, Home
Assistant, or a cloud smart-home service. The spike target is a directly
Matter-capable light that can enter normal Matter pairing or multi-admin pairing
mode and join Alan's own Matter fabric.

## Goals / Non-Goals

**Goals:**

- Prove Alan for macOS can act as a local Matter controller through Apple's
  public `Matter.framework`.
- Commission one directly Matter-capable light from a setup payload.
- Persist enough controller/fabric state to reuse the commissioned node after
  app restart.
- Read and write low-risk On/Off state and record an auditable action result.
- Keep Matter framework integration in the Apple client/service layer, behind a
  local service boundary that can later back typed `home.*` tools.

**Non-Goals:**

- Do not control Apple Home, read HomeKit rooms/scenes/automations, or depend on
  a Mac Catalyst HomeKit helper.
- Do not bridge Home Assistant, Aqara cloud, Aqara app APIs, or Apple Home APIs.
- Do not support HomeKit-only devices, bridges, bridged endpoints, locks,
  garage doors, cameras, security systems, appliances, or high-power devices.
- Do not expose raw Matter endpoint/cluster/command control to the LLM.
- Do not productize `home.*` tools, device naming, user-facing UI, or full
  physical-device governance in this spike.

## Decisions

1. **Use Apple `Matter.framework` for the macOS spike.**

   The spike should use the platform framework that is already present in the
   macOS SDK instead of starting with upstream `connectedhomeip`, `matter.js`, or
   a Rust implementation. This keeps the first verification close to the Alan
   for macOS product surface and avoids introducing a sidecar runtime before the
   core controller feasibility is known.

   Alternative considered: use upstream `connectedhomeip` directly. That is the
   most portable Matter implementation, but it adds C++ build, storage, and
   packaging complexity before we know whether the Apple platform path is enough.

2. **Keep Matter code out of `alan-runtime`.**

   Matter controller setup, fabric storage, setup payload parsing, and device
   command execution are Apple-platform concerns. The spike should place them in
   a macOS Matter controller service under the Apple client boundary. The future
   runtime-facing surface is a typed local RPC/tool provider, not direct linkage
   from Rust runtime core to `Matter.framework`.

   Alternative considered: add built-in Rust tools that call Apple APIs. That
   would pollute generic runtime/tool code with macOS-only dependencies and make
   Linux support harder.

3. **Target one directly Matter-capable light.**

   A light is low risk and maps cleanly to Matter On/Off behavior. Using a direct
   Matter light also avoids bridge endpoint mapping, vendor-specific device
   exposure, and Aqara gateway firmware variability during the first spike.

   Alternative considered: commission an Aqara Matter bridge and control a
   bridged light endpoint. That is valuable later but adds bridge-node topology
   and supported-device-type uncertainty to the first proof.

4. **Treat setup payload entry as an operator/debug surface.**

   The spike can accept a setup payload from a local debug command or narrow
   developer UI. It does not need final onboarding UX. The payload source may be
   the physical light's setup code or a multi-admin pairing code from an existing
   ecosystem, but Alan must treat the resulting commissioned node as part of
   Alan's own fabric.

   Alternative considered: design full pairing UI now. That would prematurely
   couple the spike to product UX before controller feasibility and persistence
   are proven.

5. **Record physical action evidence even in the spike.**

   Every write action should produce a structured result that includes target,
   requested action, execution status, timestamp, and error details when
   available. This is not the final governance/audit contract, but it ensures the
   later `home.*` tools can build on an evidence-producing service.

   Alternative considered: rely on logs only. Logs are useful for debugging but
   not enough for future agent-visible tool results or owner decisions.

## Risks / Trade-offs

- **Matter commissioning can fail due to network, Thread border router, setup
  code, or device state issues.** -> Keep the target to one direct Matter light
  and require manual verification notes with exact failure modes.
- **Apple framework storage behavior may require additional delegate work.** ->
  Make controller/fabric persistence an explicit spike requirement and test app
  restart before declaring success.
- **Physical writes can surprise users even when low risk.** -> Limit writes to
  On/Off on a light and record each action result.
- **A successful direct-light spike may not prove bridge support.** -> Treat
  bridge and bridged endpoint support as a follow-up product/spike scope.
- **Temporary debug entry points can become accidental product API.** -> Mark
  any setup payload entry or debug command as spike-only unless the later
  `add-home-control-tools` change adopts it.

## Migration Plan

1. Add a macOS Matter controller service prototype behind an internal boundary.
2. Add setup payload intake for a direct Matter light through a spike/debug path.
3. Create or load the Matter controller and commission the light into Alan's
   fabric.
4. Persist controller/fabric state and verify restart reuse.
5. Add read/list/set OnOff operations and structured result records.
6. Add fake-service tests for the service boundary and manual verification notes
   for the real light path.
7. After the spike, decide whether to proceed to `add-home-control-tools`.

## Open Questions

- Which local storage backend should the product path use for Matter controller
  credentials: framework-managed storage, Application Support with restricted
  permissions, Keychain-backed material, or a combination?
- Should the first productized service be in-process in Alan.app or split into a
  signed XPC helper?
- Which additional low-risk clusters, if any, should enter the next product
  change after On/Off succeeds?
