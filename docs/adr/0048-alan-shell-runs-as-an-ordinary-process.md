# Alan Shell runs as an ordinary Process

Status: accepted

Every interactive Alan Shell runs as a Shell Process with Alan OS credentials,
a namespace, descriptors, cwd, PID, and parentage; executables invoked through
it become child Processes. Renderer hosts attach input and output rather than
calling `/proc/clone` as hidden execution managers. Host OS peer identity is
used only to authorize access to the local Unix socket and does not create Alan
OS home directories, workspace bindings, or Host OS user identity inside the
namespace.
