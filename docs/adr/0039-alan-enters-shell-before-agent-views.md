# Alan enters Shell before Agent views

Status: superseded for the bare `alan` entry by ADR-0056 (2026-09-24); retained
as historical rationale.

Running `alan` boots or attaches the system-level Alan OS and enters Alan Shell
without creating or selecting an ordinary Agent Process. Agent Processes are
spawned through `/bin` and `/proc/clone`, and a renderer may attach to an
existing `/agent/<pid>` view from the Shell; detaching returns to the Shell and
does not terminate the Process. `/agent/root` remains booted by the Service
Manager, but automatic attachment to it is at most a user preference rather
than Host boot semantics.
