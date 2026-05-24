## Breaking Changes

- Bare `alan` is now the interactive Rust TUI entrypoint.
- `alan chat`, `alan ask`, `ALAN_TUI_PATH`, `clients/tui`, and the shipped
  `alan-tui` / `alan-dev-tui` executables are removed.
- macOS packaging, direct install, and Homebrew cask metadata now expose only
  the embedded `alan` or `alan-dev` command.

## Validation Evidence

- `cargo fmt --all`
- `cargo test -p alan-terminal-ui`
- `cargo test -p alan`
- `cargo test -p alan-runtime -p alan-auth`
- `cargo run -p alan --` from a noninteractive shell fails with the bare-`alan`
  terminal capability error before daemon startup.
- `bash scripts/check-rust-inline-tui-contract.sh`
- `bash scripts/check-daemon-api-route-strings.sh`
- `bash scripts/test-install-channel-descriptor.sh`
- `bash scripts/check-dev-channel-install-contract.sh`
- `bash scripts/validate-homebrew-cask.sh`
- `bash clients/apple/scripts/test-command-line-tool-installer.sh`
- `bash clients/apple/scripts/test-terminal-runtime-service.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `bash -n` over changed release/install/check scripts
- `openspec validate --all --strict`
- `openspec validate replace-typescript-tui-with-rust-inline-tui --strict`
- `git diff --check`
