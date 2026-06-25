## 1. Plumbfs file server

- [ ] 1.1 Add `plumbfs` as a user-space aP file server; post a handle under
  `/srv` and serve its tree (send, rules, ports, log).
- [ ] 1.2 Implement the `send` write entry point for typed messages.
- [ ] 1.3 Implement rule files (content/type match) with deterministic match
  order and a default dead-letter port.
- [ ] 1.4 Implement destination ports as blocking-read streams.

## 2. Auditability

- [ ] 2.1 Append every message (and its routing decision) to an observable log
  stream; keep rules as plain `cat`-able files.

## 3. Use cases

- [ ] 3.1 Human-in-the-loop governance: route results needing judgment to a human
  inbox port (approval stays an explicit request/response).
- [ ] 3.2 Agent→tool/agent handoff by result type.

## 4. Verification

- [ ] 4.1 Tests: routing by type, dead-letter on no match, message log
  completeness, port blocking-read.
- [ ] 4.2 Run `just verify`.
- [ ] 4.3 Run `openspec validate add-plumber-message-routing --strict`.
