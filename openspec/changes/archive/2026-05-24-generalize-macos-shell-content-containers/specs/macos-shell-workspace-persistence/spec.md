## ADDED Requirements

### Requirement: Workspace manifest stores content-container restore snapshots
After `generalize-macos-shell-content-containers`, the macOS shell workspace manifest SHALL
remain the workspace restore authority and SHALL store restorable PaneSlot / ContentInstance
snapshots instead of terminal-only pane snapshots.

#### Scenario: Terminal-only manifest upgrades to content-container shape
- **WHEN** alan 读取 `persist-macos-shell-workspaces` 产生的 terminal-only workspace manifest
- **THEN** alan 将每个 terminal restore leaf 升级为 PaneSlot 加 `terminal` ContentInstance restore payload
- **AND** Space/Tab IDs、ordering、selected Space/Tab、pin 状态、TTL anchor 和 active-task metadata 保持一致
- **AND** 后续 manifest 写入只使用 content-container restore shape

#### Scenario: Pinned mixed tab snapshot is saved
- **WHEN** 用户 pin 或 update-pin 一个包含 terminal、markdown 或 settings content 的 split tab
- **THEN** workspace manifest 保存 split tree、PaneSlot restore identity、ContentInstance kind 和每个 content 的 restorable payload
- **AND** terminal payload 保存 cwd、launch target 和用户可见 title
- **AND** markdown/settings payload 保存对应文件引用或 settings surface identity
- **AND** manifest 不保存 terminal process、renderer object、scrollback 或 delivery queue

#### Scenario: Unpinned mixed tab live snapshot is saved
- **WHEN** 未 pin tab 包含 terminal、markdown 或 settings content 且仍在 TTL 内
- **THEN** workspace manifest 的 live snapshot 保存 content-aware restore state
- **AND** lifecycle pruning 继续使用原有 `max(lastActivatedAt, lastActivityAt)`、pin 状态和 active-task metadata
- **AND** 非 terminal content 不会被误判为 terminal active task

#### Scenario: ShellStateSnapshot stays a runtime projection
- **WHEN** content-container migration 已经完成且 app 重新启动
- **THEN** alan 从 workspace manifest materialize v0.2 shell state
- **AND** `ShellStateSnapshot` 只发布当前 UI、control-plane 和 runtime projection
- **AND** `shell-state-window_main.json` 不重新成为 workspace restore authority
