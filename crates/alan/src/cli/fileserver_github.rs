//! Hidden internal CLI entrypoint backing the `github-fileserver` Quartermaster package.

use std::path::PathBuf;
use std::sync::Arc;

use alan_service_manager::githubfs::{GhCliGithubBackend, GithubFs, serve_stdio};
use anyhow::Result;
use clap::Args;

const ENV_GITHUBFS_GH_BIN: &str = "ALAN_GITHUBFS_GH_BIN";

/// Hidden arguments for `alan __fileserver-github`.
#[derive(Debug, Default, Args)]
pub struct FileserverGithubArgs {
    /// Optional explicit `gh` CLI executable path (also configurable via `ALAN_GITHUBFS_GH_BIN`).
    #[arg(long = "gh-bin", hide = true)]
    pub gh_bin: Option<PathBuf>,
}

/// Serve `githubfs` over stdio using framed `aP` wire transport.
pub async fn run(args: FileserverGithubArgs) -> Result<()> {
    let gh_bin = args.gh_bin.or_else(|| {
        std::env::var_os(ENV_GITHUBFS_GH_BIN)
            .filter(|val| !val.is_empty())
            .map(PathBuf::from)
    });
    let backend = match gh_bin {
        Some(path) => GhCliGithubBackend::with_binary(path),
        None => GhCliGithubBackend::new(),
    };
    let server = Arc::new(GithubFs::new(Arc::new(backend)));
    serve_stdio(server)
        .await
        .map_err(|err| anyhow::anyhow!("githubfs stdio server failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::dispatch::{Cli, Commands, uses_product_stores};
    use clap::{CommandFactory, Parser};

    #[test]
    fn hidden_fileserver_github_subcommand_parses_and_stays_hidden_from_help() {
        let mut help = Vec::new();
        Cli::command().write_long_help(&mut help).unwrap();
        let help_text = String::from_utf8(help).unwrap();
        assert!(
            !help_text.contains("__fileserver-github"),
            "hidden subcommand leaked into help:\n{help_text}"
        );

        let parsed = Cli::try_parse_from(["alan", "__fileserver-github"]).unwrap();
        assert!(!uses_product_stores(parsed.command.as_ref()));
        assert!(matches!(
            parsed.command,
            Some(Commands::FileserverGithub(FileserverGithubArgs {
                gh_bin: None
            }))
        ));

        let with_bin =
            Cli::try_parse_from(["alan", "__fileserver-github", "--gh-bin", "/tmp/fixture-gh"])
                .unwrap();
        assert!(matches!(
            with_bin.command,
            Some(Commands::FileserverGithub(FileserverGithubArgs {
                gh_bin: Some(ref path)
            })) if path == std::path::Path::new("/tmp/fixture-gh")
        ));
    }
}
