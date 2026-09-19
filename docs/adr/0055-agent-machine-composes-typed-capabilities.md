# Agent Machine composes deterministic and typed model capabilities

Status: accepted direction, 2026-09-19; not implemented. Extends ADR-0024 and
supersedes the generation-only interpretation of the AI Turing Machine analogy.

Agent Machine advances from events, current state and capability results.
Deterministic computation, typed evaluation, generation, governed effects and
waiting are composable operations. System 1 and System 2 describe work modes,
not Kernel Process types, mandatory child Processes or permission levels.
Spawn only for an independently needed lifecycle or isolation boundary.

Jev is a candidate typed evaluation adapter, not a substitute chat generator.
The Connection boundary must expose operation capabilities and typed results;
evaluation must not be disguised as assistant text or hidden in provider-only
extra parameters. Provider/profile secrets stay with their existing owners.

An evaluation result is evidence, not authorization. All Agent-originated
effects reuse Agent Runtime governance and the existing durable effect
lifecycle. Human Shell operations do not acquire a universal AI-review gate.
Unknown effects after a crash cannot be blindly replayed. Explicit input mode,
namespace reachability and deterministic safety gates take precedence over
model suggestions. Fallback consumes a bounded budget.

Machine state, durable rollout/checkpoint, public Tape and model-input
projection are distinct. Today's namespace Tape is a text projection, not a
complete recovery checkpoint. Keep existing evidence owners and define a
versioned recoverable record before claiming complete replay/fork. Completion,
waiting, failure and Process exit are separate; success need not generate text.

Kernel, aP, Process identity, routefs and Package Service remain unchanged.
Rubrics may be ordinary definition/Skill files. q installation is not execution
authorization; there is no new cognitive package kind or global execution
manager. Memory feedback does not automatically promote policy or permissions.

Owning unimplemented proposal: `add-cognitive-model-routing`. Its runtime deltas
must be implemented, verified and synced before replacing canonical generation
contracts. The next planning pass selects a bounded vertical implementation slice.
