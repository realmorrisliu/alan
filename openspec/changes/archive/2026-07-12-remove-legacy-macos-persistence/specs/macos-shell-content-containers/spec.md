## MODIFIED Requirements

### Requirement: Content state persists across app restore
alan SHALL 通过 current-schema workspace manifest 持久化通用 container state、PaneSlots 和
ContentInstances，使 app restore 后能恢复 tab、split 和 content kind，而不会把非 terminal
content 误恢复为 terminal。Content restore SHALL NOT decode or convert historical terminal-only
workspace manifests or persistent shell-state files.

#### Scenario: App restores mixed content tab
- **WHEN** alan 重新打开之前包含 terminal、markdown 和 settings pane 的窗口
- **THEN** shell state 恢复同一 tab、split tree、PaneSlot IDs 和每个 ContentInstance 的 kind
- **AND** terminal content 从 manifest 中的 terminal restore payload 创建新的 terminal runtime，而不是恢复上一轮 app 进程中的 OS process
- **AND** markdown/settings content 恢复为各自的 viewer/settings surface

#### Scenario: Historical persistence input does not materialize content
- **WHEN** restore encounters a terminal-only workspace manifest or persistent
  `shell-state-window_main.json`
- **THEN** alan does not decode or convert that input into PaneSlots or ContentInstances
- **AND** unsupported workspace-manifest bytes follow the current corrupt-evidence path
