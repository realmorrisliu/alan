## 1. Introduce replacement owners

- [x] 1.1 Add Process Launch Context types using namespace paths, mounts, descriptors, credentials, and cwd only
- [x] 1.2 Add channel System Store path resolution behind the current Host adapter
- [x] 1.3 Add explicit Host Mount grant input shared by namespace projection and native sandbox derivation
- [x] 1.4 Add descriptor-local Agent Definition resolution with no Host-directory overlay

## 2. Remove workspace runtime configuration

- [x] 2.1 Replace `WorkspaceRuntimeConfig`, workspace id/root/alan-dir fields, and forced workspace routing across engine callers
- [x] 2.2 Convert Agent, child Process, Tool, policy, cwd, and sandbox setup to Process Launch Context
- [x] 2.3 Remove workspace-routing errors and global/workspace-local Tool classifications
- [x] 2.4 Add regressions proving Tool access cannot exceed explicit mounts and sandbox grants cannot drift

## 3. Move durable data off Host project directories

- [x] 3.1 Route rollouts, checkpoints, cache, tmp, and runtime metadata to owning System Store subtrees
- [x] 3.2 Route Memory Stores through explicit trees/descriptors and remove `.alan/runtime/<channel>/memory` inference
- [x] 3.3 Remove creation and reading of directory-local `.alan/runtime`, cache, tmp, shell-restore, and metadata trees
- [x] 3.4 Verify stable/dev System Stores are isolated and raw backing paths never enter Agent-visible content

## 4. Remove implicit Agent, Skill, and package sources

- [x] 4.1 Remove global/workspace/default/named AgentRoot overlay discovery and the `--agent` launch option
- [x] 4.2 Remove workspace, AgentRoot, `~/.agents`, and `.agents` implicit Skill providers
- [x] 4.3 Remove workspace/local-source package precedence and keep `add-alan-package-management` blocked
- [x] 4.4 Add explicit import/descriptor tests for Agent Definitions and Skills mounted from Host content

## 5. Remove workspace CLI and registry

- [x] 5.1 Delete `WorkspaceRegistry`, workspace IDs, and registry persistence
- [x] 5.2 Delete `alan workspace`, workspace-shaped `alan init`, and hidden compatibility aliases
- [x] 5.3 Make bare `alan` enter Alan Shell without privately selecting an Agent profile
- [x] 5.4 Update help, install scripts, tests, and docs to Host Mount and Alan Shell vocabulary

## 6. Migrate connections and clean legacy state

- [x] 6.1 Implement one-shot migrate-verify-delete for non-secret legacy connection metadata
- [x] 6.2 Preserve Host credential-store secrets and create only opaque references
- [x] 6.3 Delete recognized generated workspace/daemon/session state using exact owned-path guards
- [x] 6.4 Report user-authored persona, policy, Agent, Skill, and Memory roots without scanning or deleting them
- [x] 6.5 Provide explicit import followed by optional source deletion for authored content

## 7. Prove complete removal

- [x] 7.1 Add architecture guards rejecting workspace runtime types, commands, paths, Tool locality, and implicit source scans
- [x] 7.2 Run focused engine, Tool/sandbox, Agent/Skill, connection, memory, rollout, and CLI tests
- [x] 7.3 Run `just test`, `just check`, `just fmt`, `just lint`, and strict OpenSpec validation
- [x] 7.4 Verify built binary help and runtime behavior contain no workspace commands or hidden aliases

## 8. Review and archive readiness

- [x] 8.1 Merge implementation PR #643 after current-HEAD Codex review, zero unresolved threads, green CI, and delayed recheck
- [ ] 8.2 Merge the canonical-spec sync PR after removing superseded workspace capability text and completing current-HEAD Codex review, zero unresolved threads, green CI, and delayed recheck
- [x] 8.3 Verify `add-alan-package-management` remains blocked and explicitly depends on the next two changes
- [ ] 8.4 Archive only after implementation and canonical-spec sync are merged and strictly valid
