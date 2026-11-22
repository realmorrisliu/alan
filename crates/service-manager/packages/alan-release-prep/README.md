# Alan Release-Preparation Skill (`alan-release-prep`)

Quartermaster (`q`) `skill` package documenting Alan's bounded, strictly human-gated release-preparation workflow (`vX.Y.Z` PR drafting, verification, human merge/tag/publish gate, and post-release `alan update` close-out).

## Zero Ambient Authority

Installing this package stores the Skill bytes in Package Service with zero ambient Host mounts, credentials, or release authority.

## Install

```bash
alan q install crates/service-manager/packages/alan-release-prep
```

## Configure Bounded Workflow

```bash
alan host grant save alan-repo /path/to/alan --access read-write \
  --git-push-remote origin --git-push-branch 'chore/release-*'
alan workflow init-release-prep \
  --grant alan-repo \
  --target-version 0.1.5
alan workflow run release-prep --dry-run
```

## Human Gate & Post-Release Close-Out

`ReleaseGatePolicy` fails closed unless all three flags remain `true`:
- `require_human_merge = true`
- `forbid_autonomous_tag_push = true`
- `forbid_autonomous_publish = true`

After a human maintainer merges the release PR, pushes `v<version>`, and `.github/workflows/release.yml` finishes publishing artifacts, run:

```bash
alan update --check && alan update && alan --version
```
