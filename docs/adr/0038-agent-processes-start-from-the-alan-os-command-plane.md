# Agent Processes start from the Alan OS command plane

Status: accepted

Ordinary Agent Processes are created inside Alan OS by executing an Agent
Executable through Alan Shell or another Process and passing an explicit Agent
Definition descriptor. Alan OS Host boot does not accept `--agent`, resolve
named Host OS overlays, or select an Agent Process profile; those legacy CLI
surfaces are removed. Root Agent Process creation remains a Service Manager
boot responsibility and is configured by its boot unit rather than the Host
Command Plane. Its system-owned Agent Definition reference changes only through
the system update path and cannot be replaced by a boot argument or launch
directory, keeping `/agent/root` semantically stable across Process restarts.
