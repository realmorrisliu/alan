# Alan Product Context

This glossary names Alan product concepts that cut across macOS shell, runtime,
and future platform work.

## Language

**Primary Shell Window**:
The single main Alan shell window used by the macOS app. Short-term product
work assumes there is only one shell window, and summon behavior targets this
window.
_Avoid_: recent shell window, per-Space shell window, Quick Terminal window

**Primary Window Summon**:
The user action that brings Alan's primary shell window to the user's current
macOS Space and display. It targets the main Alan window, not a detached
terminal panel or separate terminal runtime, and it preserves the current Alan
workspace Space, Tab, and Pane selection. Alan comes to the user's current
desktop context rather than moving the user to Alan's previous desktop context.
It replaces the former Quick Terminal shortcut without keeping Quick Terminal
compatibility aliases. It is an app/window command, not a shell workspace
action.
_Avoid_: Quick Terminal summon, Peak summon, global terminal toggle, quick-terminal alias
