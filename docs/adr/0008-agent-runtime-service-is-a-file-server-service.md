# Agent Runtime Service Is A File-Server Service

Status: Accepted. Refined by ADR-0024 and ADR-0025.

Agent Runtime Service is a File-Server Service managed by Service Manager. It
posts a handle under `/srv`, serves AgentFS at `/agent`, and executes Agent
Processes.

Clients inspect and steer work through AgentFS and `/proc` files, descriptors,
streams, and control writes. Process owns lifecycle; Agent Machine owns tape and
transition-local state; AgentFS owns agent IO, requests, actions, and machine
files.
