# Quartermaster: Package Management as an Alan OS Organ

Status: Accepted. Extends [ADR-0024](0024-plan9-kernel-model.md) (kernel model)
and [ADR-0027](0027-north-star-capability-map.md) (north-star map). The
normative v0 contract with scenarios lives in OpenSpec
(`package-management-contract`, drafted in change
`add-alan-package-management`); this ADR records the identity, naming, and
fusion decisions and their rationale, and distills a longer standalone "APM"
vision draft into the Alan OS model.

## Context

Dogfooding Alan's skill system against its first real external workload
(ai-berkshire, a 19-skill Claude Code / Codex investment-research repository)
exposed that Alan has no supported way to adopt external content, and that the
natural unit of adoption is the repository, not the individual skill. In
parallel, a standalone product vision ("APM — a package manager for the Agent
Era") argued that an agent environment consists of far more than software —
CLI tools, MCP servers, skills, workflows, models, knowledge packs — and that
these are today managed by fragmented per-ecosystem installers with no unified
store, permission model, or reproducibility story.

Both pressures point at the same missing organ. The question this ADR settles
is what that organ *is*: a standalone universal product that treats Alan as
one adapter target, or a subsystem of Alan OS.

## Decisions

### D1. Package management is an Alan OS organ, not a standalone product

The package manager is a subsystem of Alan OS. It reuses Alan's existing
mechanisms instead of shipping parallel ones. The standalone-product framing
(own store root such as `/opt/apm`, own profile system, own trust model,
first-class adapters for foreign agent stacks) is rejected for now: it would
run a second, incompatible namespace-and-policy system inside an OS whose core
mechanism is namespaces and policy. Spinning the subsystem out later remains
possible if it earns it; designing for that spin-out now is not a goal.

### D2. The subsystem is named Quartermaster; the command is `q`

The command surfaced in the Alan Shell command namespace is `q`; the
subsystem's formal name is **Quartermaster**. The name works because "agent"
means the same thing in both worlds: the intelligence-service Quartermaster
outfits agents with equipment, and Alan's `q` outfits agents with
capabilities. The lineage (British intelligence, Turing's world) matches
Alan's own naming heritage, and the verbs under the name stay deliberately
boring (`q install`, `q list`, `q upgrade`, `q uninstall`) in the acme
tradition: a good name over a plain interface. In v0, before the shell command
namespace lands, the surface is hosted by the `alan` CLI; registering `q` into
`/bin` follows the shell contract when available.

### D3. Fusion table: vision concepts map to existing Alan OS mechanisms

The standalone draft's concepts translate as follows; the left side is
vocabulary from the draft, the right side is what Alan OS already owns:

All namespace concepts below are Alan OS (`alan-kernel`, aP) namespaces — pure
userspace constructs identical on every host — never host-OS mount
namespaces. Agent Processes execute inside Alan OS and see only its file
system; host paths are runtime implementation detail.

| Vision concept | Alan OS mechanism |
|---|---|
| Profiles (`default`, `work`, per-project) | Namespaces — a profile is a set of packages bound into a process's namespace, not a parallel switcher |
| `/opt/apm/store` content-addressed store | Alan-owned store projected read-only at `/lib/pkg/<package-id>` in the namespace (host-directory-mounts); backing location is implementation detail; content-addressing aligns with `content-addressed-knowledge` |
| Per-package permission declarations | PolicyEngine / policy chain — declarations map into policy when that slice lands, never a parallel permission system |
| App packages | `alan-app-distribution` and app/service integration contracts own app semantics |
| Service packages | Service Manager owns service lifecycle |
| Knowledge packs | `content-addressed-knowledge` owns knowledge storage |
| Agent adapters (Claude/Codex layouts) | v0's conversion primitives (adapter preamble, format conversion) generalized later into export/install-into-foreign-agents |

### D4. Terminology: "package", never "Capability Package"

The draft's central term "Capability Package" is rejected: *capability* is
already load-bearing in this repo (OpenSpec capability specs, ADR-0027's
capability map, skill frontmatter `capabilities`, tool/descriptor access
rights). The unit is a **distribution package** ("package" in context); what a
package delivers is described by its contents (skills, tools, and later
types), not by a new umbrella noun.

### D5. Manifest-first is adopted as a principle; declarative only

Package metadata is declarative data the manager can parse, validate, and
audit without executing arbitrary code (no Homebrew-formula-style executable
DSL). Each package has a channel-unique package id (normalized source basename
or explicit `--name`); a collision with a different source is rejected before
write. v0 keeps metadata minimal (provenance, materialization manifest) and
deliberately excludes a type enum; later slices pay the format-migration cost
when a second package type becomes real.

### D6. Q is the sole capability-resolution authority (physical unification)

Skill capabilities have exactly one owner: Quartermaster. There is no bypass
source. Every skill an agent can reach — including Alan's own first-party
built-ins — is a Q package projected through `/lib/pkg`. Pre-installed and
distribution packages live in the channel-scoped store; AgentRoot, workspace,
and public skills remain at their authored locations and are registered as Q
local-source providers, never copied into the store. They contribute skills
only through Q resolution, not through a separately enumerated host directory.
This retires both legacy bypasses: directory enumeration
(`package_dirs_for_roots`) and the later `builtin_capability_packages()` append
inside `ResolvedCapabilityView::from_package_dirs`. Q becomes the sole
authority that resolves the capability set for a given agent.

This is **physical unification**, chosen over interface-only unification (Q as
a single façade that still aggregates untouched legacy source directories). The
trade-off: physical unification requires migrating built-in and agent-root
skills into the Q package model, which is real work; interface-only would have
been cheaper but leaves the fragmentation D6 exists to remove, and a façade
over unchanged sources is not "one owner" — it is one reader over many owners.

Two concepts that "physical unification" must **not** be allowed to conflate:

- **"Managed by Q" (ownership/lifecycle)** — install, register, upgrade,
  uninstall all flow through Q; no source escapes it. This is achievable
  without a namespace-native engine.
- **"Discovered by the agent via `ls /lib/pkg`" (runtime self-discovery)** —
  the agent reading manifest-selected capability roots as files, without
  recursively treating all package content as skills. This is gated on
  `refactor-engine-namespace-native` (ADR-0027 Ring 2, unfinished).

Until Ring 2 lands, the engine obtains its resolved capability set through Q's
host-side resolution interface (the "use Q to find skills" path), while
`/lib/pkg` is already the single namespace view over store-backed and
local-source providers. When Ring 2 lands, the resolution interface degrades into the agent walking the
manifest-selected roots under `/lib/pkg` directly — a presentation-layer
finish, not a re-architecture.

Scope note: this concerns **Skill** capabilities. Core **Tool** execution is
owned by the namespace-native `/bin/<tool>` contract. Tools stay outside Q's
v0 package ownership, and Q introduces neither an in-process registry bypass
nor a parallel Tool execution path. Future Tool package distribution must
extend that canonical namespace contract rather than replace it.

### D7. Roadmap is sliced; the change is the first slice of D6

The `add-alan-package-management` change is **slice 1** of the D6 authority
model, not a standalone package manager. Slice 1 establishes: Q as the sole
skill-resolution authority (legacy `package_dirs_for_roots` enumeration and
direct `builtin_capability_packages()` injection retired), distribution packages from git/local sources
(source-revision-token-as-version),
built-in first-party skills reseeded as Q pre-installed packages, agent-root /
workspace skills registered as Q local-source packages, the read-only
`/lib/pkg` store projection, provenance, manifest, idempotent upgrade, exact
uninstall, honest failure on missing capabilities, and the ai-berkshire
dogfooding validation.

Explicitly deferred to later slices, in rough order of expected pressure:
agent runtime self-discovery via `ls /lib/pkg` (gated on Ring 2), additional
package types (MCP servers, tools/binaries, workflows, models, knowledge
packs), permission declarations wired to policy, `q` registered in `/bin`,
user-configurable package profiles beyond the baseline rule that each Agent
Process sees only its Q resolved set under `/lib/pkg`, reproducibility
(lockfile-equivalent), registry/signing/trust, Homebrew-as-migration-source.
None are promised by slice 1's contract; each needs its own change with its
own contract delta.

## Consequences

- Slice 1 is large — it MODIFYs skill-system-contract's discovery/first-party/
  channel-source requirements and reseeds built-in distribution — because
  physical unification cannot be faked with a façade. Reviewers judge it
  against the D6 authority model, not against a minimal installer.
- One owner for all skill capabilities means future slices extend one
  subsystem instead of accreting parallel installers (skills installer, MCP
  installer, model downloader) that would recreate the fragmentation the
  vision criticizes.
- Rejecting profiles/store/permission parallels binds Quartermaster's fate to
  Alan's namespace and policy mechanisms — intentionally: if those mechanisms
  are not good enough for a package manager, that is a kernel problem to fix,
  not to route around.
