# Host restart creates new Processes

Status: accepted

Restarting an Alan OS Host creates a new Process table, new PIDs, and a new
Root Agent Process; it never deserializes prior live Processes, descriptors,
namespaces, or runtime tasks. System services restore only state owned by the
System Store, and later Agent Processes may continue work by consuming prior
Rollouts, Checkpoints, Memory Stores, and handoff files. `/agent/root` is a
stable role path rather than a promise of stable Process identity.
