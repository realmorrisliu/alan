# Hosts attach through the ready Standard Namespace

Status: accepted

Alan OS Host boot succeeds only after the Standard Namespace, required system
services, and `/agent/root` are ready. It exposes an aP attachment surface and
whole-system shutdown, not Root Agent PIDs, Agent Execution Engine handles,
task handles, or event receivers. Stable namespace paths remain valid across
internal Process replacement, and failed required boot units make boot fail
rather than exposing a partially ready system as healthy.
