//! Live terminal stage indicator and streaming download progress bar for `alan update`.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

const PROGRESS_BAR_WIDTH: usize = 20;
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
            let line = render_progress_line_with_style(
                asset,
                0,
                total_bytes,
                Duration::ZERO,
                self.ascii_bar,
            );
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
        let line = render_progress_line_with_style(
            asset,
            downloaded,
            total_bytes,
            now.saturating_duration_since(self.started_at),
            self.ascii_bar,
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
            let line = render_progress_line_with_style(
                asset,
                downloaded,
                effective_total,
                elapsed,
                self.ascii_bar,
            );
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "\r\x1b[2K{line}");
            let _ = stderr.flush();
            self.inline_active = false;
        }
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

pub(crate) fn render_progress_line_with_style(
    asset: &str,
    downloaded: u64,
    total_bytes: Option<u64>,
    elapsed: Duration,
    ascii_bar: bool,
) -> String {
    let rate_str = if elapsed.as_secs_f64() > 0.0 && downloaded > 0 {
        format_rate(downloaded as f64 / elapsed.as_secs_f64())
    } else {
        "-- KiB/s".to_string()
    };

    if let Some(total) = total_bytes.filter(|&size| size > 0) {
        let clamped = downloaded.min(total);
        let ratio = (clamped as f64 / total as f64).clamp(0.0, 1.0);
        let filled = (ratio * PROGRESS_BAR_WIDTH as f64).round() as usize;
        let empty = PROGRESS_BAR_WIDTH.saturating_sub(filled);
        let (fill_ch, empty_ch) = if ascii_bar {
            ('#', '-')
        } else {
            ('█', '░')
        };
        let bar = format!(
            "{}{}",
            std::iter::repeat_n(fill_ch, filled).collect::<String>(),
            std::iter::repeat_n(empty_ch, empty).collect::<String>()
        );
        let pct = (ratio * 100.0).round() as u64;
        format!(
            "Downloading {asset}  [{bar}] {pct:3}%  {} / {}  {rate_str}",
            format_bytes(clamped),
            format_bytes(total)
        )
    } else {
        format!(
            "Downloading {asset}  {}  {rate_str}",
            format_bytes(downloaded)
        )
    }
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
}
