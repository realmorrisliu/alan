use anyhow::{Context, Result};
use crossterm::cursor::{MoveTo, MoveToNextLine};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, terminal as crossterm_terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};
use std::io::{IsTerminal, Stdout, Write, stdout};

use crate::transcript_ui::{style_transcript_line, wrapped_line_count};

pub type AlanTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn is_interactive_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub fn terminal_capability_error() -> &'static str {
    "bare `alan` requires an interactive terminal; use explicit management subcommands for noninteractive automation"
}

pub struct TerminalSession {
    terminal: AlanTerminal,
    viewport_height: u16,
}

impl TerminalSession {
    pub fn enter() -> Result<Self> {
        crossterm_terminal::enable_raw_mode().context("failed to enable raw terminal mode")?;
        let startup_guard = TerminalStartupGuard::new();
        let mut out = stdout();
        execute!(out, EnableBracketedPaste).context("failed to enable terminal input modes")?;
        let mut terminal = build_inline_terminal(out, 1)?;
        terminal.clear().context("failed to clear terminal")?;
        startup_guard.disarm();
        Ok(Self {
            terminal,
            viewport_height: 1,
        })
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

    pub fn viewport_size(&self) -> (usize, usize) {
        self.terminal
            .size()
            .map(|area| (area.width as usize, area.height as usize))
            .unwrap_or((80, 24))
    }

    pub fn set_inline_height(&mut self, height: u16) -> Result<()> {
        let terminal_size = self
            .terminal
            .size()
            .context("failed to read terminal size")?;
        let height = height.max(1).min(terminal_size.height.max(1));
        if height == self.viewport_height {
            return Ok(());
        }

        let top = self.terminal.get_frame().area().y;
        execute!(
            self.terminal.backend_mut(),
            MoveTo(0, top),
            Clear(ClearType::FromCursorDown),
            MoveTo(0, top)
        )
        .context("failed to clear the previous inline viewport")?;

        let terminal = build_inline_terminal(stdout(), height)?;
        self.terminal = terminal;
        self.viewport_height = height;
        Ok(())
    }

    pub fn write_scrollback(&mut self, lines: &[String]) -> Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        let width = self
            .terminal
            .size()
            .context("failed to read terminal size for scrollback")?
            .width as usize;
        let max_height = u16::MAX as usize;
        let mut chunk = Vec::new();
        let mut chunk_height = 0usize;
        for line in lines {
            let styled = style_transcript_line(line.clone());
            let line_height = wrapped_line_count(std::slice::from_ref(&styled), width).max(1);
            anyhow::ensure!(
                line_height <= max_height,
                "transcript line exceeds terminal scrollback insertion height"
            );
            if !chunk.is_empty() && chunk_height + line_height > max_height {
                self.insert_scrollback_chunk(std::mem::take(&mut chunk), chunk_height as u16)?;
                chunk_height = 0;
            }
            chunk.push(styled);
            chunk_height += line_height;
        }
        if !chunk.is_empty() {
            self.insert_scrollback_chunk(chunk, chunk_height as u16)?;
        }
        Ok(())
    }

    fn insert_scrollback_chunk(&mut self, lines: Vec<Line<'static>>, height: u16) -> Result<()> {
        self.terminal
            .insert_before(height, |buf| {
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .render(buf.area, buf);
            })
            .context("failed to append transcript to terminal scrollback")
    }
}

fn build_inline_terminal(out: Stdout, height: u16) -> Result<AlanTerminal> {
    let backend = CrosstermBackend::new(out);
    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
    .context("failed to initialize terminal")
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
        let _ = restore_terminal_input_and_line(self.terminal.backend_mut());
        let _ = self.terminal.show_cursor();
    }
}

fn restore_terminal_input_and_line<W: Write>(writer: &mut W) -> std::io::Result<()> {
    execute!(writer, DisableBracketedPaste, MoveToNextLine(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_error_names_bare_alan_contract() {
        assert!(terminal_capability_error().contains("bare `alan`"));
        assert!(!terminal_capability_error().contains("alan-tui"));
    }

    #[test]
    fn terminal_restoration_ends_the_inline_prompt_line() {
        let mut output = Vec::new();

        restore_terminal_input_and_line(&mut output).unwrap();

        assert_eq!(output, b"\x1b[?2004l\x1b[1E");
    }
}
