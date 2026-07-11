# Quartermaster: Package Management as an Alan OS Organ

Status: Accepted. Extends [ADR-0024](0024-plan9-kernel-model.md) (kernel model)
and [ADR-0027](0027-north-star-capability-map.md) (north-star map). The
normative v0 contract with scenarios lives in OpenSpec
(`package-management-contract`, drafted in change
`add-alan-package-management`); this ADR records the identity, naming, and
fusion decisions and their rationale, and distills a longer standalone "APM"
vision draft into the Alan OS model.

## Context

Dogfooding alan's skill system against its first real external workload
(ai-berkshire, a 19-skill Claude Code / Codex investment-research repository)
exposed that alan has no supported way to adopt external content, and that the
natural unit of adoption is the repository, not the individual skill. In
parallel, a standalone product vision ("APM — a package manager for the Agent
Era") argued that an agent environment consists of far more than software —
CLI tools, MCP servers, skills, workflows, models, knowledge packs — and that
these are today managed by fragmented per-ecosystem installers with no unified
store, permission model, or reproducibility story.

Both pressures point at the same missing organ. The question this ADR settles
is what that organ *is*: a standalone universal product that treats alan as
one adapter target, or a subsystem of Alan OS.

## Decisions

### D1. Package management is an Alan OS organ, not a standalone product

The package manager is a subsystem of Alan OS. It reuses alan's existing
mechanisms instead of shipping parallel ones. The standalone-product framing
(own store root such as `/opt/apm`, own profile system, own trust model,
first-class adapters for foreign agent stacks) is rejected for now: it would
run a second, incompatible namespace-and-policy system inside an OS whose core
mechanism is namespaces and policy. Spinning the subsystem out later remains
possible if it earns it; designing for that spin-out now is not a goal.

### D2. The subsystem is named Quartermaster; the command is `q`

The command surfaced in the alan shell command namespace is `q`; the
subsystem's formal name is **Quartermaster**. The name works because "agent"
means the same thing in both worlds: the intelligence-service Quartermaster
outfits agents with equipment, and alan's `q` outfits agents with
capabilities. The lineage (British intelligence, Turing's world) matches
alan's own naming heritage, and the verbs under the name stay deliberately
boring (`q install`, `q list`, `q upgrade`, `q uninstall`) in the acme
tradition: a good name over a plain interface. In v0, before the shell command
namespace lands, the surface is hosted by the alan CLI; registering `q` into
`/bin` follows the shell contract when available.

### D3. Fusion table: vision concepts map to existing Alan OS mechanisms

The standalone draft's concepts translate as follows; the left side is
vocabulary from the draft, the right side is what Alan OS already owns:

All namespace concepts below are Alan OS (alan-kernel, aP) namespaces — pure
userspace constructs identical on every host — never host-OS mount
namespaces. Agent Processes execute inside Alan OS and see only its file
system; host paths are runtime implementation detail.

| Vision concept | Alan OS mechanism |
|---|---|
| Profiles (`default`, `work`, per-project) | Namespaces — a profile is a set of packages bound into a process's namespace, not a parallel switcher |
| `/opt/apm/store` content-addressed store | alan-owned store projected read-only at `/lib/pkg` in the namespace (host-directory-mounts); backing location is implementation detail; content-addressing aligns with `content-addressed-knowledge` |
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
DSL). v0 keeps metadata minimal (provenance, materialization manifest) and
deliberately excludes a type enum; later slices pay the format-migration cost
when a second package type becomes real.

### D6. Roadmap is sliced; v0 is skills-from-git only

v0 (change `add-alan-package-management`): distribution packages from git/local
sources, commit-as-version, materialized skill packages, the read-only
`/lib/pkg` store projection (whole store, all agents), provenance, manifest,
idempotent upgrade, exact uninstall, honest failure on missing capabilities.
Explicitly deferred, in rough order of expected pressure: web-capability and
multi-agent gaps (separate changes seeded by the dogfooding run), additional
package types (MCP servers, tools/binaries, workflows, models, knowledge
packs), permission declarations wired to policy, `q` registered in `/bin`,
per-agent package visibility via namespace binds (different agents see
different `/lib/pkg` contents — the real "profiles"; requires skill discovery
through the namespace), reproducibility (lockfile-equivalent),
registry/signing/trust, Homebrew-as-migration-source. None of these are
promised by v0's contract; each needs its own change with its own contract
delta.

## Consequences

- The v0 change stays small and shippable while this ADR carries the long
  arc; reviewers judge v0 against its own contract, not against the vision.
- Future slices extend one subsystem instead of accreting parallel installers
  (skills installer, MCP installer, model downloader) that would recreate the
  fragmentation the vision criticizes.
- Rejecting profiles/store/permission parallels binds Quartermaster's fate to
  alan's namespace and policy mechanisms — intentionally: if those mechanisms
  are not good enough for a package manager, that is a kernel problem to fix,
  not to route around.
