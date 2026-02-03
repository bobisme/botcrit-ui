//! Review list screen rendering

use opentui::{OptimizedBuffer, Style};

use super::components::{draw_help_bar, format_thread_count, HotkeyHint, Rect};
use crate::model::Model;

/// Render the review list screen
pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    let theme = &model.theme;
    let area = Rect::from_size(model.width, model.height);
    let safe_width = area.width;

    buffer.fill_rect(area.x, area.y, safe_width, 1, theme.background);
    buffer.draw_text(
        area.x + 2,
        area.y,
        "Reviews",
        Style::fg(theme.foreground).with_bold(),
    );

    let inner = Rect::new(
        area.x,
        area.y + 1,
        safe_width,
        area.height.saturating_sub(3),
    );
    buffer.fill_rect(
        inner.x,
        inner.y,
        inner.width,
        inner.height,
        theme.background,
    );
    let reviews = model.filtered_reviews();

    if reviews.is_empty() {
        buffer.draw_text(
            inner.x + 2,
            inner.y + 1,
            "No reviews found",
            theme.style_muted(),
        );
        render_help_bar(model, buffer, area);
        return;
    }

    let start = model.list_scroll.min(reviews.len());
    let visible = inner.height as usize;
    let end = (start + visible).min(reviews.len());

    for (row, review) in reviews[start..end].iter().enumerate() {
        let idx = start + row;
        let y = inner.y + row as u32;
        draw_review_row(model, buffer, inner, y, review, idx == model.list_index);
    }

    // Help bar at bottom
    render_help_bar(
        model,
        buffer,
        Rect::new(area.x, area.y, safe_width, area.height),
    );
}

fn draw_review_row(
    model: &Model,
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    review: &crate::db::ReviewSummary,
    selected: bool,
) {
    let theme = &model.theme;

    let bg = if selected {
        theme.selection_bg
    } else {
        theme.background
    };

    // Selection indicator and background
    let (prefix, style) = if selected {
        (
            "> ",
            Style::fg(theme.selection_fg).with_bg(theme.selection_bg),
        )
    } else {
        ("  ", theme.style_foreground_on(bg))
    };

    let row_width = area.width;
    // Fill row background (avoid last column to prevent terminal wrap)
    buffer.fill_rect(area.x, y, row_width, 1, bg);

    let mut x = area.x;

    // Selection indicator
    buffer.draw_text(x, y, prefix, style);
    x += 2;

    let mut remaining = row_width.saturating_sub(x - area.x);
    if remaining == 0 {
        return;
    }

    // Review ID
    let id_style = if selected {
        Style::fg(theme.primary).with_bg(theme.selection_bg)
    } else {
        Style::fg(theme.primary).with_bg(bg)
    };
    let id_text = if review.review_id.len() > 8 {
        &review.review_id[..8]
    } else {
        &review.review_id
    };
    let id_width = draw_segment(buffer, x, y, id_text, remaining, id_style);
    x += id_width.min(8);
    if id_width < 8 && remaining >= 8 {
        x += 8 - id_width;
    }

    remaining = row_width.saturating_sub(x - area.x);
    if remaining == 0 {
        return;
    }

    // Status badge for closed reviews
    if review.status != "open" && remaining > 0 {
        let badge = format!("[{}]", review.status);
        let badge_color = match review.status.as_str() {
            "merged" => theme.success,
            "abandoned" => theme.muted,
            "approved" => theme.warning,
            _ => theme.foreground,
        };
        let used = draw_segment(
            buffer,
            x,
            y,
            &badge,
            remaining,
            Style::fg(badge_color).with_bg(bg),
        );
        x += used;
        if remaining > used {
            x += 1;
        }
    }

    // Title (truncated to fit)
    let remaining = row_width.saturating_sub(x - area.x);
    if remaining == 0 {
        return;
    }
    let title_width = remaining.saturating_sub(25).max(10).min(remaining);
    let used = draw_segment(buffer, x, y, &review.title, title_width, style);
    x += used;
    if remaining > used {
        x += 1;
    }

    // Author
    let remaining = row_width.saturating_sub(x - area.x);
    let author_width = 12.min(remaining.saturating_sub(12));
    if author_width > 0 {
        let used = draw_segment(
            buffer,
            x,
            y,
            &review.author,
            author_width,
            theme.style_muted_on(bg),
        );
        x += used;
        if remaining > used {
            x += 1;
        }
    }

    // Thread count
    let remaining = row_width.saturating_sub(x - area.x);
    if remaining > 0 {
        let thread_str = format_thread_count(review.thread_count, review.open_thread_count);
        let thread_color = if review.open_thread_count > 0 {
            theme.warning
        } else {
            theme.muted
        };
        let threads_label = format!("{thread_str} threads");
        draw_segment(
            buffer,
            x,
            y,
            &threads_label,
            remaining,
            Style::fg(thread_color).with_bg(bg),
        );
    }
}

fn draw_segment(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    text: &str,
    max_width: u32,
    style: Style,
) -> u32 {
    if max_width == 0 {
        return 0;
    }
    let max_width_usize = max_width as usize;
    let display = if text.len() > max_width_usize {
        if max_width_usize <= 3 {
            text[..max_width_usize].to_string()
        } else {
            format!("{}...", &text[..max_width_usize - 3])
        }
    } else {
        text.to_string()
    };
    buffer.draw_text(x, y, &display, style);
    display.len() as u32
}

fn render_help_bar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let hints = &[
        HotkeyHint::new("Commands", "ctrl+p"),
        HotkeyHint::new("Navigate", "j/k"),
        HotkeyHint::new("Select", "Enter"),
        HotkeyHint::new("Open Only", "o"),
        HotkeyHint::new("All", "a"),
        HotkeyHint::new("Quit", "q"),
    ];
    draw_help_bar(buffer, area, &model.theme, hints);
}
