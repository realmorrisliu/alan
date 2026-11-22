## ADDED Requirements

### Requirement: External file-server plugins expose resources exclusively through aP trees
An external `fileserver` plugin installed via Quartermaster SHALL expose its
state, control verbs, and event feeds exclusively as files and streams over the
`aP` protocol (`ctl`, `status`, optional read-only `events` `Stream`, and
resource subtrees). Control mutations written to `ctl` or write endpoints SHALL
commit atomically on clunk (close) rather than on partial writes, and mounted
access (`ReadOnly` vs `ReadWrite`) SHALL be enforced at the Kernel/Service
boundary.

#### Scenario: Partial write does not commit control mutation before clunk
- **WHEN** a caller writes a multi-chunk control command to a mounted file server's `ctl` file without closing the fid yet
- **THEN** the external service state does not mutate until the fid is clunked cleanly
- **AND** a malformed command fails on clunk without partial side effects

### Requirement: Reference githubfs projects GitHub issues, pulls, and events as files
The reference `githubfs` plugin SHALL serve `/srv/github` (mounted explicitly at
`/mnt/github`) exporting top-level `ctl`, `status`, read-only NDJSON `events`,
and `repos/<owner>/<repo>/{issues,pulls}` subtrees. Credentials SHALL remain
with Host adapters, and destructive merge or release-publish verbs SHALL be
omitted by default.

#### Scenario: Creating an issue through mounted githubfs
- **WHEN** `/mnt/github` is mounted `ReadWrite` and an authorized caller writes an issue payload to `/mnt/github/repos/<owner>/<repo>/issues/new` and clunks the fid
- **THEN** `githubfs` creates the issue using the Host-adapter credential and returns the created number and URL
- **AND** raw tokens are never exposed in any namespace file, while `issues/<n>/{meta.json,body.md,comments,ctl}`, `pulls/<n>/{meta.json,checks.json,reviews.json,ctl}`, and append-only `events` remain accessible with monotonic cursors

#### Scenario: Destructive merge or tag-release verbs are rejected
- **WHEN** an Agent writes a merge or release-publish command to a pull request `ctl` node in `githubfs`
- **THEN** the clunk fails closed and no merge or release API call occurs

### Requirement: External GitHub content is untrusted and triggers require control-plane allowlists
GitHub issue `body.md`, titles, and comments SHALL be treated strictly as
non-authoritative external text that cannot alter trust posture or expand mount
grants. Autonomous dispatch MUST be gated by a control-plane allowlist checking
repository, trigger actor, and explicit label before spawning a confined worker
on an isolated worktree without merge or release authority.

#### Scenario: Untrusted issue body cannot trigger autonomous execution or widen grants
- **WHEN** a non-allowlisted user opens an issue containing prompt instructions demanding immediate execution or `--unsafe-host-account`
- **THEN** the trigger gate ignores the issue because neither actor nor label satisfies the control-plane allowlist
- **AND** inspecting `body.md` manually grants no elevated posture or mount authority

#### Scenario: Allowlisted labeled issue launches a confined isolated worker
- **WHEN** an allowlisted maintainer applies the `alan:auto` label to an issue on an allowlisted repository
- **THEN** the dispatcher launches one bounded Agent Process inside OS confinement with an explicit grant to an isolated git worktree
- **AND** the worker can run tests, open a PR, and push branch updates without gaining merge or release rights

### Requirement: Event-driven subagent dispatch composes through routefs with loop guards
Event-driven automation SHALL compose `/mnt/github/events` with `routefs`
(`/mnt/route`) through plain-text inspectable routing rules. Dispatch MUST
deduplicate by unique event id, ignore self-authored feedback cycles, bound
worker concurrency and retries before escalating to a human inbox, and never
leave hidden standing watchers behind bare `alan`.

#### Scenario: Self-authored commits or repeated events do not cause an infinite loop
- **WHEN** a worker pushes fixes to its PR branch or identical duplicate events arrive on `/mnt/github/events`
- **THEN** duplicate event IDs and self-authored push notifications are skipped without spawning redundant worker processes
- **AND** exceeding the configured retry cap routes the item to the human review port instead of re-queueing another worker

#### Scenario: Bare alan exits without leaving a standing background watcher
- **WHEN** a user starts an ordinary interactive `alan` session and exits normally
- **THEN** no GitHub polling process or background service remains alive unless explicitly started through `alan workflow` or an OS scheduler

### Requirement: Self-iteration release preparation remains human-gated
A bounded release-preparation workflow MAY gather changes since the previous
release tag, update crate versions and changelog entries, run `just quality` and
release verification in an isolated worktree, and open a release pull request.
Merging the release PR, pushing git tags, and publishing release artifacts SHALL
require explicit human confirmation, after which `alan update` MAY install the
published release locally.

#### Scenario: Release workflow prepares a verified PR without autonomous publication
- **WHEN** an operator triggers the release-preparation workflow
- **THEN** the workflow validates tests and opens a release pull request ready for review
- **AND** it neither merges the PR nor pushes a git tag or publishes release binaries autonomously
