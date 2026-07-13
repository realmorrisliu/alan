# Clean generated state; explicitly import authored content

Status: accepted

Removal of the workspace runtime model automatically deletes recognized
generated runtime, cache, registry, shell-restore, and daemon/session state.
Legacy connection metadata receives one bounded migrate-verify-delete pass into
Connection Service while credential secrets remain in their owning Host store.
Persona, policy, Agent Definition, Skill, and Memory files that may be
user-authored are never silently deleted or scanned by the new runtime; users
may explicitly import them, verify the installed result, and then choose to
remove the source. No long-lived compatibility reader or overlay remains.
