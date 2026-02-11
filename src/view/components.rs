//! Reusable UI components

use opentui::buffer::BoxStyle;
use opentui::{OptimizedBuffer, Rgba, Style};

use crate::theme::Theme;

/// A rectangular area for layout
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create from terminal dimensions
    #[must_use]
    pub const fn from_size(width: u16, height: u16) -> Self {
        Self::new(0, 0, width as u32, height as u32)
    }

    /// Inner area after removing border (1 cell on each side)
    #[must_use]
    pub const fn inner(&self) -> Self {
        Self {
            x: self.x + 1,
            y: self.y + 1,
            width: self.width.saturating_sub(2),
            height: self.height.saturating_sub(2),
        }
    }

    /// Split horizontally at a given width from left
    #[must_use]
    pub const fn split_left(&self, width: u32) -> (Self, Self) {
        let left = Self {
            x: self.x,
            y: self.y,
            width,
            height: self.height,
        };
        let right = Self {
            x: self.x + width,
            y: self.y,
            width: self.width.saturating_sub(width),
            height: self.height,
        };
        (left, right)
    }

    /// Split vertically at a given height from top
    #[must_use]
    pub const fn split_top(&self, height: u32) -> (Self, Self) {
        let top = Self {
            x: self.x,
            y: self.y,
            width: self.width,
            height,
        };
        let bottom = Self {
            x: self.x,
            y: self.y + height,
            width: self.width,
            height: self.height.saturating_sub(height),
        };
        (top, bottom)
    }
}

/// Draw a bordered box with optional title
#[allow(dead_code)]
pub fn draw_box(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    border_color: Rgba,
    title: Option<&str>,
    title_color: Rgba,
) {
    buffer.draw_box(
        area.x,
        area.y,
        area.width,
        area.height,
        BoxStyle::rounded(Style::fg(border_color)),
    );

    if let Some(title) = title {
        let title_str = format!(" {title} ");
        buffer.draw_text(
            area.x + 2,
            area.y,
            &title_str,
            Style::fg(title_color).with_bold(),
        );
    }
}

/// Draw a filled rectangle
#[allow(dead_code)]
pub fn fill_rect(buffer: &mut OptimizedBuffer, area: Rect, color: Rgba) {
    buffer.fill_rect(area.x, area.y, area.width, area.height, color);
}

/// Draw text, truncating if necessary
pub fn draw_text_truncated(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    text: &str,
    max_width: u32,
    style: Style,
) {
    if max_width == 0 {
        return;
    }

    let text = if text.len() > max_width as usize {
        if max_width <= 3 {
            text[..max_width as usize].to_string()
        } else {
            let truncated = &text[..max_width.saturating_sub(3) as usize];
            format!("{truncated}...")
        }
    } else {
        text.to_string()
    };

    buffer.draw_text(x, y, &text, style);
}

/// Draw a horizontal line
#[allow(dead_code)]
pub fn draw_hline(buffer: &mut OptimizedBuffer, x: u32, y: u32, width: u32, color: Rgba) {
    let line = "─".repeat(width as usize);
    buffer.draw_text(x, y, &line, Style::fg(color));
}

/// Draw a status badge (e.g., "[open]", "[merged]")
#[allow(dead_code)]
pub fn draw_badge(buffer: &mut OptimizedBuffer, x: u32, y: u32, text: &str, fg: Rgba, bg: Rgba) {
    let badge = format!("[{text}]");
    buffer.draw_text(x, y, &badge, Style::fg(fg).with_bg(bg));
}

/// Format a thread count display
#[must_use]
pub fn format_thread_count(total: i64, open: i64) -> String {
    if total == 0 {
        "0".to_string()
    } else if open == 0 {
        format!("{total}")
    } else {
        format!("{open}/{total}")
    }
}

/// Truncate a path for display, keeping the filename visible
#[must_use]
pub fn truncate_path(path: &str, max_width: usize) -> String {
    if path.len() <= max_width {
        return path.to_string();
    }

    // Try to keep the filename
    if let Some(idx) = path.rfind('/') {
        let filename = &path[idx + 1..];
        if filename.len() + 4 <= max_width {
            // ".../" + filename
            let available = max_width - filename.len() - 4;
            let prefix = &path[..available.min(idx)];
            return format!("{prefix}.../{filename}");
        }
    }

    // Just truncate from the end
    let truncated = &path[..max_width.saturating_sub(1)];
    format!("{truncated}…")
}

/// Dim the cells in `area` by scaling both fg and bg colors.
pub fn dim_rect(buffer: &mut OptimizedBuffer, area: Rect, scale: f32) {
    for row in area.y..area.y + area.height {
        for col in area.x..area.x + area.width {
            if let Some(cell) = buffer.get_mut(col, row) {
                cell.fg = Rgba::new(
                    cell.fg.r * scale,
                    cell.fg.g * scale,
                    cell.fg.b * scale,
                    cell.fg.a,
                );
                cell.bg = Rgba::new(
                    cell.bg.r * scale,
                    cell.bg.g * scale,
                    cell.bg.b * scale,
                    cell.bg.a,
                );
            }
        }
    }
}

/// A label + key hint for the help bar.
pub struct HotkeyHint {
    pub label: &'static str,
    pub key: &'static str,
}

impl HotkeyHint {
    #[must_use]
    pub const fn new(label: &'static str, key: &'static str) -> Self {
        Self { label, key }
    }

    #[must_use]
    pub const fn width(&self) -> usize {
        self.label.len() + 1 + self.key.len()
    }
}

/// Draw a right-aligned help bar of `[label key]` pairs within `area`.
///
/// The bar is drawn on the second-to-last row of `area`. The last row
/// is filled with `theme.background` as a bottom margin.
pub fn draw_help_bar(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    theme: &Theme,
    hints: &[HotkeyHint],
) {
    let y = area.y + area.height.saturating_sub(2);
    let bottom_y = area.y + area.height.saturating_sub(1);
    buffer.fill_rect(area.x, bottom_y, area.width, 1, theme.background);
    buffer.fill_rect(area.x, y, area.width, 1, theme.background);

    if hints.is_empty() || area.width == 0 {
        return;
    }

    let separator = "  ";
    let sep_len = separator.len();
    let total_width: usize = hints
        .iter()
        .map(HotkeyHint::width)
        .sum::<usize>()
        + hints.len().saturating_sub(1) * sep_len;

    let padding: u32 = 2;
    let x_start = if (total_width as u32) + padding <= area.width {
        area.x + area.width - total_width as u32 - padding
    } else {
        area.x + padding.min(area.width)
    };

    let dim = theme.style_muted();
    let bright = theme.style_foreground();

    let mut x = x_start;
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            buffer.draw_text(x, y, separator, dim);
            x += sep_len as u32;
        }
        buffer.draw_text(x, y, hint.label, dim);
        x += hint.label.len() as u32;
        buffer.draw_text(x, y, " ", dim);
        x += 1;
        buffer.draw_text(x, y, hint.key, bright);
        x += hint.key.len() as u32;
    }
}
