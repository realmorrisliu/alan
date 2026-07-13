# Closing an Agent view only detaches

Status: accepted

Closing an Agent ContentInstance, Pane, Tab, window, or the macOS app releases
only that renderer's Agent Attachment. It never infers Process ownership from
visibility or attachment counts and therefore does not terminate the Agent
Process. Stopping execution is a separate explicit Alan OS command written to
`/proc/<pid>/ctl`, with lineage effects determined by Process policy; a close UI
may offer that choice but defaults to closing the view.
