//! Live terminal stage indicator and streaming download progress bar for `alan update`.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const PROGRESS_BAR_WIDTH: usize = 20;
const MIN_PROGRESS_BAR_WIDTH: usize = 10;
const DEFAULT_TERMINAL_WIDTH: usize = 80;
const REDRAW_INTERVAL: Duration = Duration::from_millis(80);

pub(super) struct UpdateProgress {
    enabled: bool,
    is_tty: bool,
    ascii_bar: bool,
    started_at: Instant,
    last_draw: Option<Instant>,
    inline_active: bool,
}

impl UpdateProgress {
    pub(super) fn new(enabled: bool) -> Self {
        Self::with_tty(enabled, std::io::stderr().is_terminal())
    }

    pub(super) fn with_tty(enabled: bool, is_tty: bool) -> Self {
        Self {
            enabled,
            is_tty,
            ascii_bar: prefers_ascii_bar(),
            started_at: Instant::now(),
            last_draw: None,
            inline_active: false,
        }
    }

    pub(super) fn stage(&mut self, message: &str) {
        if !self.enabled {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        if self.inline_active {
            let _ = writeln!(stderr);
            self.inline_active = false;
        }
        let _ = writeln!(stderr, "{message}");
        let _ = stderr.flush();
    }

    pub(super) fn start_download(&mut self, asset: &str, total_bytes: Option<u64>) {
        self.started_at = Instant::now();
        self.last_draw = None;
        if !self.enabled {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        if self.is_tty {
            let line = self.render_tty_line(asset, 0, total_bytes, Duration::ZERO);
            let _ = write!(stderr, "\r\x1b[2K{line}");
            let _ = stderr.flush();
            self.inline_active = true;
            self.last_draw = Some(self.started_at);
        } else {
            let line = match total_bytes.filter(|&size| size > 0) {
                Some(total) => format!("Downloading {asset} ({})...", format_bytes(total)),
                None => format!("Downloading {asset}..."),
            };
            let _ = writeln!(stderr, "{line}");
            let _ = stderr.flush();
        }
    }

    pub(super) fn advance_download(
        &mut self,
        asset: &str,
        downloaded: u64,
        total_bytes: Option<u64>,
    ) {
        if !self.enabled || !self.is_tty {
            return;
        }
        let now = Instant::now();
        if self
            .last_draw
            .is_some_and(|prev| now.saturating_duration_since(prev) < REDRAW_INTERVAL)
        {
            return;
        }
        let line = self.render_tty_line(
            asset,
            downloaded,
            total_bytes,
            now.saturating_duration_since(self.started_at),
        );
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, "\r\x1b[2K{line}");
        let _ = stderr.flush();
        self.inline_active = true;
        self.last_draw = Some(now);
    }

    pub(super) fn finish_download(
        &mut self,
        asset: &str,
        downloaded: u64,
        total_bytes: Option<u64>,
    ) {
        if !self.enabled {
            return;
        }
        let elapsed = self.started_at.elapsed();
        let effective_total = total_bytes.filter(|&size| size > 0).or(Some(downloaded));
        if self.is_tty {
            let line = self.render_tty_line(asset, downloaded, effective_total, elapsed);
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "\r\x1b[2K{line}");
            let _ = stderr.flush();
            self.inline_active = false;
        }
    }

    fn render_tty_line(
        &self,
        asset: &str,
        downloaded: u64,
        total_bytes: Option<u64>,
        elapsed: Duration,
    ) -> String {
        // Reserve one trailing column to avoid terminal deferred-wrap at the right margin.
        let max_cols = stderr_terminal_width().saturating_sub(1).max(1);
        render_progress_line_for_width(
            asset,
            downloaded,
            total_bytes,
            elapsed,
            self.ascii_bar,
            Some(max_cols),
        )
    }

    pub(super) fn finish_on_error(&mut self) {
        if self.enabled && self.inline_active {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr);
            let _ = stderr.flush();
        }
        self.inline_active = false;
    }
}

fn prefers_ascii_bar() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
        || matches!(std::env::var("TERM").ok().as_deref(), Some("dumb"))
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn format_rate(bytes_per_sec: f64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes_per_sec >= MIB {
        format!("{:.1} MiB/s", bytes_per_sec / MIB)
    } else if bytes_per_sec >= KIB {
        format!("{:.1} KiB/s", bytes_per_sec / KIB)
    } else {
        format!("{:.0} B/s", bytes_per_sec.max(0.0))
    }
}

#[cfg(test)]
fn render_progress_line_with_style(
    asset: &str,
    downloaded: u64,
    total_bytes: Option<u64>,
    elapsed: Duration,
    ascii_bar: bool,
) -> String {
    render_progress_line_for_width(asset, downloaded, total_bytes, elapsed, ascii_bar, None)
}

pub(crate) fn render_progress_line_for_width(
    asset: &str,
    downloaded: u64,
    total_bytes: Option<u64>,
    elapsed: Duration,
    ascii_bar: bool,
    max_width: Option<usize>,
) -> String {
    let rate_str = if elapsed.as_secs_f64() > 0.0 && downloaded > 0 {
        format_rate(downloaded as f64 / elapsed.as_secs_f64())
    } else {
        "-- KiB/s".to_string()
    };
    let ellipsis = if ascii_bar { "..." } else { "…" };

    let line = if let Some(total) = total_bytes.filter(|&size| size > 0) {
        let clamped = downloaded.min(total);
        let ratio = (clamped as f64 / total as f64).clamp(0.0, 1.0);
        let pct = (ratio * 100.0).round() as u64;
        let dl = format_bytes(clamped);
        let tot = format_bytes(total);
        match max_width {
            None => format!(
                "Downloading {asset}  [{}] {pct:3}%  {dl} / {tot}  {rate_str}",
                build_bar(ratio, PROGRESS_BAR_WIDTH, ascii_bar)
            ),
            Some(limit) => fit_known_total_line(
                KnownTotalSpec {
                    asset,
                    ratio,
                    pct,
                    dl: &dl,
                    tot: &tot,
                    rate_str: &rate_str,
                    ascii_bar,
                    ellipsis,
                },
                limit,
            ),
        }
    } else {
        let dl = format_bytes(downloaded);
        match max_width {
            None => format!("Downloading {asset}  {dl}  {rate_str}"),
            Some(limit) => fit_unknown_total_line(asset, &dl, &rate_str, ellipsis, limit),
        }
    };

    debug_assert!(
        max_width.is_none_or(|limit| display_width(&line) <= limit),
        "rendered progress line `{line}` ({} cols) exceeded limit {max_width:?}",
        display_width(&line)
    );
    line
}

fn stderr_terminal_width() -> usize {
    #[cfg(unix)]
    {
        // SAFETY: `libc::winsize` is a plain C struct initialized by `ioctl(TIOCGWINSZ)`.
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        // SAFETY: `STDERR_FILENO` is the standard error fd and `&mut ws` is valid storage.
        let rc = unsafe { libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ as _, &mut ws) };
        if rc == 0 && ws.ws_col > 0 {
            return usize::from(ws.ws_col);
        }
    }
    if let Some(cols) = std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&cols| cols > 0)
    {
        return cols;
    }
    DEFAULT_TERMINAL_WIDTH
}

fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

fn build_bar(ratio: f64, bar_width: usize, ascii_bar: bool) -> String {
    let filled = (ratio * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled);
    let (fill_ch, empty_ch) = if ascii_bar {
        ('#', '-')
    } else {
        ('█', '░')
    };
    let mut bar = String::with_capacity(bar_width * fill_ch.len_utf8());
    bar.extend(std::iter::repeat_n(fill_ch, filled));
    bar.extend(std::iter::repeat_n(empty_ch, empty));
    bar
}

fn truncate_to_display_width(s: &str, max_cols: usize, ellipsis: &str) -> String {
    if display_width(s) <= max_cols {
        return s.to_string();
    }
    if max_cols == 0 {
        return String::new();
    }
    let ellipsis_width = display_width(ellipsis);
    let (budget, suffix) = if ellipsis.is_empty() || ellipsis_width >= max_cols {
        (max_cols, "")
    } else {
        (max_cols - ellipsis_width, ellipsis)
    };
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push_str(suffix);
    out
}

struct KnownTotalSpec<'a> {
    asset: &'a str,
    ratio: f64,
    pct: u64,
    dl: &'a str,
    tot: &'a str,
    rate_str: &'a str,
    ascii_bar: bool,
    ellipsis: &'a str,
}

fn fit_known_total_line(spec: KnownTotalSpec<'_>, limit: usize) -> String {
    let KnownTotalSpec {
        asset,
        ratio,
        pct,
        dl,
        tot,
        rate_str,
        ascii_bar,
        ellipsis,
    } = spec;
    let full_metrics = format!("{pct:3}%  {dl} / {tot}  {rate_str}");
    for bar_width in (MIN_PROGRESS_BAR_WIDTH..=PROGRESS_BAR_WIDTH).rev() {
        let candidate = format!(
            "Downloading {asset}  [{}] {full_metrics}",
            build_bar(ratio, bar_width, ascii_bar)
        );
        if display_width(&candidate) <= limit {
            return candidate;
        }
    }

    for bar_width in (MIN_PROGRESS_BAR_WIDTH..=12).rev() {
        let fixed_overhead =
            display_width("Downloading ") + 3 + bar_width + 2 + display_width(&full_metrics);
        if limit >= fixed_overhead + 4 {
            let short_asset = truncate_to_display_width(asset, limit - fixed_overhead, ellipsis);
            let candidate = format!(
                "Downloading {short_asset}  [{}] {full_metrics}",
                build_bar(ratio, bar_width, ascii_bar)
            );
            if display_width(&candidate) <= limit {
                return candidate;
            }
        }
    }

    let compact_metrics = format!("{pct:3}%  {dl}/{tot}");
    let short_metrics = format!("{pct:3}%  {dl}");
    let pct_only = format!("{pct:3}%");
    let bar_min = build_bar(ratio, MIN_PROGRESS_BAR_WIDTH, ascii_bar);
    for candidate in [
        format!("Downloading  [{bar_min}] {full_metrics}"),
        format!("Downloading  [{bar_min}] {compact_metrics}"),
        format!("[{bar_min}] {compact_metrics}"),
        format!("[{bar_min}] {short_metrics}"),
        format!("[{bar_min}] {pct_only}"),
    ] {
        if display_width(&candidate) <= limit {
            return candidate;
        }
    }

    if display_width(&pct_only) <= limit {
        return pct_only;
    }
    let compact_pct = format!("{pct}%");
    truncate_to_display_width(&compact_pct, limit, "")
}

fn fit_unknown_total_line(
    asset: &str,
    dl: &str,
    rate_str: &str,
    ellipsis: &str,
    limit: usize,
) -> String {
    let full = format!("Downloading {asset}  {dl}  {rate_str}");
    if display_width(&full) <= limit {
        return full;
    }
    let overhead =
        display_width("Downloading ") + 2 + display_width(dl) + 2 + display_width(rate_str);
    if limit >= overhead + 4 {
        let short_asset = truncate_to_display_width(asset, limit - overhead, ellipsis);
        let candidate = format!("Downloading {short_asset}  {dl}  {rate_str}");
        if display_width(&candidate) <= limit {
            return candidate;
        }
    }
    for candidate in [
        format!("Downloading  {dl}  {rate_str}"),
        format!("{dl}  {rate_str}"),
        dl.to_string(),
    ] {
        if display_width(&candidate) <= limit {
            return candidate;
        }
    }
    truncate_to_display_width(dl, limit, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_and_rate_formatters_scale_units_cleanly() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(17 * 1024 * 1024), "17.0 MiB");
        assert_eq!(format_rate(0.0), "0 B/s");
        assert_eq!(format_rate(360.0 * 1024.0), "360.0 KiB/s");
        assert_eq!(format_rate(2.5 * 1024.0 * 1024.0), "2.5 MiB/s");
    }

    #[test]
    fn progress_line_renders_known_and_unknown_totals() {
        let half = render_progress_line_with_style(
            "alan.tar.gz",
            5 * 1024 * 1024,
            Some(10 * 1024 * 1024),
            Duration::from_secs(2),
            true,
        );
        assert!(half.contains("[##########----------]"), "{half}");
        assert!(half.contains(" 50%"), "{half}");
        assert!(half.contains("5.0 MiB / 10.0 MiB"), "{half}");
        assert!(half.contains("2.5 MiB/s"), "{half}");

        let clamped = render_progress_line_with_style(
            "alan.tar.gz",
            12 * 1024 * 1024,
            Some(10 * 1024 * 1024),
            Duration::from_secs(1),
            false,
        );
        assert!(clamped.contains("100%"), "{clamped}");
        assert!(clamped.contains("10.0 MiB / 10.0 MiB"), "{clamped}");

        let unknown = render_progress_line_with_style(
            "alan.tar.gz",
            2 * 1024 * 1024,
            None,
            Duration::from_secs(4),
            true,
        );
        assert!(unknown.contains("2.0 MiB"), "{unknown}");
        assert!(unknown.contains("512.0 KiB/s"), "{unknown}");
        assert!(!unknown.contains('%'), "{unknown}");
    }

    #[test]
    fn progress_line_fits_every_terminal_width_without_overflow() {
        let total = 18 * 1024 * 1024;
        let downloaded = 9 * 1024 * 1024;
        let elapsed = Duration::from_secs(25);
        for asset in [
            "alan-0.1.1-aarch64-apple-darwin.tar.gz",
            "测试发布包-0.1.1-aarch64-apple-darwin.tar.gz",
        ] {
            for ascii in [false, true] {
                for width in [120, 80, 79, 60, 48, 40, 24, 12, 4] {
                    let known = render_progress_line_for_width(
                        asset,
                        downloaded,
                        Some(total),
                        elapsed,
                        ascii,
                        Some(width),
                    );
                    assert!(
                        display_width(&known) <= width,
                        "known overflow at {width} ({ascii}, {asset}): `{known}` ({} cols)",
                        display_width(&known)
                    );
                    if width == 79 && asset.starts_with("alan-") {
                        assert!(known.contains("Downloading "), "{known}");
                        assert!(known.contains('[') && known.contains(']'), "{known}");
                        assert!(known.contains('%'), "{known}");
                        assert!(known.contains(" / "), "{known}");
                        assert!(known.contains("/s"), "{known}");
                    }

                    let unknown = render_progress_line_for_width(
                        asset,
                        downloaded,
                        None,
                        elapsed,
                        ascii,
                        Some(width),
                    );
                    assert!(
                        display_width(&unknown) <= width,
                        "unknown overflow at {width} ({ascii}, {asset}): `{unknown}` ({} cols)",
                        display_width(&unknown)
                    );
                }
            }
        }
    }

    #[test]
    fn vt100_80_col_redraws_stay_on_single_physical_row() {
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(24).expect("nonzero rows"),
            std::num::NonZeroU16::new(80).expect("nonzero cols"),
            0,
        );
        let asset = "alan-0.1.1-aarch64-apple-darwin.tar.gz";
        let total = 18 * 1024 * 1024;

        parser.process(b"Checking for updates...\r\n");
        for pct in [0u64, 25, 50, 75, 100] {
            let line = render_progress_line_for_width(
                asset,
                total * pct / 100,
                Some(total),
                Duration::from_secs(1 + pct / 25),
                false,
                Some(79),
            );
            parser.process(format!("\r\x1b[2K{line}").as_bytes());
        }
        parser.process(b"\r\nVerifying archive and extracting...\r\n");

        let contents = parser.screen().contents();
        let rows: Vec<&str> = contents.lines().collect();
        assert_eq!(
            rows.first().copied(),
            Some("Checking for updates..."),
            "{contents:?}"
        );
        assert!(
            rows.get(1)
                .is_some_and(|row| row.contains("Downloading") && row.contains("100%")),
            "{contents:?}"
        );
        assert_eq!(
            rows.get(2).copied(),
            Some("Verifying archive and extracting..."),
            "{contents:?}"
        );
        let downloading_rows = contents
            .lines()
            .filter(|row| row.contains("Downloading"))
            .count();
        assert_eq!(downloading_rows, 1, "{contents:?}");
    }
}
