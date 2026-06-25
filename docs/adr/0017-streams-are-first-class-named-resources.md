# Streams Are File Kinds

Streams in Alan Kernel should be File kinds that can be read, tailed, and
resumed from offsets rather than internal event transport hidden behind
subscriptions. Watching is a blocking read on a stream File (`tail -f`
semantics); Subscription is retired as a concept (ADR-0024 D8), not a separate
watch surface. Stream Files provide the UNIX-like pipe/log substrate for replay,
Agent/App evidence interpretation, host recovery, and cross-app observation
without becoming a separate Kernel primitive.
