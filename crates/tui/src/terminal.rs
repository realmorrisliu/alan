use anyhow::{Context, Result};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event as TerminalEvent,
};
use crossterm::{execute, terminal as crossterm_terminal};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{IsTerminal, Stdout, Write, stdout};
use std::time::Duration;

use crate::app::AppEvent;
use crate::ui;

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
        let mut out = stdout();
        execute!(out, EnableBracketedPaste, EnableMouseCapture)
            .context("failed to enable terminal input modes")?;
        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend).context("failed to initialize terminal")?;
        terminal.clear().context("failed to clear terminal")?;
        Ok(Self { terminal })
    }

    pub fn draw(&mut self, app: &crate::app::TuiApp) -> Result<()> {
        self.terminal
            .draw(|frame| ui::draw(frame, app))
            .map(|_| ())
            .context("failed to draw terminal frame")
    }

    pub fn viewport_height(&self) -> usize {
        self.terminal
            .size()
            .map(|area| area.height as usize)
            .unwrap_or(24)
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

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = crossterm_terminal::disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            DisableBracketedPaste
        );
        let _ = self.terminal.show_cursor();
    }
}

pub fn spawn_terminal_events(tx: tokio::sync::mpsc::Sender<AppEvent>) {
    tokio::task::spawn_blocking(move || {
        loop {
            match crossterm::event::poll(Duration::from_millis(100)) {
                Ok(true) => match crossterm::event::read() {
                    Ok(event) => {
                        let should_quit = matches!(
                            event,
                            TerminalEvent::Key(crossterm::event::KeyEvent {
                                code: crossterm::event::KeyCode::Char('q'),
                                modifiers,
                                ..
                            }) if modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                        );
                        if tx.blocking_send(AppEvent::Terminal(event)).is_err() || should_quit {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.blocking_send(AppEvent::Error(format!(
                            "terminal input failed: {err}"
                        )));
                        break;
                    }
                },
                Ok(false) => {}
                Err(err) => {
                    let _ = tx
                        .blocking_send(AppEvent::Error(format!("terminal polling failed: {err}")));
                    break;
                }
            }
        }
    });
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
