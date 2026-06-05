# Review Notes

- Close guard covers pane, tab, window, app quit, and Quick Terminal close paths. Interactive UI paths prompt once per close scope when active terminal work is present; control-plane close commands return `requires_confirmation` without mutating state.
- Terminal transcript snapshots are bounded on manifest encoding by row count and encoded bytes, carry truncation metadata, and omit PTYs, process handles, Ghostty surface objects, renderer objects, delivery queues, and unbounded scrollback.
- Old workspace manifests without transcript snapshot fields remain decodable. Pinned tab structure remains authoritative; matching transcript snapshots seed restored terminal history, and unmatched transcript overlays are discarded.
- Restart smoke evidence used installed `~/Applications/Alan Dev.app` (`app.alanworks.macos.dev`), confirmed quit with active terminal work, relaunched a fresh app instance, observed prior output, and verified new input in the restored cwd:
  `/private/tmp/alan-persist-session-smoke-dev-final/manifest.txt`.
