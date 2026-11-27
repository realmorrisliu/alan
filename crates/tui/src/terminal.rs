use anyhow::{Context, Result};
use crossterm::cursor::{MoveTo, MoveToNextLine};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, terminal as crossterm_terminal};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};
use std::io::{IsTerminal, Stdout, Write, stdout};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::hyperlink::HyperlinkBackend;
use crate::transcript_ui::wrapped_line_count;

#[cfg(test)]
#[path = "terminal_accessibility_tests.rs"]
mod accessibility_tests;
#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;

pub type AlanTerminal = Terminal<HyperlinkBackend<Stdout>>;

/// Output capability policy owned by the native terminal adapter.
#[derive(Clone, Copy)]
pub(crate) struct TerminalStylePolicy {
    colors: u16,
    attributes: bool,
}

impl TerminalStylePolicy {
    #[cfg(test)]
    pub(crate) fn from_capabilities(no_color: bool, term: &str, colors: u16) -> Self {
        Self::from_env_capabilities(no_color, term, None, colors)
    }

    pub(crate) fn from_env_capabilities(
        no_color: bool,
        term: &str,
        colorterm: Option<&str>,
        colors: u16,
    ) -> Self {
        let no_capability = no_color || matches!(term, "dumb" | "vt100" | "vt102" | "vt220");
        let effective_colors = if no_capability {
            0
        } else if crate::theme::colorterm_is_truecolor(colorterm) && colors > 0 {
            u16::MAX
        } else {
            colors
        };
        Self {
            colors: effective_colors,
            attributes: !no_color && term != "dumb",
        }
    }

    fn installed() -> Self {
        let colorterm = std::env::var("COLORTERM").ok();
        Self::from_env_capabilities(
            std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
            &std::env::var("TERM").unwrap_or_default(),
            colorterm.as_deref(),
            crossterm::style::available_color_count(),
        )
    }

    pub(crate) fn apply(self, buffer: &mut ratatui::buffer::Buffer) {
        use ratatui::style::Modifier;
        for cell in &mut buffer.content {
            cell.fg = downgrade_color(cell.fg, self.colors);
            cell.bg = downgrade_color(cell.bg, self.colors);
            cell.underline_color = downgrade_color(cell.underline_color, self.colors);
            if !self.attributes {
                cell.modifier = Modifier::empty();
            } else if self.colors < 16 {
                // Conservative native baseline: do not assume italic, dim or blink.
                cell.modifier &= Modifier::BOLD | Modifier::UNDERLINED | Modifier::REVERSED;
            }
        }
    }
}

fn downgrade_color(color: ratatui::style::Color, colors: u16) -> ratatui::style::Color {
    use ratatui::style::Color;
    match color {
        Color::Reset => Color::Reset,
        _ if colors == 0 => Color::Reset,
        Color::Rgb(r, g, b) => {
            if colors == u16::MAX {
                Color::Rgb(r, g, b)
            } else if colors >= 256 {
                rgb_to_xterm256(r, g, b)
            } else {
                rgb_to_ansi(r, g, b, colors >= 16)
            }
        }
        Color::Indexed(index) => {
            if u16::from(index) < colors {
                Color::Indexed(index)
            } else {
                Color::Reset
            }
        }
        Color::DarkGray
        | Color::LightRed
        | Color::LightGreen
        | Color::LightYellow
        | Color::LightBlue
        | Color::LightMagenta
        | Color::LightCyan
        | Color::White => {
            if colors >= 16 {
                color
            } else {
                Color::Reset
            }
        }
        _ => {
            if colors >= 8 {
                color
            } else {
                Color::Reset
            }
        }
    }
}

fn rgb_to_xterm256(r: u8, g: u8, b: u8) -> ratatui::style::Color {
    use ratatui::style::Color;
    match (r, g, b) {
        (82, 130, 255) => return Color::Indexed(69),
        (0, 47, 167) => return Color::Indexed(26),
        (34, 211, 166) => return Color::Indexed(43),
        (0, 138, 102) => return Color::Indexed(35),
        (245, 158, 11) => return Color::Indexed(214),
        (194, 106, 6) => return Color::Indexed(172),
        (248, 113, 113) => return Color::Indexed(203),
        (217, 56, 46) => return Color::Indexed(167),
        _ => {}
    }
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max.saturating_sub(min) <= 14 {
        if max < 8 {
            return Color::Indexed(16);
        }
        if min > 243 {
            return Color::Indexed(231);
        }
        let avg = ((u16::from(r) + u16::from(g) + u16::from(b)) / 3) as u8;
        let gray_idx = ((u16::from(avg.saturating_sub(8)) * 23) / 230).min(23) as u8;
        return Color::Indexed(232 + gray_idx);
    }
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let nearest = |channel: u8| -> u8 {
        LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, level)| channel.abs_diff(**level))
            .map_or(0, |(idx, _)| idx as u8)
    };
    Color::Indexed(16 + 36 * nearest(r) + 6 * nearest(g) + nearest(b))
}

fn rgb_to_ansi(r: u8, g: u8, b: u8, has_16: bool) -> ratatui::style::Color {
    use ratatui::style::Color;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max <= 55 || min >= 230 {
        return Color::Reset;
    }
    if max.saturating_sub(min) <= 36 {
        return if has_16 && max <= 150 {
            Color::DarkGray
        } else {
            Color::Reset
        };
    }
    if b > r.saturating_add(40) && b >= g {
        if has_16 && r >= 60 {
            Color::LightBlue
        } else {
            Color::Blue
        }
    } else if g > r.saturating_add(25) && g >= b {
        Color::Green
    } else if g > r.saturating_add(30) && b > r.saturating_add(30) {
        Color::Cyan
    } else if r > g.saturating_add(40) && r > b.saturating_add(40) && g.saturating_sub(b) <= 35 {
        Color::Red
    } else if r > b.saturating_add(40) && g > b.saturating_add(40) && g >= 90 && r >= g {
        Color::Yellow
    } else if r > g.saturating_add(40) && r > b.saturating_add(40) {
        Color::Red
    } else if r > g.saturating_add(30) && b > g.saturating_add(30) {
        Color::Magenta
    } else {
        Color::Reset
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
    // insert_before emits every cell; covered wide-character cells must emit no bytes.
    let width = buffer.area.width as usize;
    if width > 0 {
        for row in buffer.content.chunks_mut(width) {
            let mut covered = 0;
            for cell in row {
                if covered > 0 {
                    cell.set_symbol("");
                    covered -= 1;
                } else {
                    covered =
                        unicode_width::UnicodeWidthStr::width(cell.symbol()).saturating_sub(1);
                }
            }
        }
    }
}

pub fn is_interactive_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub fn terminal_capability_error() -> &'static str {
    "bare `alan` requires an interactive terminal; use explicit management subcommands for noninteractive automation"
}

static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);
static KITTY_KEYBOARD_ACTIVE: AtomicBool = AtomicBool::new(false);
static OSC_TITLE_ACTIVE: AtomicBool = AtomicBool::new(false);
static MOUSE_CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);

fn write_mouse_capture_mode<W: Write>(writer: &mut W, enabled: bool) -> std::io::Result<()> {
    if enabled {
        execute!(writer, EnableMouseCapture)
    } else {
        execute!(writer, DisableMouseCapture)
    }
}

fn should_enable_kitty_keyboard(term: &str, is_tty: bool) -> bool {
    is_tty && term != "dumb"
}

/// Clear the visible screen and move the cursor to `(0, 0)` on interactive VT terminals without
/// purging terminal scrollback history.
pub(crate) fn clear_startup_screen<W: Write>(
    writer: &mut W,
    term: &str,
    interactive: bool,
) -> std::io::Result<()> {
    if interactive && !term.eq_ignore_ascii_case("dumb") {
        execute!(writer, Clear(ClearType::All), MoveTo(0, 0))?;
    }
    Ok(())
}

fn push_kitty_keyboard_flags<W: Write>(writer: &mut W) -> std::io::Result<()> {
    execute!(
        writer,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
}

fn pop_kitty_keyboard_flags<W: Write>(writer: &mut W) -> std::io::Result<()> {
    execute!(writer, PopKeyboardEnhancementFlags)
}

/// Sanitize a terminal window title fragment so control sequences cannot escape OSC framing.
pub(crate) fn sanitize_osc_title(title: &str) -> String {
    let mut cleaned = String::with_capacity(title.len().min(128));
    let mut last_space = false;
    for ch in title.chars() {
        if cleaned.chars().count() >= 128 {
            break;
        }
        if ch.is_control() {
            if !last_space && !cleaned.is_empty() {
                cleaned.push(' ');
                last_space = true;
            }
            continue;
        }
        last_space = ch == ' ';
        cleaned.push(ch);
    }
    cleaned.trim().to_string()
}

/// Format an `alan · <project> · <state>` window title string.
pub(crate) fn format_osc_title(project: Option<&str>, state: &str) -> String {
    let clean_state = sanitize_osc_title(state);
    match project.map(sanitize_osc_title).filter(|s| !s.is_empty()) {
        Some(project) if !clean_state.is_empty() => format!("alan · {project} · {clean_state}"),
        Some(project) => format!("alan · {project}"),
        None if !clean_state.is_empty() => format!("alan · {clean_state}"),
        None => "alan".to_string(),
    }
}

fn write_osc_title_to<W: Write>(writer: &mut W, title: &str) -> std::io::Result<()> {
    let clean = sanitize_osc_title(title);
    write!(writer, "\x1b]0;{clean}\x07")?;
    writer.flush()
}

/// Update the standard OSC 0 window/icon title when attached to an active non-dumb TTY.
pub(crate) fn set_terminal_title(title: &str) {
    let term = std::env::var("TERM").unwrap_or_default();
    if !TERMINAL_ACTIVE.load(Ordering::SeqCst) || !is_interactive_terminal() || term == "dumb" {
        return;
    }
    let mut out = stdout();
    if write_osc_title_to(&mut out, title).is_ok() {
        OSC_TITLE_ACTIVE.store(true, Ordering::SeqCst);
    }
}

/// Ring the terminal BEL (`\x07`) when attached to an active non-dumb TTY unless `ALAN_NO_BELL` is set.
pub(crate) fn ring_terminal_bell() {
    let term = std::env::var("TERM").unwrap_or_default();
    if !TERMINAL_ACTIVE.load(Ordering::SeqCst)
        || !is_interactive_terminal()
        || term == "dumb"
        || std::env::var_os("ALAN_NO_BELL").is_some_and(|value| !value.is_empty())
    {
        return;
    }
    let mut out = stdout();
    let _ = out.write_all(b"\x07");
    let _ = out.flush();
}

/// Restore raw mode, bracketed paste, Kitty keyboard enhancement, mouse capture, title, and cursor visibility.
pub fn restore_terminal_modes() {
    let _ = crossterm_terminal::disable_raw_mode();
    let mut out = stdout();
    if MOUSE_CAPTURE_ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = write_mouse_capture_mode(&mut out, false);
    }
    if KITTY_KEYBOARD_ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = pop_kitty_keyboard_flags(&mut out);
    }
    if OSC_TITLE_ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = write_osc_title_to(&mut out, "");
    }
    let _ = execute!(out, DisableBracketedPaste, crossterm::cursor::Show);
    let _ = out.flush();
}

/// Install a process-wide panic hook (once) that restores terminal modes before printing the panic.
pub fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if TERMINAL_ACTIVE.swap(false, Ordering::SeqCst)
                || crossterm_terminal::is_raw_mode_enabled().unwrap_or(false)
            {
                restore_terminal_modes();
            }
            previous(info);
        }));
    });
}

/// RAII guard that disables `ECHO` on stdin when stdin is a TTY and restores termios on drop.
pub struct StdinEchoGuard {
    fd: libc::c_int,
    original: libc::termios,
}

impl StdinEchoGuard {
    /// Disable terminal `ECHO` if stdin is a TTY; returns `Ok(None)` when stdin is piped/non-TTY.
    pub fn disable_if_tty() -> Result<Option<Self>> {
        let fd = libc::STDIN_FILENO;
        // SAFETY: `isatty` is safe to invoke on any file descriptor.
        if unsafe { libc::isatty(fd) } != 1 {
            return Ok(None);
        }
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `original` is valid aligned storage for `termios`, checked against return code.
        if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error()).context("read terminal attributes");
        }
        // SAFETY: `tcgetattr` succeeded and initialized `original`.
        let original = unsafe { original.assume_init() };
        let mut no_echo = original;
        no_echo.c_lflag &= !libc::ECHO;
        // SAFETY: `no_echo` is a valid `termios` struct derived from `tcgetattr`.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &no_echo) } != 0 {
            return Err(std::io::Error::last_os_error()).context("disable terminal echo");
        }
        Ok(Some(Self { fd, original }))
    }
}

impl Drop for StdinEchoGuard {
    fn drop(&mut self) {
        // SAFETY: `original` was obtained from `tcgetattr` on `self.fd`.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
        }
    }
}

pub(crate) trait InlineSessionBackend:
    ratatui::backend::Backend<Error = std::io::Error> + Write
{
    fn link_sink(&self) -> std::sync::Arc<std::sync::Mutex<crate::hyperlink::CellLinkMap>>;
    fn image_sink(&self) -> std::sync::Arc<std::sync::Mutex<Option<String>>>;
    fn last_cursor_pos(&self) -> Option<ratatui::layout::Position>;
    fn rebuild_inline(&self, height: u16) -> Result<Terminal<Self>>
    where
        Self: Sized;
    fn is_live_tty(&self) -> bool {
        false
    }
}

impl InlineSessionBackend for HyperlinkBackend<Stdout> {
    fn link_sink(&self) -> std::sync::Arc<std::sync::Mutex<crate::hyperlink::CellLinkMap>> {
        HyperlinkBackend::link_sink(self)
    }

    fn image_sink(&self) -> std::sync::Arc<std::sync::Mutex<Option<String>>> {
        HyperlinkBackend::image_sink(self)
    }

    fn last_cursor_pos(&self) -> Option<ratatui::layout::Position> {
        HyperlinkBackend::last_cursor_pos(self)
    }

    fn rebuild_inline(&self, height: u16) -> Result<Terminal<Self>> {
        build_inline_terminal(stdout(), height)
    }

    fn is_live_tty(&self) -> bool {
        true
    }
}

pub(crate) struct InlineSession<B: InlineSessionBackend> {
    pub(crate) terminal: Terminal<B>,
    pub(crate) viewport_height: u16,
    last_screen_size: ratatui::layout::Size,
    last_cursor_offset: u16,
    hyperlink_host_root: Option<std::path::PathBuf>,
    hyperlinks_enabled: bool,
    pub(crate) graphics_capability: crate::graphics::GraphicsCapability,
    pub(crate) theme_mode: crate::theme::ThemeMode,
}

pub struct TerminalSession {
    pub(crate) inner: InlineSession<HyperlinkBackend<Stdout>>,
    mouse_capture_enabled: bool,
}

impl TerminalSession {
    pub fn enter() -> Result<Self> {
        install_panic_hook();
        TERMINAL_ACTIVE.store(true, Ordering::SeqCst);
        let startup_guard = TerminalStartupGuard::new();
        crossterm_terminal::enable_raw_mode().context("failed to enable raw terminal mode")?;
        let theme_mode = crate::theme::ThemeMode::from_env();
        let graphics_capability = crate::graphics::probe_terminal_graphics(theme_mode);
        let mut out = stdout();
        execute!(out, EnableBracketedPaste).context("failed to enable terminal input modes")?;
        let term = std::env::var("TERM").unwrap_or_default();
        if should_enable_kitty_keyboard(&term, is_interactive_terminal())
            && push_kitty_keyboard_flags(&mut out).is_ok()
        {
            KITTY_KEYBOARD_ACTIVE.store(true, Ordering::SeqCst);
        }
        clear_startup_screen(&mut out, &term, is_interactive_terminal())
            .context("failed to clear startup screen")?;
        let mut terminal = build_inline_terminal(out, 1)?;
        terminal.clear().context("failed to clear terminal")?;
        let last_screen_size = terminal
            .size()
            .context("failed to read initial terminal size")?;
        startup_guard.disarm();
        Ok(Self {
            inner: InlineSession {
                terminal,
                viewport_height: 1,
                last_screen_size,
                last_cursor_offset: 0,
                hyperlink_host_root: None,
                hyperlinks_enabled: !term.eq_ignore_ascii_case("dumb"),
                graphics_capability,
                theme_mode,
            },
            mouse_capture_enabled: false,
        })
    }

    /// Detected terminal inline graphics capability probed at session startup.
    pub fn graphics_capability(&self) -> crate::graphics::GraphicsCapability {
        self.inner.graphics_capability
    }

    /// Enable or disable SGR mouse capture strictly for open modals/pickers.
    pub fn set_mouse_capture(&mut self, enabled: bool) -> Result<()> {
        if self.mouse_capture_enabled == enabled {
            return Ok(());
        }
        write_mouse_capture_mode(self.inner.terminal.backend_mut(), enabled)
            .context("failed to toggle terminal mouse capture")?;
        self.mouse_capture_enabled = enabled;
        MOUSE_CAPTURE_ACTIVE.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    /// Configure the optional Host project root used to resolve relative `file://` OSC 8 links.
    pub fn set_hyperlink_context(&mut self, enabled: bool, host_root: Option<std::path::PathBuf>) {
        self.inner.hyperlinks_enabled = enabled;
        self.inner.hyperlink_host_root = host_root;
    }

    /// Publish permanent scrollback lines and the rebuilt inline viewport as one host update.
    pub fn draw_inline_frame(
        &mut self,
        committed: &[Line<'static>],
        height: u16,
        base_height: u16,
        draw: impl FnOnce(&mut Frame<'_>),
    ) -> Result<()> {
        self.inner
            .draw_inline_frame(committed, height, base_height, draw)
    }

    /// Publish typed permanent scrollback items and the rebuilt inline viewport as one update.
    pub fn draw_inline_frame_items(
        &mut self,
        committed: &[crate::graphics_scrollback::ScrollbackItem],
        height: u16,
        base_height: u16,
        draw: impl FnOnce(&mut Frame<'_>),
    ) -> Result<()> {
        self.inner
            .draw_inline_frame_items(committed, height, base_height, draw)
    }

    pub fn draw_with<F>(&mut self, draw: F) -> Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.inner.draw_with(draw)
    }

    pub fn viewport_size(&self) -> (usize, usize) {
        self.inner.viewport_size()
    }

    pub fn set_inline_height(&mut self, height: u16) -> Result<()> {
        self.inner.set_inline_height(height)
    }

    pub fn write_scrollback(&mut self, lines: &[Line<'static>]) -> Result<()> {
        self.inner.write_scrollback(lines)
    }
}

impl<B: InlineSessionBackend> InlineSession<B> {
    #[cfg(test)]
    pub(crate) fn from_inline_terminal(
        terminal: Terminal<B>,
        viewport_height: u16,
    ) -> Result<Self> {
        let last_screen_size = terminal
            .size()
            .context("failed to read initial terminal size")?;
        Ok(Self {
            terminal,
            viewport_height,
            last_screen_size,
            last_cursor_offset: 0,
            hyperlink_host_root: None,
            hyperlinks_enabled: false,
            graphics_capability: crate::graphics::GraphicsCapability::None,
            theme_mode: crate::theme::ThemeMode::Auto,
        })
    }

    /// Publish permanent scrollback lines and the rebuilt inline viewport as one host update.
    pub(crate) fn draw_inline_frame(
        &mut self,
        committed: &[Line<'static>],
        height: u16,
        base_height: u16,
        draw: impl FnOnce(&mut Frame<'_>),
    ) -> Result<()> {
        let items = (!committed.is_empty()).then(|| {
            [crate::graphics_scrollback::ScrollbackItem::Lines(
                committed.to_vec(),
            )]
        });
        let slice = items.as_ref().map_or(&[][..], |single| single.as_slice());
        self.draw_inline_frame_items(slice, height, base_height, draw)
    }

    /// Publish typed permanent scrollback items and the rebuilt inline viewport as one update.
    pub(crate) fn draw_inline_frame_items(
        &mut self,
        committed: &[crate::graphics_scrollback::ScrollbackItem],
        height: u16,
        base_height: u16,
        draw: impl FnOnce(&mut Frame<'_>),
    ) -> Result<()> {
        synchronized_frame(
            self,
            |session, begin| synchronized_boundary(session.terminal.backend_mut(), begin),
            |session| {
                session.sync_terminal_resize()?;
                crate::graphics_scrollback::write_scrollback_items(session, committed)?;
                let screen = session.terminal.size()?.height;
                let top = session.terminal.get_frame().area().y;
                let height = anchored_inline_height(base_height, height, screen, top);
                session.set_inline_height(height)?;
                session.draw_with(draw)
            },
        )
    }

    pub(crate) fn draw_with<F>(&mut self, draw: F) -> Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        let policy = TerminalStylePolicy::installed();
        let hyperlinks = self.hyperlinks_enabled;
        let host_root = self.hyperlink_host_root.clone();
        let link_sink = self.terminal.backend().link_sink();
        if let Ok(mut guard) = link_sink.lock() {
            *guard = crate::hyperlink::CellLinkMap::default();
        }
        let completed = self
            .terminal
            .draw(|frame| {
                render_frame_with_policy(frame, policy, draw);
                if hyperlinks {
                    let area_height = frame.area().height;
                    let links = crate::hyperlink::collect_buffer_links(
                        frame.buffer_mut(),
                        area_height,
                        host_root.as_deref(),
                    );
                    if let Ok(mut guard) = link_sink.lock() {
                        *guard = links;
                    }
                }
            })
            .context("failed to draw terminal frame")?;
        let top = completed.buffer.area.y;
        if let Some(pos) = self.terminal.backend().last_cursor_pos() {
            self.last_cursor_offset = pos.y.saturating_sub(top);
        }
        Ok(())
    }

    pub(crate) fn viewport_size(&self) -> (usize, usize) {
        self.terminal
            .size()
            .map(|area| (area.width as usize, area.height as usize))
            .unwrap_or((80, 24))
    }

    /// Synchronize Ratatui's viewport area with any terminal size change before writing
    /// scrollback or computing `top`, preserving committed rows above the live viewport.
    pub(crate) fn sync_terminal_resize(&mut self) -> Result<()> {
        let current_size = self
            .terminal
            .size()
            .context("failed to read terminal size")?;
        if current_size == self.last_screen_size {
            return Ok(());
        }

        let horizontal_shrink = current_size.width < self.last_screen_size.width;
        let height_below_viewport = current_size.height < self.viewport_height;

        if horizontal_shrink || height_below_viewport {
            let cur_pos = self
                .terminal
                .get_cursor_position()
                .context("failed to read cursor position during resize")?;
            let on_screen_top = cur_pos.y.saturating_sub(self.last_cursor_offset);
            let target_height = self.viewport_height.min(current_size.height.max(1));
            // On horizontal shrink, push all committed rows above the viewport into scrollback
            // and re-anchor at row 0 instead of letting Ratatui's `Clear(All)` erase them.
            let transition_height = if horizontal_shrink {
                current_size.height.max(1)
            } else {
                target_height
            };
            prepare_inline_height_transition(
                self.terminal.backend_mut(),
                on_screen_top,
                current_size.height,
                transition_height,
            )
            .context("failed to preserve scrollback across terminal resize")?;
            let terminal = self.terminal.backend().rebuild_inline(target_height)?;
            self.terminal = terminal;
            self.viewport_height = target_height;
            self.last_cursor_offset = 0;
        } else {
            self.terminal
                .autoresize()
                .context("failed to resize terminal")?;
        }
        self.last_screen_size = current_size;
        Ok(())
    }

    pub fn set_inline_height(&mut self, height: u16) -> Result<()> {
        self.sync_terminal_resize()?;
        let terminal_size = self
            .terminal
            .size()
            .context("failed to read terminal size")?;
        let height = height.max(1).min(terminal_size.height.max(1));
        if height == self.viewport_height {
            return Ok(());
        }

        let top = self.terminal.get_frame().area().y;
        prepare_inline_height_transition(
            self.terminal.backend_mut(),
            top,
            terminal_size.height,
            height,
        )
        .context("failed to clear the previous inline viewport")?;

        let terminal = self.terminal.backend().rebuild_inline(height)?;
        self.terminal = terminal;
        self.viewport_height = height;
        self.last_screen_size = terminal_size;
        self.last_cursor_offset = 0;
        Ok(())
    }

    pub fn write_scrollback(&mut self, lines: &[Line<'static>]) -> Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        self.sync_terminal_resize()?;
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
        let hyperlinks = self.hyperlinks_enabled;
        let host_root = self.hyperlink_host_root.clone();
        let link_sink = self.terminal.backend().link_sink();
        if let Ok(mut guard) = link_sink.lock() {
            *guard = crate::hyperlink::CellLinkMap::default();
        }
        self.terminal
            .insert_before(height, |buf| {
                render_scrollback_with_policy(buf, lines, policy);
                if hyperlinks {
                    let links = crate::hyperlink::collect_buffer_links(
                        buf,
                        buf.area.height,
                        host_root.as_deref(),
                    );
                    if let Ok(mut guard) = link_sink.lock() {
                        *guard = links;
                    }
                }
            })
            .context("failed to append transcript to terminal scrollback")
    }
}

/// Compute the top row to clear before rebuilding an inline viewport of `new_height`.
/// When an inline viewport expands upward, `new_height` claims rows above the old `top`;
/// clearing from `min(top, screen_height - new_height)` wipes prior scrollback/banner cells.
pub(crate) fn inline_clear_y(top: u16, screen_height: u16, new_height: u16) -> u16 {
    top.min(screen_height.saturating_sub(new_height))
}

/// Prepare the terminal for rebuilding an inline viewport of `new_height` rows starting from `top`.
pub(crate) fn prepare_inline_height_transition<W: Write>(
    writer: &mut W,
    top: u16,
    screen_height: u16,
    new_height: u16,
) -> std::io::Result<()> {
    let clear_y = inline_clear_y(top, screen_height, new_height);
    let deficit = top.min(screen_height).saturating_sub(clear_y);
    if deficit > 0 {
        // Upward expansion (including full-screen `new_height == screen_height`): clear the old
        // live viewport first, then scroll committed rows above `top` into terminal scrollback.
        execute!(
            writer,
            MoveTo(0, top),
            Clear(ClearType::FromCursorDown),
            MoveTo(0, screen_height.saturating_sub(1))
        )?;
        writer.write_all("\n".repeat(deficit as usize).as_bytes())?;
    }
    execute!(
        writer,
        MoveTo(0, clear_y),
        Clear(ClearType::FromCursorDown),
        MoveTo(0, clear_y)
    )
}

/// Restore terminal modes, write `\r\n`, and suspend the current foreground process via `SIGTSTP`.
#[cfg(unix)]
pub(crate) fn suspend_current_process() -> Result<()> {
    restore_terminal_modes();
    let mut out = stdout();
    let _ = out.write_all(b"\r\n");
    let _ = out.flush();
    // SAFETY: `libc::raise(libc::SIGTSTP)` sends standard POSIX job-control stop to this process.
    if unsafe { libc::raise(libc::SIGTSTP) } != 0 {
        return Err(std::io::Error::last_os_error()).context("raise SIGTSTP");
    }
    Ok(())
}

/// Non-Unix no-op fallback for job suspend.
#[cfg(not(unix))]
pub(crate) fn suspend_current_process() -> Result<()> {
    Ok(())
}

pub(crate) fn anchored_inline_height(base: u16, desired: u16, screen: u16, top: u16) -> u16 {
    // Only stable content may require moving the Inline origin upward.
    // Transient hints consume available rows BELOW the current owner origin.
    base.max(desired.min(screen.saturating_sub(top)))
        .min(screen)
        .max(1)
}

fn build_inline_terminal(out: Stdout, height: u16) -> Result<AlanTerminal> {
    let backend = HyperlinkBackend::new(out);
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
        TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
        let mut out = stdout();
        if KITTY_KEYBOARD_ACTIVE.swap(false, Ordering::SeqCst) {
            let _ = pop_kitty_keyboard_flags(&mut out);
        }
        if OSC_TITLE_ACTIVE.swap(false, Ordering::SeqCst) {
            let _ = write_osc_title_to(&mut out, "");
        }
        let _ = execute!(out, DisableBracketedPaste);
        let _ = crossterm_terminal::disable_raw_mode();
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if !self.inner.terminal.backend().is_live_tty() {
            return;
        }
        TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
        let _ = crossterm_terminal::disable_raw_mode();
        if self.mouse_capture_enabled || MOUSE_CAPTURE_ACTIVE.swap(false, Ordering::SeqCst) {
            self.mouse_capture_enabled = false;
            MOUSE_CAPTURE_ACTIVE.store(false, Ordering::SeqCst);
            let _ = write_mouse_capture_mode(self.inner.terminal.backend_mut(), false);
        }
        if KITTY_KEYBOARD_ACTIVE.swap(false, Ordering::SeqCst) {
            let _ = pop_kitty_keyboard_flags(self.inner.terminal.backend_mut());
        }
        if OSC_TITLE_ACTIVE.swap(false, Ordering::SeqCst) {
            let _ = write_osc_title_to(self.inner.terminal.backend_mut(), "");
        }
        let _ = restore_terminal_input_and_line(self.inner.terminal.backend_mut());
        let _ = self.inner.terminal.show_cursor();
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
