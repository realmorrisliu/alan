# Service readiness is Process liveness plus handle publication

Status: accepted

A File-Server Service is ready only while its Process is running in `/proc` and
every handle declared by its Boot Unit is published in `/srv`. The Service
Manager does not consume implementation callbacks, Rust channels, or Agent
Execution Engine events. Exit invalidates the service's handles; publication
timeout fails and terminates the stale launch before restart policy applies.
Unit PID, attempts, status, and errors are exposed through a tree owned by the
Service Manager, while `/srv` remains only the handle rendezvous.
