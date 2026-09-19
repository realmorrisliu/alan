# Remove Daemon-Era Surfaces Before Replacement Design

> Historical cleanup decision. Subsequent ADR-0044/0045 defined local Host attachment and it is implemented; ADR-0054 now retires the desktop product. Do not cite this record as evidence that attachment is still undecided.

Status: accepted

Alan will remove daemon/session/HTTP/WebSocket/relay contracts and implementation as a clean
break, even when that temporarily removes user-visible capabilities. Keeping those compatibility
surfaces until Alan for macOS has a replacement design would preserve a boundary already rejected
by the Alan OS model, so Alan for macOS integration remains a separate, deliberately undecided
follow-on. The cleanup also removes session identity from Agent Engine, rollout, protocol, TUI, and
Memory Store surfaces and actively deletes recognized daemon/session state instead of retaining a
compatibility reader, migrator, backup format, or resumable legacy path. Alan will not replace
Session with a Thread, Conversation, Run, or equivalent center object: lifecycle belongs to Process,
machine state belongs to Agent Machine, and continuity belongs to rollout/checkpoint files and
Memory Stores.
