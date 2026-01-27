//! Review list screen rendering

use opentui::{OptimizedBuffer, Style};

use super::components::{draw_box, draw_text_truncated, format_thread_count, Rect};
use crate::model::Model;

/// Render the review list screen
pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    let theme = &model.theme;
    let area = Rect::from_size(model.width, model.height);

    // Main box with title
    draw_box(
        buffer,
        area,
        theme.border,
        Some("Reviews"),
        theme.foreground,
    );

    let inner = area.inner();
    let reviews = model.filtered_reviews();

    if reviews.is_empty() {
        buffer.draw_text(
            inner.x + 2,
            inner.y + 1,
            "No reviews found",
            Style::fg(theme.muted),
        );
        draw_help_bar(model, buffer, area);
        return;
    }

    // Group reviews by status
    let open_reviews: Vec<_> = reviews.iter().filter(|r| r.status == "open").collect();
    let closed_reviews: Vec<_> = reviews.iter().filter(|r| r.status != "open").collect();

    let mut y = inner.y;

    // Open section
    if !open_reviews.is_empty() {
        buffer.draw_text(inner.x + 1, y, "OPEN", Style::fg(theme.muted).with_bold());
        y += 1;

        for (i, review) in open_reviews.iter().enumerate() {
            let global_idx = i;
            draw_review_row(
                model,
                buffer,
                inner,
                y,
                review,
                global_idx == model.list_index,
            );
            y += 1;
        }

        y += 1; // Spacing
    }

    // Closed section
    if !closed_reviews.is_empty() {
        buffer.draw_text(inner.x + 1, y, "CLOSED", Style::fg(theme.muted).with_bold());
        y += 1;

        for (i, review) in closed_reviews.iter().enumerate() {
            let global_idx = open_reviews.len() + i;
            draw_review_row(
                model,
                buffer,
                inner,
                y,
                review,
                global_idx == model.list_index,
            );
            y += 1;
        }
    }

    // Help bar at bottom
    draw_help_bar(model, buffer, area);
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

    // Selection indicator and background
    let (prefix, style) = if selected {
        (
            "▸ ",
            Style::fg(theme.selection_fg).with_bg(theme.selection_bg),
        )
    } else {
        ("  ", Style::fg(theme.foreground))
    };

    // Fill row background if selected
    if selected {
        buffer.fill_rect(area.x, y, area.width, 1, theme.selection_bg);
    }

    let mut x = area.x;

    // Selection indicator
    buffer.draw_text(x, y, prefix, style);
    x += 2;

    // Review ID
    let id_style = if selected {
        Style::fg(theme.primary).with_bg(theme.selection_bg)
    } else {
        Style::fg(theme.primary)
    };
    buffer.draw_text(x, y, &review.review_id, id_style);
    x += 8; // Fixed width for ID

    // Status badge for closed reviews
    if review.status != "open" {
        let badge = format!("[{}]", review.status);
        let badge_color = match review.status.as_str() {
            "merged" => theme.success,
            "abandoned" => theme.muted,
            "approved" => theme.warning,
            _ => theme.foreground,
        };
        buffer.draw_text(x, y, &badge, Style::fg(badge_color));
        x += badge.len() as u32 + 1;
    }

    // Title (truncated to fit)
    let remaining = area.width.saturating_sub(x - area.x);
    let title_width = remaining.saturating_sub(25).max(10);
    draw_text_truncated(buffer, x, y, &review.title, title_width, style);
    x += title_width + 1;

    // Author
    let remaining = area.width.saturating_sub(x - area.x);
    let author_width = 12.min(remaining.saturating_sub(12));
    if author_width > 0 {
        draw_text_truncated(
            buffer,
            x,
            y,
            &review.author,
            author_width,
            Style::fg(theme.muted),
        );
        x += author_width + 1;
    }

    // Thread count
    let thread_str = format_thread_count(review.thread_count, review.open_thread_count);
    let thread_color = if review.open_thread_count > 0 {
        theme.warning
    } else {
        theme.muted
    };
    let threads_label = format!("{thread_str} threads");
    buffer.draw_text(x, y, &threads_label, Style::fg(thread_color));
}

fn draw_help_bar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let y = area.y + area.height - 1;

    // Draw separator
    buffer.draw_text(
        area.x + 1,
        y,
        &"─".repeat(area.width.saturating_sub(2) as usize),
        Style::fg(theme.border),
    );

    // Help text
    let help = "j/k navigate  Enter select  o open only  a all  q quit";
    buffer.draw_text(area.x + 2, y, help, Style::fg(theme.muted));
}
