# Service Manager uses bounded restart budgets

Status: accepted

Service Manager implements only `never`, `on-failure`, and `always` restart
policies with bounded exponential backoff, a restart budget, and a stable
window that resets the budget. Exhausting a required unit's budget before
readiness fails boot; exhausting it afterward marks the system degraded and
requires an explicit `ctl` retry while Service Manager remains observable.
Root Agent uses `always` but receives no exception from crash-loop protection.
Backoff timings are small system calibration constants, not a unit policy
language.
