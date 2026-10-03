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

use crate::transcript_ui::wrapped_line_count;

#[cfg(test)]
#[path = "terminal_accessibility_tests.rs"]
mod accessibility_tests;

pub type AlanTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Output capability policy owned by the native terminal adapter.
#[derive(Clone, Copy)]
pub(crate) struct TerminalStylePolicy {
    colors: u16,
    attributes: bool,
}

impl TerminalStylePolicy {
    pub(crate) fn from_capabilities(no_color: bool, term: &str, colors: u16) -> Self {
        Self {
            colors: if no_color || matches!(term, "dumb" | "vt100" | "vt102" | "vt220") {
                0
            } else {
                colors
            },
            attributes: term != "dumb",
        }
    }

    fn installed() -> Self {
        Self::from_capabilities(
            std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
            &std::env::var("TERM").unwrap_or_default(),
            crossterm::style::available_color_count(),
        )
    }

    pub(crate) fn apply(self, buffer: &mut ratatui::buffer::Buffer) {
        use ratatui::style::{Color, Modifier};
        let supported = |color| match color {
            Color::Reset => true,
            Color::Rgb(..) => self.colors == u16::MAX,
            Color::Indexed(index) => u16::from(index) < self.colors,
            Color::DarkGray
            | Color::LightRed
            | Color::LightGreen
            | Color::LightYellow
            | Color::LightBlue
            | Color::LightMagenta
            | Color::LightCyan
            | Color::White => self.colors >= 16,
            _ => self.colors >= 8,
        };
        for cell in &mut buffer.content {
            if !supported(cell.fg) {
                cell.fg = Color::Reset;
            }
            if !supported(cell.bg) {
                cell.bg = Color::Reset;
            }
            if !supported(cell.underline_color) {
                cell.underline_color = Color::Reset;
            }
            if !self.attributes {
                cell.modifier = Modifier::empty();
            } else if self.colors < 16 {
                // Conservative native baseline: do not assume italic, dim or blink.
                cell.modifier &= Modifier::BOLD | Modifier::UNDERLINED | Modifier::REVERSED;
            }
        }
    }
}

pub(crate) fn render_frame_with_policy(
    frame: &mut Frame<'_>,
    policy: TerminalStylePolicy,
    draw: impl FnOnce(&mut Frame<'_>),
) {
    draw(frame);
    policy.apply(frame.buffer_mut());
}

pub(crate) fn render_scrollback_with_policy(
    buffer: &mut ratatui::buffer::Buffer,
    lines: Vec<Line<'static>>,
    policy: TerminalStylePolicy,
) {
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(buffer.area, buffer);
    policy.apply(buffer);
}

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

    /// Publish permanent scrollback and the rebuilt inline viewport as one host update.
    pub fn draw_inline_frame<F>(
        &mut self,
        committed: &[Line<'static>],
        height: u16,
        base_height: u16,
        draw: F,
    ) -> Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        synchronized_frame(
            self,
            |session, begin| synchronized_boundary(session.terminal.backend_mut(), begin),
            |session| {
                session.write_scrollback(committed)?;
                let screen = session.terminal.size()?.height;
                let top = session.terminal.get_frame().area().y;
                let height = anchored_inline_height(base_height, height, screen, top);
                session.set_inline_height(height)?;
                session.draw_with(draw)
            },
        )
    }

    pub fn draw_with<F>(&mut self, draw: F) -> Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        let policy = TerminalStylePolicy::installed();
        self.terminal
            .draw(|frame| render_frame_with_policy(frame, policy, draw))
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

    pub fn write_scrollback(&mut self, lines: &[Line<'static>]) -> Result<()> {
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
            let styled = line.clone();
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
        let policy = TerminalStylePolicy::installed();
        self.terminal
            .insert_before(height, |buf| {
                render_scrollback_with_policy(buf, lines, policy);
            })
            .context("failed to append transcript to terminal scrollback")
    }
}

pub(crate) fn anchored_inline_height(base: u16, desired: u16, screen: u16, top: u16) -> u16 {
    // Only stable content may require moving the Inline origin upward.
    // Transient hints consume available rows BELOW the current owner origin.
    base.max(desired.min(screen.saturating_sub(top)))
        .min(screen)
        .max(1)
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

fn synchronized_boundary(writer: &mut impl Write, begin: bool) -> Result<()> {
    if begin {
        execute!(writer, crossterm_terminal::BeginSynchronizedUpdate)
            .context("failed to begin synchronized terminal update")
    } else {
        execute!(writer, crossterm_terminal::EndSynchronizedUpdate)
            .context("failed to end synchronized terminal update")
    }
}

fn synchronized_frame<T>(
    owner: &mut T,
    mut boundary: impl FnMut(&mut T, bool) -> Result<()>,
    frame: impl FnOnce(&mut T) -> Result<()>,
) -> Result<()> {
    let result = boundary(owner, true).and_then(|()| frame(owner));
    // Even a partial begin or failed frame must attempt to release the host.
    let cleanup = boundary(owner, false);
    match (result, cleanup) {
        (Ok(()), cleanup) => cleanup,
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "also failed to end synchronized terminal update: {cleanup:#}"
        ))),
    }
}

fn restore_terminal_input_and_line<W: Write>(writer: &mut W) -> std::io::Result<()> {
    execute!(writer, DisableBracketedPaste, MoveToNextLine(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_frame_transaction_orders_native_boundaries_and_closes_on_errors() {
        // Inject only IO at the existing frame owner, not renderer geometry.
        for failure in [
            None,
            Some("begin"),
            Some("scrollback"),
            Some("clear"),
            Some("draw"),
            Some("end"),
        ] {
            let mut output = Vec::new();
            let result = synchronized_frame(
                &mut output,
                |out, begin| {
                    synchronized_boundary(out, begin)?;
                    let stage = if begin { "begin" } else { "end" };
                    anyhow::ensure!(failure != Some(stage), "{stage} failed");
                    Ok(())
                },
                |out| {
                    for stage in ["scrollback", "clear", "draw"] {
                        out.extend_from_slice(stage.as_bytes());
                        anyhow::ensure!(failure != Some(stage), "{stage} failed");
                    }
                    Ok(())
                },
            );
            assert!(
                output.starts_with(b"\x1b[?2026h"),
                "{failure:?}: {output:?}"
            );
            assert!(output.ends_with(b"\x1b[?2026l"), "{failure:?}: {output:?}");
            match failure {
                None => {
                    assert!(result.is_ok());
                    assert_eq!(output, b"\x1b[?2026hscrollbackcleardraw\x1b[?2026l");
                }
                Some(stage) => assert!(format!("{:#}", result.unwrap_err()).contains(stage)),
            }
        }
        let error = synchronized_frame(
            &mut (),
            |_, begin| {
                if begin {
                    Ok(())
                } else {
                    anyhow::bail!("cleanup failed")
                }
            },
            |_| anyhow::bail!("original draw failed"),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("original draw failed"));
        assert!(format!("{error:#}").contains("cleanup failed"));
    }

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
