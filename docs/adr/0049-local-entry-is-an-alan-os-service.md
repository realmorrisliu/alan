# Local entry is an Alan OS service

Status: accepted

A Service-Manager-started Local Entry Service creates each local Shell Process,
assigns its Alan OS credentials and Login Namespace Template, and hands off its
namespace to an authorized local renderer. The Alan OS Host Unix socket adapter
only verifies Host OS peer access and transports aP bytes; it does not become a
second Process manager. Entry records are bounded creation protocol state, not
Sessions, and disconnect may end the Shell Process without ending independent
Agent Processes spawned beneath it.
