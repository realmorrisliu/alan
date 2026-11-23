## ADDED Requirements

### Requirement: Enabled fileserver packages run under foreground Service Manager supervision
When an `Active` Root binding or explicit `--fileserver` invocation flag selects an installed `fileserver` package, `ServiceManager` SHALL launch the confined server process once during boot, publish `/srv/<service-id>`, mount `/mnt/<service-id>` into both the Root Agent namespace and `LocalEntryService` with the explicitly granted `ReadOnly` or `ReadWrite` access, and stop all supervised fileserver process groups during `ServiceManager::shutdown`.

#### Scenario: Foreground alan session mounts enabled githubfs for both Agent and TUI /issue
- **WHEN** `github-fileserver` is enabled `ReadWrite` with network permission and `alan` starts in interactive mode
- **THEN** `/mnt/github` is available in Root Agent and `LocalEntryService` namespaces so TUI `/issue` uses Tier 1 (`issues/new`) automatically
- **AND** exiting `alan` terminates the confined fileserver process group cleanly

### Requirement: Explicit foreground workflow watcher composes githubfs with routefs and bounded runs
`alan workflow watch <id>` SHALL start only when explicitly invoked by an operator or scheduler, run the installed `github-fileserver` inside OS confinement, compose `/mnt/github/events` with `/mnt/route` through `GithubEventDispatcher`, dispatch allowlisted work items into isolated `alan -p --ephemeral` runs, support bounded single-cycle inspection via `--once`, and tear down all child processes on `Ctrl+C` or `SIGTERM`.

#### Scenario: Explicit watcher processes allowlisted events and stops cleanly
- **WHEN** an operator runs `alan workflow watch <id> --once` against a repository with an allowlisted labeled event and an untrusted event
- **THEN** the untrusted event is suppressed before dispatch and only the allowlisted event routes to the `worker` port
- **AND** the supervised `githubfs` process stops cleanly before `alan workflow watch` exits
