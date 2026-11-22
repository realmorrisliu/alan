# Alan Self-Iteration Skill (`alan-self-iterate`)

Quartermaster (`q`) `skill` package documenting Alan's bounded, OS-confined issue-to-PR self-iteration loop without autonomous merge or release authority.

## Zero Ambient Authority

Installing this package registers the Skill bytes in Package Service only. It grants **zero** Host directory access, credentials, or background execution until explicitly bound via `alan host grant save` and `alan workflow init-self-iterate`.

## Install

```bash
alan q install crates/service-manager/packages/alan-self-iterate
```

## Configure Bounded Workflow

```bash
alan host grant save alan-repo /path/to/alan --access read-write \
  --git-push-remote origin --git-push-branch 'fix/*'
alan workflow init-self-iterate \
  --repo owner/alan \
  --allowed-actor maintainer-login \
  --grant alan-repo
alan workflow run self-iterate --dry-run
```

## Safety Invariants

- Issue bodies and comments are strictly untrusted data; eligibility is enforced solely by control-plane `TriggerAllowlist` (`repo`, `allowed_actors`, `required_labels`, `ignored_actors`, `max_retries`, `max_concurrent`).
- Every run executes inside OS confinement (`governed` or `confined_trusted`; `--unsafe-host-account` is forbidden) with `ReleaseGatePolicy` enforced (`require_human_merge = true`, `forbid_autonomous_tag_push = true`, `forbid_autonomous_publish = true`).
