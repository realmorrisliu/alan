# Bounded legacy macOS cleanup record

This record is the only active-tree artifact allowed to name the retired local
surfaces during the hard cut. It records sanitized paths and classifications;
it is not a cleanup executable, migration reader, or compatibility contract.

## Dry inventory

Inventory date: 2026-07-12 (Asia/Shanghai)

| Sanitized path or pattern | Observed state | Ownership classification | Intended action |
| --- | --- | --- | --- |
| `~/Library/Application Support/AlanNative/` | Present; 60 files in 2 directories, 241,664 allocated bytes; 58 files match `shell-state-*.json` | Exact retired Alan support root, owned by the current user | Remove the whole tree after operator confirmation |
| `~/Library/Application Support/alan-macos/shell-state-*.json` | 257 files, 707,871 bytes | Retired Alan shell-state projection inside the current stable root, owned by the current user | Remove only matching files; preserve every other stable-root entry |
| `~/Library/Application Support/alan-macos-dev/shell-state-*.json` | 69 files, 142,708 bytes | Retired Alan shell-state projection inside the current dev root, owned by the current user | Remove only matching files; preserve every other dev-root entry |
| `~/Library/Application Support/alan-macos/shell-workspace-window_main.json` | Current schema `1` / content contract `0.2`, five Spaces, and a non-null retired quick-terminal field | Current Alan workspace manifest with unsupported legacy content | Preserve original bytes; the hard-cut loader must quarantine them as corrupt evidence and create a current default |
| `~/Library/Application Support/alan-macos-dev/shell-workspace-window_main.json` | Current schema `1` / content contract `0.2`, seven Spaces, no retired quick-terminal field | Current Alan workspace manifest | Leave untouched |
| `~/Library/Application Support/alan-macos-dev/terminal-profiles.json` | One profile is both Managed-User-owned and `sudo_user`; the Login shell profile is also present | Retired Alan-managed profile entry inside a current profile document | Remove only the retired profile entry; preserve the document and all non-managed profiles |
| `~/Library/Application Support/alan-macos-dev/managed-terminal-users.json` | One current Managed User catalog entry | Current Alan catalog | Leave untouched; do not remove an account or home directory |
| `~/Applications/alan.app` | Resolves to the same inode and stable bundle identifier as `~/Applications/Alan.app` | Case-insensitive spelling of the current canonical bundle, not a distinct retired bundle | Leave untouched |
| `/Applications/alan.app` | Absent | No candidate | No action |
| `/usr/local/bin/alan` and `/usr/local/bin/alan-dev` | Canonical links to `Alan.app` and `Alan Dev.app` respectively | Current channel-owned links | Leave untouched |
| Other inspected CLI link locations | No `alan` or `alan-dev` link present | No candidate | No action |
| `/etc/sudoers.d/alan-terminal-<gui-user>-to-<managed-user>` | Expected candidate absent; `/etc/sudoers.d` is empty | No verified Alan-owned legacy sudoers entry | No privileged deletion; do not inspect or mutate unrelated sudoers state |

No terminal-only workspace manifest was found. No distinct lowercase bundle,
lowercase-bundle link, ambiguous file, non-Alan-owned link, verified legacy
sudoers entry, account, or home directory is authorized for deletion.

## Operator confirmation and cleanup result

The operator explicitly confirmed every deletion class above on 2026-07-12.
Ambiguous files, non-Alan-owned links, Unix accounts, home directories, current
manifests, the Managed User catalog, current apps, and current CLI links remained
outside the authorized deletion set.

The bounded cleanup then produced these sanitized results:

| Class | Result |
| --- | --- |
| Historical `AlanNative` support root | Removed; follow-up inventory reports the path absent |
| Stable Application Support `shell-state-*.json` | 257 files removed; the still-running pre-hard-cut stable app subsequently recreated one file, proving the old binary was still an active writer |
| Dev Application Support `shell-state-*.json` | 69 files removed; follow-up inventory reports zero files |
| Managed-User-owned `sudo_user` profile | The single `univer` entry was removed; the Login shell profile and profile document were preserved |
| Current manifests and Managed User catalog | Preserved |
| Lowercase bundle and current CLI links | Left untouched because inventory proved they resolve to current channel-owned artifacts |
| Candidate Alan sudoers entries | None found; no privileged deletion was attempted |
| Unix accounts and home directories | Untouched |

No cleanup command, compatibility reader, or migration executable is intended
to remain in the merged tree. The source hard cut and fresh Alan Dev validation
must prove that a current binary does not recreate Application Support
shell-state files.

The operator later instructed that the running stable Alan instance MUST remain
open. Its single regenerated shell-state file is therefore an explicit local
hold, not an authorized reason to terminate or replace the stable app. It can be
removed after that pre-hard-cut binary is voluntarily closed or upgraded. Alan
Dev remained at zero Application Support shell-state files after fresh launch.

## Fresh Alan Dev verification

Verification date: 2026-07-12 (Asia/Shanghai)

- A freshly installed and relaunched `Alan Dev.app` loaded the current schema `1` /
  content contract `0.2` workspace manifest. Its Application Support root still
  contained zero `shell-state-*.json` files after launch.
- `ALAN_INSTALL_CHANNEL=dev alan-dev shell state` read the seven-Space current
  workspace through the temporary control-plane namespace. The projection did
  not contain `quick_terminal`.
- Settings > Terminal showed only the current Login shell profile and current
  Managed User account/home/shell/ownership-marker surface. No legacy cleanup
  row, legacy sudoers status, or compatibility action was visible.
- The isolated Apple UI smoke completed workspace restart restore, cleared the
  restored transcript, proved the cleared transcript stayed absent after a
  second relaunch, and verified post-relaunch input plus cwd. Evidence is in
  `/private/tmp/alan-legacy-ui-smoke-5/manifest.txt` and screenshots
  `09-restart-restore.png`, `10-restart-clear.png`, and
  `12-restart-after-input.png`.
- The operator explicitly authorized installing the Alan Dev privileged helper
  and running the real helper-owned Managed User PTY smoke. The current Dev app
  and embedded helper were installed, but the helper could not launch: the local
  build is ad-hoc signed, macOS rejected it with a launch-constraint code-signing
  violation, and the submitted job remained at `EX_CONFIG`. A retry through the
  supported `SMAppService` registration path returned
  `SMAppServiceErrorDomain Code=57` (`Socket is not connected`).
- The repository supports a non-ad-hoc identity through
  `ALAN_SIGNING_IDENTITY` / `ALAN_DEVELOPER_ID_APPLICATION`, but
  `security find-identity -v -p codesigning` reported zero valid identities in
  the current keychain. Therefore a real helper-owned Managed User PTY launch
  remains unverified in this environment. No AMFI, SIP, `launchctl`, or manual
  helper-launch bypass was used; focused helper and fake-client tests cover the
  current code path meanwhile.
- The running pre-hard-cut stable Alan instance remains open by explicit operator
  instruction. Verification and smoke shutdown targeted only isolated Alan Dev
  PIDs and did not signal, restart, or replace stable Alan.
