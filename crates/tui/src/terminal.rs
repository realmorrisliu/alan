use anyhow::{Context, Result};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::{execute, terminal as crossterm_terminal};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{IsTerminal, Stdout, Write, stdout};

pub type AlanTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn is_interactive_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub fn terminal_capability_error() -> &'static str {
    "bare `alan` requires an interactive terminal; use explicit management subcommands for noninteractive automation"
}

pub struct TerminalSession {
    terminal: AlanTerminal,
}

impl TerminalSession {
    pub fn enter() -> Result<Self> {
        crossterm_terminal::enable_raw_mode().context("failed to enable raw terminal mode")?;
        let startup_guard = TerminalStartupGuard::new();
        let mut out = stdout();
        execute!(out, EnableBracketedPaste).context("failed to enable terminal input modes")?;
        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend).context("failed to initialize terminal")?;
        terminal.clear().context("failed to clear terminal")?;
        startup_guard.disarm();
        Ok(Self { terminal })
    }

    pub fn draw_with<F>(&mut self, draw: F) -> Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal
            .draw(draw)
            .map(|_| ())
            .context("failed to draw terminal frame")
    }

    pub fn viewport_height(&self) -> usize {
        self.terminal
            .size()
            .map(|area| area.height as usize)
            .unwrap_or(24)
    }

    pub fn viewport_size(&self) -> (usize, usize) {
        self.terminal
            .size()
            .map(|area| (area.width as usize, area.height as usize))
            .unwrap_or((80, 24))
    }

    pub fn write_scrollback(&mut self, lines: &[String]) -> Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        let out = self.terminal.backend_mut();
        for line in lines {
            writeln!(out, "{line}")?;
        }
        out.flush()?;
        Ok(())
    }
}

struct TerminalStartupGuard {
    armed: bool,
}

impl TerminalStartupGuard {
    fn new() -> Self {
        Self { armed: true }
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for TerminalStartupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut out = stdout();
        let _ = execute!(out, DisableBracketedPaste);
        let _ = crossterm_terminal::disable_raw_mode();
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = crossterm_terminal::disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), DisableBracketedPaste);
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_error_names_bare_alan_contract() {
        assert!(terminal_capability_error().contains("bare `alan`"));
        assert!(!terminal_capability_error().contains("alan-tui"));
    }
}
