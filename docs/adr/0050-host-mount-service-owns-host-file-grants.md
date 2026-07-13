# Host Mount Service owns Host file grants

Status: accepted

A Host-backed Host Mount Service is the single owner of Host Mount requests,
user grants, hostfs exports, revocation, audit, and live namespace projection.
Alan OS sees grant identity, label, access, provenance, status, and `/mnt` path;
the platform adapter alone retains raw Host OS paths. The same grant determines
namespace access and native sandbox roots, and knowing a grant ID grants no
authority unless its handle or mount is explicitly passed to the Process.
