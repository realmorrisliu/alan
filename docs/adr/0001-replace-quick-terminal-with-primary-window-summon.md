# Replace Quick Terminal With Primary Window Summon

> Product obligation superseded by ADR-0054 (2026-09-19). The original window decision below is historical; retained client maintenance does not authorize new desktop work.

Alan will remove the standalone Quick Terminal Peak and reuse the former global
shortcut to summon the single primary macOS shell window instead. This avoids
keeping a second terminal runtime, detached panel, manifest shape, and command
surface alive after the product direction moved to one authoritative primary
shell window; old quick-terminal restore data is discarded rather than migrated
or preserved as compatibility state.
