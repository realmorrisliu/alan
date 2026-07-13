# Separate Host and Alan OS command planes

Status: accepted

Host lifecycle, attachment, Host Mount authorization, credentials, and native
integration use a Host Command Plane. Operations inside Alan OS use Alan Shell,
namespace files, `/bin` executables spawned through `/proc/clone`, and
service-owned `ctl` files. The external `alan` CLI may provide boot-and-attach
convenience but must not reproduce Alan OS commands as a typed management
interface; legacy workspace initialization and registry commands are removed,
while macOS Space, Tab, and Pane automation remains explicitly host-owned.
