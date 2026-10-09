## Context

The native failure is recorded in the terminal change's `native-acceptance.md`.
`change_process_directory` updates the existing Process Tool binding. Generation
assembly currently uses cached persona/skills and historical Tape, which need not
include that control operation. Rewriting historical paths would alter user intent.

## Decisions

Read the selected namespace cwd from the existing Tool execution binding before
each generation request, append a concise Runtime instruction using a JSON-escaped
path, and include its tokens in pre-turn and mid-turn compaction overhead. Do not
cache this dynamic instruction in persona/skills or modify Tape. The selected cwd
is context only; Tools still reconcile live authority immediately before effects.
Missing or non-UTF-8 binding data must not be replaced by a Host/config path.

## Verification and limits

Capture actual requests after file/API directory selection and after a subsequent
selection. Verify no old directory appears in the dynamic instruction and prompt
overhead remains included. Native revoke/remount must read a disposable project's
files without spelling out the replacement namespace path. This repairs context
availability, not a guarantee that any model will choose a correct Tool argument.
