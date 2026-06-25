# Streams Are File Kinds

Streams in Alan Kernel should be File kinds that can be read, tailed, watched,
and resumed from offsets rather than internal event transport hidden behind
subscriptions. Subscriptions remain watch operation surfaces over Files,
Process endpoints, or stream Files, while stream Files provide the UNIX-like
pipe/log substrate for replay, Agent/App evidence interpretation, host recovery,
and cross-app observation without becoming a separate Kernel primitive.
