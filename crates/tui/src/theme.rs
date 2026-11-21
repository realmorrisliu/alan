//! Semantic terminal theme tokens aligned with `alanworks.app` design tokens.
//!
//! Normal prose and body backgrounds always remain [`Color::Reset`] so the user's
//! terminal background stays transparent.

use ratatui::style::{Color, Modifier, Style};

/// User-selectable display theme mode configured via `ALAN_THEME`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Auto,
    Midnight,
    Paper,
    Ansi,
    Dumb,
}

impl ThemeMode {
    /// Parse `ALAN_THEME=auto|midnight|paper|ansi|dumb` (case-insensitive).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "midnight" | "dark" => Some(Self::Midnight),
            "paper" | "light" => Some(Self::Paper),
            "ansi" | "16" | "basic" => Some(Self::Ansi),
            "dumb" | "mono" | "monochrome" => Some(Self::Dumb),
            _ => None,
        }
    }

    /// Read [`ThemeMode`] from `ALAN_THEME`, defaulting to [`ThemeMode::Auto`].
    pub fn from_env() -> Self {
        std::env::var("ALAN_THEME")
            .ok()
            .and_then(|value| Self::parse(&value))
            .unwrap_or_default()
    }
}

/// Semantic state for status dot indicators (`●` or monochrome ASCII fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotState {
    Run,
    Wait,
    Block,
    Muted,
}

/// Centralized semantic palette and glyph policy for Alan's TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub is_dumb: bool,
    pub accent: Color,
    pub status_run: Color,
    pub status_wait: Color,
    pub status_block: Color,
    pub muted: Color,
    pub faint: Color,
    pub rule: Color,
    pub selection_bg: Color,
    pub syntax_keyword: Color,
    pub syntax_type: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_comment: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::midnight()
    }
}

impl Theme {
    /// Dark TrueColor palette (`Midnight`) matching `website/src/styles/tokens.css`.
    pub const fn midnight() -> Self {
        Self {
            mode: ThemeMode::Midnight,
            is_dumb: false,
            accent: Color::Rgb(82, 130, 255),
            status_run: Color::Rgb(34, 211, 166),
            status_wait: Color::Rgb(245, 158, 11),
            status_block: Color::Rgb(248, 113, 113),
            muted: Color::Rgb(152, 166, 199),
            faint: Color::Rgb(103, 119, 158),
            rule: Color::Rgb(68, 86, 134),
            selection_bg: Color::Rgb(19, 36, 84),
            syntax_keyword: Color::Rgb(167, 139, 250),
            syntax_type: Color::Rgb(120, 156, 235),
            syntax_string: Color::Rgb(34, 211, 166),
            syntax_number: Color::Rgb(245, 158, 11),
            syntax_comment: Color::Rgb(103, 119, 158),
        }
    }

    /// Light TrueColor palette (`Paper`) matching `website/src/styles/tokens.css`.
    pub const fn paper() -> Self {
        Self {
            mode: ThemeMode::Paper,
            is_dumb: false,
            accent: Color::Rgb(0, 47, 167),
            status_run: Color::Rgb(0, 138, 102),
            status_wait: Color::Rgb(194, 106, 6),
            status_block: Color::Rgb(217, 56, 46),
            muted: Color::Rgb(80, 87, 102),
            faint: Color::Rgb(123, 130, 145),
            rule: Color::Rgb(172, 167, 154),
            selection_bg: Color::Rgb(227, 234, 250),
            syntax_keyword: Color::Rgb(109, 40, 217),
            syntax_type: Color::Rgb(0, 47, 167),
            syntax_string: Color::Rgb(0, 138, 102),
            syntax_number: Color::Rgb(194, 106, 6),
            syntax_comment: Color::Rgb(123, 130, 145),
        }
    }

    /// 16/8-color ANSI fallback palette.
    pub const fn ansi() -> Self {
        Self {
            mode: ThemeMode::Ansi,
            is_dumb: false,
            accent: Color::Cyan,
            status_run: Color::Green,
            status_wait: Color::Yellow,
            status_block: Color::Red,
            muted: Color::DarkGray,
            faint: Color::DarkGray,
            rule: Color::DarkGray,
            selection_bg: Color::Reset,
            syntax_keyword: Color::Magenta,
            syntax_type: Color::Cyan,
            syntax_string: Color::Green,
            syntax_number: Color::Yellow,
            syntax_comment: Color::DarkGray,
        }
    }

    /// Monochrome `TERM=dumb` / `NO_COLOR` palette using `Color::Reset` everywhere.
    pub const fn dumb() -> Self {
        Self {
            mode: ThemeMode::Dumb,
            is_dumb: true,
            accent: Color::Reset,
            status_run: Color::Reset,
            status_wait: Color::Reset,
            status_block: Color::Reset,
            muted: Color::Reset,
            faint: Color::Reset,
            rule: Color::Reset,
            selection_bg: Color::Reset,
            syntax_keyword: Color::Reset,
            syntax_type: Color::Reset,
            syntax_string: Color::Reset,
            syntax_number: Color::Reset,
            syntax_comment: Color::Reset,
        }
    }

    /// Resolve a [`Theme`] from explicit terminal capabilities and environment overrides.
    pub fn resolve(
        mode: ThemeMode,
        no_color: bool,
        term: &str,
        colorterm: Option<&str>,
        colors: u16,
    ) -> Self {
        if no_color || term == "dumb" || mode == ThemeMode::Dumb {
            return Self::dumb();
        }
        if matches!(term, "vt100" | "vt102" | "vt220") || colors == 0 {
            return Self::dumb();
        }
        match mode {
            ThemeMode::Midnight => Self::midnight(),
            ThemeMode::Paper => Self::paper(),
            ThemeMode::Ansi => Self::ansi(),
            ThemeMode::Dumb => Self::dumb(),
            ThemeMode::Auto => {
                let truecolor = colorterm_is_truecolor(colorterm) || colors == u16::MAX;
                if truecolor || colors >= 256 {
                    Self::midnight()
                } else {
                    Self::ansi()
                }
            }
        }
    }

    /// Resolve the active [`Theme`] from `ALAN_THEME`, `NO_COLOR`, `TERM`, `COLORTERM`,
    /// and `crossterm::style::available_color_count()`.
    pub fn from_env() -> Self {
        let mode = ThemeMode::from_env();
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        let term = std::env::var("TERM").unwrap_or_default();
        let colorterm = std::env::var("COLORTERM").ok();
        let colors = crossterm::style::available_color_count();
        Self::resolve(mode, no_color, &term, colorterm.as_deref(), colors)
    }

    pub fn accent_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.accent)
        }
    }

    pub fn accent_bold(self) -> Style {
        self.bold_if_capable(self.accent_style())
    }

    pub fn muted_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.muted)
        }
    }

    pub fn faint_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.faint)
        }
    }

    pub fn rule_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.rule).add_modifier(Modifier::DIM)
        }
    }

    pub fn run_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.status_run)
        }
    }

    pub fn wait_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.status_wait)
        }
    }

    pub fn block_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.status_block)
        }
    }

    pub fn bold_if_capable(self, style: Style) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            style.add_modifier(Modifier::BOLD)
        }
    }

    pub fn italic_if_capable(self, style: Style) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            style.add_modifier(Modifier::ITALIC)
        }
    }

    pub fn underlined_if_capable(self, style: Style) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            style.add_modifier(Modifier::UNDERLINED)
        }
    }

    pub fn dim_if_capable(self, style: Style) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            style.add_modifier(Modifier::DIM)
        }
    }

    /// Open left vertical gutter prefix used for code blocks, diffs, plans, yields, and thinking.
    pub fn gutter(self) -> &'static str {
        if self.is_dumb { "| " } else { "▎ " }
    }

    /// Semantic status dot glyph (`●` or monochrome ASCII hierarchy under `TERM=dumb`) and style.
    pub fn status_dot(self, state: DotState) -> (&'static str, Style) {
        if self.is_dumb {
            let glyph = match state {
                DotState::Run => "*",
                DotState::Wait => "?",
                DotState::Block => "!",
                DotState::Muted => "·",
            };
            return (glyph, Style::default());
        }
        let color = match state {
            DotState::Run => self.status_run,
            DotState::Wait => self.status_wait,
            DotState::Block => self.status_block,
            DotState::Muted => self.muted,
        };
        ("●", Style::default().fg(color))
    }

    /// Styles for `(active_prefix, left_column, right_column)` in two-column completion menus.
    pub fn completion_row_styles(self, selected: bool) -> (Style, Style, Style) {
        if self.is_dumb {
            return (Style::default(), Style::default(), Style::default());
        }
        if selected {
            let mut prefix = Style::default()
                .fg(self.accent)
                .add_modifier(Modifier::BOLD);
            let mut left = Style::default()
                .fg(self.accent)
                .add_modifier(Modifier::BOLD);
            let mut right = Style::default();
            if self.selection_bg != Color::Reset {
                prefix = prefix.bg(self.selection_bg);
                left = left.bg(self.selection_bg);
                right = right.bg(self.selection_bg);
            }
            (prefix, left, right)
        } else {
            (
                Style::default(),
                Style::default().fg(self.accent),
                Style::default().fg(self.muted),
            )
        }
    }

    /// Distinct Markdown heading hierarchy (`h1`..`h6`).
    pub fn heading_style(self, level: usize) -> Style {
        if self.is_dumb {
            return Style::default();
        }
        match level {
            1 => Style::default()
                .fg(self.accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            2 => Style::default()
                .fg(self.accent)
                .add_modifier(Modifier::BOLD),
            3 => Style::default()
                .fg(self.status_wait)
                .add_modifier(Modifier::BOLD),
            4 => Style::default()
                .fg(self.status_run)
                .add_modifier(Modifier::BOLD),
            5 => Style::default()
                .fg(self.syntax_keyword)
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .fg(self.muted)
                .add_modifier(Modifier::BOLD | Modifier::DIM),
        }
    }

    pub fn diff_add_style(self) -> Style {
        self.run_style()
    }

    pub fn diff_remove_style(self) -> Style {
        self.block_style()
    }

    /// Semantic style for inline code spans (`` `code` ``): calm tint without underline.
    pub fn inline_code_style(self) -> Style {
        if self.is_dumb {
            Style::default()
        } else {
            Style::default().fg(self.syntax_type)
        }
    }

    /// Semantic style for user/command prompt sigils (`: ` / `! `) in transcript history.
    pub fn user_prompt_prefix_style(self, command: bool) -> Style {
        if self.is_dumb {
            Style::default()
        } else if command {
            self.bold_if_capable(self.wait_style())
        } else {
            self.accent_bold()
        }
    }

    /// Semantic style for user/command prompt body text in transcript history.
    pub fn user_prompt_body_style(self) -> Style {
        self.bold_if_capable(Style::default())
    }
}

pub(crate) fn colorterm_is_truecolor(colorterm: Option<&str>) -> bool {
    colorterm.is_some_and(|value| {
        let trimmed = value.trim();
        trimmed.eq_ignore_ascii_case("truecolor") || trimmed.eq_ignore_ascii_case("24bit")
    })
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
