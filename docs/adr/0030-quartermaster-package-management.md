# Quartermaster package management

Status: Superseded by
[ADR-0052](0052-packages-have-no-implicit-host-directory-sources.md) and the
rewritten `add-alan-package-management` OpenSpec change.

This ADR previously proposed Quartermaster (`q`) as a Host-side package and
Skill resolution authority backed by Alan-home stores and implicit workspace,
AgentRoot, and `.agents` providers. The system-level Alan OS architecture made
those decisions obsolete.

Current package management is owned by a supervised Package Service. Durable
state belongs to its channel System Store subtree; management happens through
Alan Shell and the service's aP tree; package content is projected through Alan
OS namespaces; Skills are passed by descriptor; Host directories are never
implicit package sources. OpenSpec is the normative contract.
