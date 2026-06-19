# Replace Quick Terminal With Primary Window Summon

Alan will remove the standalone Quick Terminal Peak and reuse the former global
shortcut to summon the single primary macOS shell window instead. This avoids
keeping a second terminal runtime, detached panel, manifest shape, and command
surface alive after the product direction moved to one authoritative primary
shell window; old quick-terminal restore data is discarded rather than migrated
or preserved as compatibility state.
