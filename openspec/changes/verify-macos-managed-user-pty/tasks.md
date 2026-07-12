## 1. Operator Prerequisites

- [ ] 1.1 Select or explicitly authorize provisioning of an Alan-managed local account without
  adopting an unrelated unmarked account.
- [ ] 1.2 Install and freshly launch the Developer ID-signed `Alan Dev.app`; confirm the dev
  privileged helper reports healthy.

## 2. Live Managed User Verification

- [ ] 2.1 Use the current previewed helper plan to establish or confirm the selected account's
  Alan Dev ownership marker, home directory, shell, and diagnosis readiness.
- [ ] 2.2 Run the existing helper-backed live PTY smoke for the selected account and record a
  sanitized pass or failure result.
- [ ] 2.3 Confirm the resulting Managed User and managed Terminal Profile state in Alan Dev without
  relying on sudoers or automatically binding the current Space.

## 3. Evidence And Delivery

- [ ] 3.1 Record sanitized helper status, diagnosis, PTY outcome, and any operator-authorized local
  state changes in this change's verification record.
- [ ] 3.2 If verification exposes a product defect, open a separate narrowly scoped implementation
  change and keep this smoke marked failed or incomplete until the fix is verified.
- [ ] 3.3 Keep the current HEAD under Codex review until every thread is resolved, required CI is
  green, and a delayed refresh shows no new findings before merge.
- [ ] 3.4 After merge, sync the build/test delta into the canonical spec and archive this change only
  after the live Managed User PTY smoke has recorded a pass.
