# Agent Tool Governance Becomes Agent Action Governance

Alan OS should not generalize the existing Alan Agent tool governance model into
governance for all app commands. The current allow, deny, escalate policy
engine, approval checkpoints, audit metadata, and sandbox backend ideas remain
valuable, but they are source material for Agent Action Governance inside Agent
Runtime Service and AgentFS action files: governing agent-proposed or autonomous
actions. Ordinary app commands should use lower-level Access Checks, host
consent, and app-domain rules unless they are being proposed or executed by an
Agent Process.
