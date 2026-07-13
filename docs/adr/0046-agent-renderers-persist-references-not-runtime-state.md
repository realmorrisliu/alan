# Agent renderers persist references, offsets, and presentation

Status: accepted

An Alan for macOS Agent ContentInstance persists a Process Reference composed
of Alan OS boot identity and PID, caller-held offsets for the AgentFS streams it
renders, and host presentation state. It never persists or copies Tape, pending
requests, Tool state, Agent Machine state, provider state, or Process status
authority. Reattachment verifies the boot identity before walking `/proc` and
`/agent`; Host restart invalidates the reference, while durable Rollouts,
Checkpoints, Memory Stores, or handoff may explicitly seed a new Process with a
new identity.
