//! Alan Shell command-line surface and dispatch adapter.

use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};

use crate::cli;

#[derive(Args, Clone)]
pub(super) struct ShellTargetArgs {
    /// Explicit `alan shell` socket path
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Explicit `alan shell` control directory
    #[arg(long = "control-dir")]
    control_dir: Option<PathBuf>,
    /// Window id used to derive the local alan shell control directory
    #[arg(long)]
    window: Option<String>,
    /// Timeout for IPC requests in milliseconds
    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,
}

#[derive(Subcommand)]
pub(super) enum ShellAction {
    /// Print the canonical shell state snapshot
    State {
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Operate on shell spaces
    Space {
        #[command(subcommand)]
        action: ShellSpaceAction,
    },
    /// Operate on shell tabs
    Tab {
        #[command(subcommand)]
        action: ShellTabAction,
    },
    /// Operate on shell panes
    Pane {
        #[command(subcommand)]
        action: ShellPaneAction,
    },
    /// Operate on terminal content
    Terminal {
        #[command(subcommand)]
        action: ShellTerminalAction,
    },
    /// Attention inbox and overrides
    Attention {
        #[command(subcommand)]
        action: ShellAttentionAction,
    },
    /// Rank candidate panes for shell routing
    Routing {
        #[command(subcommand)]
        action: ShellRoutingAction,
    },
    /// Read shell events or follow the event stream
    Events {
        /// Resume after this event id
        #[arg(long = "after-event-id")]
        after_event_id: Option<String>,
        /// Maximum number of events per read
        #[arg(long)]
        limit: Option<u64>,
        /// Keep polling and emit NDJSON as new events arrive
        #[arg(long)]
        follow: bool,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
pub(super) enum ShellSpaceAction {
    /// List spaces
    List {
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Create a new space
    Create {
        /// Optional title for the space
        #[arg(long)]
        title: Option<String>,
        /// Optional working directory for the initial pane
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Open a new alan space directly
    OpenAlan {
        /// Optional title for the space
        #[arg(long)]
        title: Option<String>,
        /// Optional working directory for the initial pane
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
pub(super) enum ShellTabAction {
    /// List tabs
    List {
        /// Restrict to a specific space
        #[arg(long)]
        space: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Open a new tab
    Open {
        /// Target space id
        #[arg(long)]
        space: Option<String>,
        /// Optional tab title
        #[arg(long)]
        title: Option<String>,
        /// Optional working directory for the initial pane
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Close a tab
    Close {
        /// Tab id to close
        #[arg(long)]
        tab: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
pub(super) enum ShellPaneAction {
    /// List panes
    List {
        /// Restrict to a specific tab
        #[arg(long)]
        tab: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Print a single pane snapshot
    Snapshot {
        /// Pane id to inspect
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Split a pane
    Split {
        /// Pane id to split
        #[arg(long)]
        pane: String,
        /// Split direction
        #[arg(long, value_parser = ["horizontal", "vertical"])]
        direction: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Close a pane
    Close {
        /// Pane id to close
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Move a pane into its own tab
    Lift {
        /// Pane id to lift
        #[arg(long)]
        pane: String,
        /// Optional title for the new tab
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Move a pane into another existing tab
    Move {
        /// Pane id to move
        #[arg(long)]
        pane: String,
        /// Target tab id
        #[arg(long)]
        tab: String,
        /// Split direction used when attaching onto the destination tab
        #[arg(long, default_value = "vertical", value_parser = ["horizontal", "vertical"])]
        direction: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Move a pane inside its current tab
    MoveWithinTab {
        /// Pane id to move
        #[arg(long)]
        pane: String,
        /// Placement relative to the adjacent pane
        #[arg(long, value_parser = ["left", "right", "up", "down"])]
        placement: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Focus a pane
    Focus {
        /// Pane id to focus
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Focus an adjacent pane by spatial direction
    SpatialFocus {
        /// Spatial direction to focus
        #[arg(long, value_parser = ["left", "right", "up", "down"])]
        direction: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Resize a split node
    ResizeSplit {
        /// Split node id to resize
        #[arg(long)]
        split_node: String,
        /// Resulting split ratio
        #[arg(long)]
        ratio: f64,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Equalize split ratios in a tab
    EqualizeSplits {
        /// Optional tab id; defaults to the selected tab
        #[arg(long)]
        tab: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Zoom a split pane
    Zoom {
        /// Pane id to zoom
        #[arg(long)]
        pane: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Unzoom a tab
    Unzoom {
        /// Optional pane id whose tab should unzoom
        #[arg(long)]
        pane: Option<String>,
        /// Optional tab id; defaults to the selected tab
        #[arg(long)]
        tab: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Deprecated alias for `terminal send-text --pane`
    #[command(hide = true)]
    SendText {
        /// Pane id to target
        #[arg(long)]
        pane: String,
        /// Text to send
        #[arg(long)]
        text: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
pub(super) enum ShellTerminalAction {
    /// Send text to terminal content
    SendText {
        /// PaneSlot id to resolve to terminal content
        #[arg(long, conflicts_with = "content")]
        pane: Option<String>,
        /// Terminal content id to target directly
        #[arg(long, conflicts_with = "pane")]
        content: Option<String>,
        /// Text to send
        #[arg(long)]
        text: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
pub(super) enum ShellAttentionAction {
    /// List panes that currently require attention
    Inbox {
        #[command(flatten)]
        target: ShellTargetArgs,
    },
    /// Override a pane attention state
    Set {
        /// Pane id to target
        #[arg(long)]
        pane: String,
        /// Attention state
        #[arg(long, value_parser = ["idle", "active", "awaiting_user", "notable"])]
        state: String,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

#[derive(Subcommand)]
pub(super) enum ShellRoutingAction {
    /// Rank candidate panes for intent routing
    Candidates {
        /// Optional preferred pane id
        #[arg(long)]
        pane: Option<String>,
        #[command(flatten)]
        target: ShellTargetArgs,
    },
}

pub(super) fn run(action: ShellAction) -> Result<()> {
    match action {
        ShellAction::State { target } => {
            cli::shell::run_shell_state(shell_target_options(target))?;
        }
        ShellAction::Space { action } => match action {
            ShellSpaceAction::List { target } => {
                cli::shell::run_shell_space_list(shell_target_options(target))?;
            }
            ShellSpaceAction::Create { title, cwd, target } => {
                cli::shell::run_shell_space_create(
                    title.as_deref(),
                    cwd.as_ref().map(|path| path_to_string(path)).as_deref(),
                    shell_target_options(target),
                )?;
            }
            ShellSpaceAction::OpenAlan { title, cwd, target } => {
                cli::shell::run_shell_space_open_alan(
                    title.as_deref(),
                    cwd.as_ref().map(|path| path_to_string(path)).as_deref(),
                    shell_target_options(target),
                )?;
            }
        },
        ShellAction::Tab { action } => match action {
            ShellTabAction::List { space, target } => {
                cli::shell::run_shell_tab_list(space.as_deref(), shell_target_options(target))?;
            }
            ShellTabAction::Open {
                space,
                title,
                cwd,
                target,
            } => {
                cli::shell::run_shell_tab_open(
                    space.as_deref(),
                    title.as_deref(),
                    cwd.as_ref().map(|path| path_to_string(path)).as_deref(),
                    shell_target_options(target),
                )?;
            }
            ShellTabAction::Close { tab, target } => {
                cli::shell::run_shell_tab_close(&tab, shell_target_options(target))?;
            }
        },
        ShellAction::Pane { action } => match action {
            ShellPaneAction::List { tab, target } => {
                cli::shell::run_shell_pane_list(tab.as_deref(), shell_target_options(target))?;
            }
            ShellPaneAction::Snapshot { pane, target } => {
                cli::shell::run_shell_pane_snapshot(&pane, shell_target_options(target))?;
            }
            ShellPaneAction::Split {
                pane,
                direction,
                target,
            } => {
                cli::shell::run_shell_pane_split(&pane, &direction, shell_target_options(target))?;
            }
            ShellPaneAction::Close { pane, target } => {
                cli::shell::run_shell_pane_close(&pane, shell_target_options(target))?;
            }
            ShellPaneAction::Lift {
                pane,
                title,
                target,
            } => {
                cli::shell::run_shell_pane_lift(
                    &pane,
                    title.as_deref(),
                    shell_target_options(target),
                )?;
            }
            ShellPaneAction::Move {
                pane,
                tab,
                direction,
                target,
            } => {
                cli::shell::run_shell_pane_move(
                    &pane,
                    &tab,
                    &direction,
                    shell_target_options(target),
                )?;
            }
            ShellPaneAction::MoveWithinTab {
                pane,
                placement,
                target,
            } => {
                cli::shell::run_shell_pane_move_within_tab(
                    &pane,
                    &placement,
                    shell_target_options(target),
                )?;
            }
            ShellPaneAction::Focus { pane, target } => {
                cli::shell::run_shell_pane_focus(&pane, shell_target_options(target))?;
            }
            ShellPaneAction::SpatialFocus { direction, target } => {
                cli::shell::run_shell_pane_spatial_focus(&direction, shell_target_options(target))?;
            }
            ShellPaneAction::ResizeSplit {
                split_node,
                ratio,
                target,
            } => {
                cli::shell::run_shell_pane_resize_split(
                    &split_node,
                    ratio,
                    shell_target_options(target),
                )?;
            }
            ShellPaneAction::EqualizeSplits { tab, target } => {
                cli::shell::run_shell_pane_equalize_splits(
                    tab.as_deref(),
                    shell_target_options(target),
                )?;
            }
            ShellPaneAction::Zoom { pane, target } => {
                cli::shell::run_shell_pane_zoom(&pane, shell_target_options(target))?;
            }
            ShellPaneAction::Unzoom { pane, tab, target } => {
                cli::shell::run_shell_pane_unzoom(
                    pane.as_deref(),
                    tab.as_deref(),
                    shell_target_options(target),
                )?;
            }
            ShellPaneAction::SendText { pane, text, target } => {
                cli::shell::run_shell_pane_send_text(&pane, &text, shell_target_options(target))?;
            }
        },
        ShellAction::Terminal { action } => match action {
            ShellTerminalAction::SendText {
                pane,
                content,
                text,
                target,
            } => {
                if pane.is_none() && content.is_none() {
                    anyhow::bail!("terminal send-text requires --pane or --content");
                }
                cli::shell::run_shell_terminal_send_text(
                    pane.as_deref(),
                    content.as_deref(),
                    &text,
                    shell_target_options(target),
                )?;
            }
        },
        ShellAction::Attention { action } => match action {
            ShellAttentionAction::Inbox { target } => {
                cli::shell::run_shell_attention_inbox(shell_target_options(target))?;
            }
            ShellAttentionAction::Set {
                pane,
                state,
                target,
            } => {
                cli::shell::run_shell_attention_set(&pane, &state, shell_target_options(target))?;
            }
        },
        ShellAction::Routing { action } => match action {
            ShellRoutingAction::Candidates { pane, target } => {
                cli::shell::run_shell_routing_candidates(
                    pane.as_deref(),
                    shell_target_options(target),
                )?;
            }
        },
        ShellAction::Events {
            after_event_id,
            limit,
            follow,
            target,
        } => {
            cli::shell::run_shell_events(
                after_event_id.as_deref(),
                limit,
                follow,
                shell_target_options(target),
            )?;
        }
    }

    Ok(())
}

fn shell_target_options(args: ShellTargetArgs) -> cli::shell::ShellTargetOptions {
    cli::shell::ShellTargetOptions {
        socket: args.socket,
        control_dir: args.control_dir,
        window: args.window,
        timeout_ms: args.timeout_ms,
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
