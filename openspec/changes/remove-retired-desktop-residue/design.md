## Cleanup inventory

| Finding | Evidence / consumer | Disposition |
| --- | --- | --- |
| `third_party/ghostty`, `.gitmodules` | ADR-0054; no surviving build consumer; clean submodule checkout | Remove gitlink, checkout and registration |
| `crates/alan/src/shell_command.rs`, `cli/shell.rs`, `cli/shell/tests.rs` | Main CLI dispatch only; socket/file requests target the removed desktop window/space/tab/pane server | Remove entire adapter and command |
| `skills/alan-shell-control` | Embedded package assets still seeded an unusable desktop Skill | Remove source, registration and positive test expectations |
| Channel shell-control namespace | Only desktop IPC client consumed it; installer did not | Remove Rust and script fields |
| Rust `os_host_name` descriptor field | No consumers outside descriptor tests | Remove field; keep script names used for safe upgrade/uninstall |
| `.env.example` | Advertises deleted Developer ID/notary workflow; install/release scripts do not load it | Remove stale template |
| Desktop absence guard | Blacklists source paths and crate names for a deleted product | Remove script and Just/quality wiring |
| Daemon-era absence guard and fixture suite | Broad daemon/session terminology scans require historical allowlists | Remove scripts and duplicate CI/Just/quality wiring |
| Inline TUI structural guard | Old JS TUI paths/keywords plus checks already covered by compilation | Remove script and quality wiring |
| Workspace absence guard | Mixes retired symbols/CLI paths with active Host-source safety | Remove historical checks; retain safety rules in `check-host-source-boundaries.sh` |
| Desktop negative tests | Only prove removed CLI/Skill names stay absent | Do not retain; normal CLI and Skill suites cover current behavior |
| Ignored Apple build products and secret patterns | Local artifacts still exist; canonical quality contract permits them | Preserve ignores and local data |
| macOS auth and sandbox adapters | Surviving Rust runtime consumers, explicitly retained by ADR-0054 | Keep |
| Old Host binary retirement | Installer validates manifest ownership and digest before removing a prior binary | Keep safety path |
| Anywhere, proactive memory, Groove Master, UPDF, Matter, rollout-history proposals | Explicit parked dispositions retain design inventory pending new decisions | Keep parked; do not equate deferred work with cancellation |
| Cancelled desktop/voice work and old ADRs | Historical evidence, not implementation authorization | Preserve immutable archive |

The Bash classifiers' `is_shell_control_prefix` functions recognize shell
language keywords; they are unrelated to the removed desktop control protocol.
The file-native `crates/shell` remains the Alan Shell owner.

## Verification

Use the remaining quality gate and focused CLI/Skill tests. Git inventory and
reference scans verify this deletion once; no permanent historical blacklist is
needed. Current sandbox, explicit descriptor/Host Mount, installer ownership,
rollback and migration data-protection tests remain. OpenSpec semantic checks
remain because they validate current normative surfaces and parked dispositions.

On 2026-09-29 the user explicitly requested removing unnecessary regression
checks under YAGNI. This supersedes the desktop-absence gate obligation; the
quality-gate delta records that change without restoring retired features.
