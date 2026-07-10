## Why

Alan needs evidence that Alan for macOS can safely commission and control one
low-risk Matter light through Apple's public `Matter.framework`. The previous
spike pointed toward a typed local RPC/tool provider; under the Plan 9-like
architecture the platform implementation may remain host-local, but its Alan OS
boundary must be a mountable file tree.

## What Changes

- Preserve the macOS-only feasibility scope: create/load a controller, commission
  one direct Matter light, persist fabric state, list/read the light, and set
  On/Off.
- Add a host-backed Matter Service that posts `/srv/matter` and serves
  `/mnt/matter` through aP while `Matter.framework` stays behind the adapter.
- Model commissioning as an inspectable lifecycle directory with request,
  status, result, events, and owning `ctl` files.
- Model commissioned devices as directories with readable metadata/status and a
  whole writable On/Off state document committed on clunk.
- Record each physical write as a service-owned result file and event with
  target, requested state, observed outcome, timestamp, and error details.
- Keep raw Matter endpoints/clusters, broad device categories, final `home.*`
  Tools, general governance, and product UI out of the spike.
- Require any debug entry point to perform the same file operations as future
  clients and remain explicitly spike-only.

## Capabilities

### New Capabilities

- `macos-matter-controller-spike`: Defines the host-backed Matter Service tree,
  direct-light commissioning, durable controller state, list/read/OnOff
  behavior, action records, safety constraints, and manual evidence gate.

### Modified Capabilities

None.

## Impact

- Alan for macOS hosts the platform adapter and platform credential/storage
  integration; Alan Kernel and file-unaware agent/domain crates import no Matter
  types.
- Service Manager posts/mounts the Matter tree; fake adapters can test the same
  file contract without physical hardware.
- A future home-control change may add `/bin` Tools and broader governance over
  this tree, but the spike defines no RPC or Tool API.
