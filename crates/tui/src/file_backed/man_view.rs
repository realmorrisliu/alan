//! In-TUI `/man [topic]` reader backed by the mounted `/man` namespace (with builtin fallback).

use ratatui::text::{Line, Span};

use crate::history::clean_text;
use crate::theme::Theme;

use super::app::{FileBackedApp, FileBackedEvent};
use super::tape_view::ink_tape_line;

const TOPICS: &[(&str, &str, &str)] = &[
    (
        "1/alan",
        "alan",
        "ALAN(1)                  Alan Manual                  ALAN(1)\n\nNAME\n  alan - programmable personal computing environment on alan9\n\nSYNOPSIS\n  alan [OPTIONS]\n  alan -p, --print <PROMPT> [--json] [--image <PATH>...]\n  echo <TASK> | alan [--json]\n  alan about | alan man [TOPIC]\n\nDESCRIPTION\n  Each bare `alan` invocation boots one foreground alan9 instance and\n  attaches its file-backed TUI to /agent/root. Targets standard POSIX/VT\n  terminals (Ghostty, iTerm2, Kitty, WezTerm, Terminal.app, tmux, SSH).\n\nENVIRONMENT\n  ALAN_THEME=auto|midnight|paper|ansi|dumb · NO_COLOR · TERM=dumb · ALAN_NO_WELCOME=1",
    ),
    (
        "1/q",
        "q",
        "Q(1)                    Quartermaster                   Q(1)\n\nNAME\n  q - Package Service client (/bin/q -> /srv/package)\n\nPACKAGE KINDS (ADR-0061)\n  skill | mcp | tool | provider\n\nBOUNDARIES\n  Operates strictly over authorized /mnt/... namespace paths; never\n  accepts raw Host paths or performs implicit network fetches.",
    ),
    (
        "1/connection",
        "connection",
        "CONNECTION(1)           Connection Service           CONNECTION(1)\n\nFIRST-PARTY PROVIDERS (ADR-0061)\n  magpie · openai_responses · chatgpt · vertex_gemini · openrouter\n  (Anthropic is a protocol adapter / Quartermaster `q` package.)\n\nCOMMANDS\n  alan connection list | current | add | login | set-secret | default set | test",
    ),
    (
        "4/agentfs",
        "agentfs",
        "AGENTFS(4)               File Servers               AGENTFS(4)\n\nDESCRIPTION\n  /proc/<pid> is lifecycle truth; /agent/<pid> projects io/, requests/,\n  actions/, and machine/{tape,state,checkpoints,ctl,ui/}.",
    ),
    (
        "7/alan9",
        "alan9",
        "ALAN9(7)                Architecture                ALAN9(7)\n\nSTANDARD NAMESPACE\n  /proc   Process lifecycle truth\n  /agent  AgentFS views of Agent Processes (/agent/<pid>)\n  /srv    Mountable file-server handles (aP/1 protocol)\n  /bin    Tools and Agent Executables (/bin/alan, /bin/q)\n  /lib    Immutable projected Skill/package trees\n  /man    Read-only UNIX manual pages\n  /mnt    Explicitly authorized Host Mounts",
    ),
];

/// Return immediate fallback content for a `/man [topic]` request before or without `/man` mount.
pub(in crate::file_backed) fn fallback_content(topic: &str) -> (String, String) {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return ("/man".to_string(), fallback_index());
    }
    if let Some((path, _, body)) = lookup_fallback(trimmed) {
        return (format!("/man/{path}"), body.to_string());
    }
    (
        format!("/man/{trimmed}"),
        format!(
            "No manual entry for `{trimmed}`.\n\nAvailable topics: alan, q, connection, agentfs, alan9\nTry `/man` for the index."
        ),
    )
}

fn lookup_fallback(topic: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    let normalized = topic
        .trim()
        .strip_prefix("/man/")
        .unwrap_or(topic.trim())
        .trim_matches('/')
        .to_ascii_lowercase();
    let without_parens = normalized
        .rsplit_once('(')
        .and_then(|(name, rest)| rest.ends_with(')').then_some(name))
        .unwrap_or(normalized.as_str());
    TOPICS
        .iter()
        .find(|(path, name, _)| *path == without_parens || *name == without_parens)
}

fn fallback_index() -> String {
    [
        "ALAN(1)                   Manual Index                   ALAN(1)",
        "",
        "AVAILABLE MANUAL PAGES (/man)",
        "  /man/1/alan         alan(1)         environment and foreground shell",
        "  /man/1/q            q(1)            Quartermaster package command",
        "  /man/1/connection   connection(1)   provider connection profiles",
        "  /man/4/agentfs      agentfs(4)      Agent Process file layout",
        "  /man/7/alan9        alan9(7)        Standard Namespace and architecture",
        "",
        "Usage: /man <topic>   (or `alan man <topic>` from the shell)",
    ]
    .join("\n")
}

/// Spawn an async read of `/man` via `Shell`, falling back cleanly if `/man` is unmounted.
pub(in crate::file_backed) fn spawn_read(
    shell: &alan_shell::Shell,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    topic: String,
    generation: u64,
) {
    let shell = shell.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let (owner, content) = read_via_shell(&shell, &topic).await;
        let _ = tx
            .send(FileBackedEvent::ManDetails {
                generation,
                owner,
                content,
            })
            .await;
    });
}

async fn read_via_shell(shell: &alan_shell::Shell, topic: &str) -> (String, String) {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        if let Ok(sections) = shell.ls("/man").await
            && !sections.is_empty()
        {
            let mut entries = Vec::new();
            for sec in sections {
                if let Ok(pages) = shell.ls(&format!("/man/{sec}")).await {
                    for page in pages {
                        entries.push(format!("  /man/{sec}/{page}"));
                    }
                }
            }
            if !entries.is_empty() {
                let mut out = vec![
                    "ALAN(1)                   Manual Index                   ALAN(1)".to_string(),
                    String::new(),
                    "AVAILABLE MANUAL PAGES (/man)".to_string(),
                ];
                out.extend(entries);
                out.push(String::new());
                out.push("Usage: /man <topic>   (e.g. /man alan9, /man 1/q)".to_string());
                return ("/man".to_string(), out.join("\n"));
            }
        }
        return fallback_content("");
    }

    let norm = trimmed
        .strip_prefix("/man/")
        .unwrap_or(trimmed)
        .trim_matches('/');
    let candidates = if norm.contains('/') {
        vec![format!("/man/{norm}")]
    } else {
        vec![
            format!("/man/1/{norm}"),
            format!("/man/7/{norm}"),
            format!("/man/4/{norm}"),
            format!("/man/{norm}"),
        ]
    };
    for path in candidates {
        if let Ok(bytes) = shell.cat(&path).await
            && let Ok(text) = String::from_utf8(bytes)
            && !text.trim().is_empty()
        {
            return (path, text);
        }
    }
    fallback_content(trimmed)
}

/// Format a manual page into styled lines using the active [`Theme`].
pub(in crate::file_backed) fn format_man_lines(text: &str, theme: Theme) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let cleaned = clean_text(line);
        if theme.is_dumb {
            rows.push(Line::raw(cleaned));
            continue;
        }
        let trimmed = cleaned.trim();
        let is_section_heading = !trimmed.is_empty()
            && idx > 0
            && !cleaned.starts_with(' ')
            && trimmed.chars().all(|c| {
                c.is_ascii_uppercase()
                    || c.is_ascii_digit()
                    || matches!(c, ' ' | '-' | '(' | ')' | '/' | ',' | '&')
            });
        if (idx == 0 && !trimmed.is_empty()) || is_section_heading {
            rows.push(Line::styled(cleaned, theme.accent_bold()));
        } else if let Some(rest) = cleaned.strip_prefix("  ")
            && let Some((left, right)) = split_two_col_man(rest)
        {
            rows.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(left.to_string(), theme.accent_style()),
                Span::raw(right.to_string()),
            ]));
        } else {
            rows.push(ink_tape_line(&cleaned, theme));
        }
    }
    rows
}

fn split_two_col_man(rest: &str) -> Option<(&str, &str)> {
    let gap = rest.find("   ")?;
    if gap == 0 || gap > 28 {
        return None;
    }
    Some((&rest[..gap], &rest[gap..]))
}

impl FileBackedApp {
    pub(in crate::file_backed) fn apply_man_details(
        &mut self,
        generation: u64,
        owner: String,
        content: String,
    ) {
        if self.modal.active
            && self.modal.mode == super::app::action_modal::ModalMode::Man
            && self.modal.generation == generation
        {
            self.modal.owner_path = owner;
            self.modal.rows = format_man_lines(&content, self.theme);
            self.modal.scroll = 0;
        }
    }
}
