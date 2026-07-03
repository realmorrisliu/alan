## 1. Routefs file server

- [x] 1.1 Add `routefs` as a user-space aP file server; post a handle at
  `/srv/route` and serve its tree (send, rules, ports, log) at `/mnt/route`.
  Done 2026-07-02: added `alan-routefs`, an aP file server with `send`, `rules`,
  `ports`, and `log`; the root runtime namespace posts it under `/srv/route`
  and mounts the handle at `/mnt/route`, and child-agent namespace manifests
  inherit `/srv` plus the session's shared `/mnt/route` handle alongside
  `/agent`, `/mnt/llm`, and mounted `/bin` tools. `MountFs` now forwards create
  requests to backing trees and reserves caller-visible `newfid`s before
  forwarding, so rules can be installed through the advertised
  `/mnt/route/rules/<name>` file path rather than only through direct test
  helpers.
- [x] 1.2 Implement the `send` write entry point for typed messages; frame a
  message across multiple writes and match/route only on clunk of the `send` fid
  (commit-on-clunk), never on a partial write.
  Done 2026-07-02: `send` buffers writes by offset and routes only when the fid
  is clunked; tests verify split writes and that no port delivery happens before
  clunk.
- [x] 1.3 Implement rule files (content/type match) with deterministic match
  order and a default dead-letter port.
  Done 2026-07-02: `rules/<name>` are plain JSON files, matched in lexical
  `BTreeMap` order by message type and optional content substring. Create
  reserves rule names until clunk, so concurrent same-name creates cannot both
  succeed and silently overwrite each other. Unmatched messages go to the
  built-in `dead-letter` port.
- [x] 1.4 Implement destination ports as blocking-read streams.
  Done 2026-07-02: `ports/<name>` and `log` are aP `Stream`s with blocking-read
  semantics; port delivery appends one routed JSON record per message.

## 2. Auditability

- [x] 2.1 Append every message (and its routing decision) to an observable log
  stream; keep rules as plain `cat`-able files.
  Done 2026-07-02: routefs appends the same routed record to the destination port
  and `log`, including `port`, `rule`, optional `reason`, and the message; tests
  read both the log and `rules/<name>` files.

## 3. Use cases

- [x] 3.1 Human-in-the-loop governance: route results needing judgment to a human
  inbox port (approval stays an explicit request/response).
  Done 2026-07-02: tests install an inspectable `result` rule with
  `content_contains = "needs_human_judgment"` routing to `human-inbox`, while the
  rule reason states approval remains explicit via the agent file-layout
  request/response path.
- [x] 3.2 Agent→tool/agent handoff by result type.
  Done 2026-07-02: type rules route `patch` messages to review/apply ports
  without the sender naming a receiving actor.

## 4. Verification

- [x] 4.1 Tests: routing by type, dead-letter on no match, message log
  completeness, port blocking-read.
  Done 2026-07-02: `crates/routefs/tests/routefs.rs` covers type routing, split
  send writes, no route before clunk, content-match governance routing,
  deterministic rule order, dead-letter logging, readable rule files, blocking
  port reads, complete JSON message preservation, and `/srv/route` to
  `/mnt/route` mounting. `alan-agent-engine` regression coverage verifies the
  production root namespace exposes `/srv/route` and writable `/mnt/route/send`,
  and child namespace launch manifests include `/srv` plus a shared
  `/mnt/route`. Kernel `MountFs` coverage verifies create forwarding, read-only
  create denial, and `newfid` reservation before backing create forwarding; and
  routefs coverage creates a rule through `/mnt/route/rules/<name>` before
  routing a message through that mounted namespace. Routefs also covers same-name
  create reservation and reservation release for abandoned or invalid creates.
- [x] 4.2 Run `just verify`.
  Done 2026-07-02: `just verify` passed, including workspace fmt, clippy,
  tests, and smoke verification.
- [x] 4.3 Run `openspec validate add-message-routing --strict`.
  Done 2026-07-02: strict OpenSpec validation passed for
  `add-message-routing`.
