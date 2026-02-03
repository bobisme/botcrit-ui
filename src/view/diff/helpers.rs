//! Block, diff, and comment bar helpers — core draw primitives.

use opentui::{OptimizedBuffer, Rgba, Style};

use crate::layout::{
    BLOCK_LEFT_PAD, BLOCK_RIGHT_PAD, BLOCK_SIDE_MARGIN, COMMENT_H_MARGIN, COMMENT_H_PAD,
    DIFF_H_PAD, DIFF_MARGIN, ORPHANED_CONTEXT_LEFT_PAD,
};
use crate::theme::Theme;
use crate::view::components::Rect;

use super::text_util::truncate_chars;
use super::ChangeCounts;

// --- Block helpers (for file headers, pinned headers, comments) ---

pub(super) fn block_inner_x(area: Rect) -> u32 {
    area.x + BLOCK_SIDE_MARGIN + 1 + BLOCK_LEFT_PAD
}

pub(super) fn block_inner_width(area: Rect) -> u32 {
    area.width
        .saturating_sub(BLOCK_SIDE_MARGIN * 2 + 1 + BLOCK_LEFT_PAD + BLOCK_RIGHT_PAD)
}

pub(super) fn draw_block_bar(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    bg: Rgba,
    theme: &Theme,
) {
    buffer.fill_rect(x, y, 1, 1, bg);
    buffer.draw_text(x, y, "┃", theme.style_muted_on(bg));
}

pub(super) fn draw_block_base_line(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    bg: Rgba,
    theme: &Theme,
) {
    if BLOCK_SIDE_MARGIN > 0 {
        buffer.fill_rect(area.x, y, BLOCK_SIDE_MARGIN, 1, theme.background);
        buffer.fill_rect(
            area.x + area.width.saturating_sub(BLOCK_SIDE_MARGIN),
            y,
            BLOCK_SIDE_MARGIN,
            1,
            theme.background,
        );
    }

    let content_x = area.x + BLOCK_SIDE_MARGIN;
    let content_width = area.width.saturating_sub(BLOCK_SIDE_MARGIN * 2);
    buffer.fill_rect(content_x, y, content_width, 1, bg);
    draw_block_bar(buffer, content_x, y, bg, theme);
}

// --- Diff helpers (no bar, no side margins, no padding) ---

pub(super) fn diff_content_x(area: Rect) -> u32 {
    area.x + DIFF_H_PAD
}

pub(super) fn diff_content_width(area: Rect) -> u32 {
    area.width.saturating_sub(DIFF_H_PAD * 2)
}

pub(super) fn orphaned_context_x(area: Rect) -> u32 {
    diff_content_x(area).saturating_add(ORPHANED_CONTEXT_LEFT_PAD)
}

pub(super) fn orphaned_context_width(area: Rect) -> u32 {
    diff_content_width(area).saturating_sub(ORPHANED_CONTEXT_LEFT_PAD)
}

pub(super) fn draw_diff_base_line(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    bg: Rgba,
) {
    buffer.fill_rect(area.x, y, area.width, 1, bg);
}

pub(super) fn diff_margin_area(area: Rect) -> Rect {
    Rect::new(
        area.x + DIFF_MARGIN,
        area.y,
        area.width.saturating_sub(DIFF_MARGIN * 2),
        area.height,
    )
}

// --- Comment bar (┃ in darkest background color) ---

pub(super) fn draw_comment_bar(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    bg: Rgba,
    theme: &Theme,
) {
    buffer.fill_rect(x, y, 1, 1, bg);
    buffer.draw_text(x, y, "┃", Style::fg(theme.background).with_bg(bg));
}

pub(super) fn draw_thread_range_bar(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    bg: Rgba,
    theme: &Theme,
) {
    buffer.fill_rect(x, y, 2, 1, bg);
    buffer.draw_text(x, y, "┃", Style::fg(theme.background).with_bg(bg));
}

/// The comment block area inset by the horizontal margin (bar goes here).
pub(super) fn comment_block_area(area: Rect) -> Rect {
    Rect {
        x: area.x + COMMENT_H_MARGIN,
        width: area.width.saturating_sub(COMMENT_H_MARGIN * 2),
        ..area
    }
}

/// Padded content area inside a comment (after bar + margin + padding).
pub(super) fn comment_content_area(block: Rect) -> Rect {
    // block already has bar at block.x; content starts 1 (bar) + pad from block.x
    Rect {
        x: block.x + 1 + COMMENT_H_PAD,
        width: block.width.saturating_sub(1 + COMMENT_H_PAD * 2),
        ..block
    }
}

pub(super) fn draw_block_text_line(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    bg: Rgba,
    text: &str,
    style: Style,
    theme: &Theme,
) {
    let content_x = block_inner_x(area);
    let content_width = block_inner_width(area) as usize;
    let display_text = truncate_chars(text, content_width);
    draw_block_base_line(buffer, area, y, bg, theme);
    buffer.draw_text(content_x, y, display_text, style.with_bg(bg));
}

pub(super) fn draw_block_line_with_right(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    bg: Rgba,
    left: &str,
    right: Option<&str>,
    left_style: Style,
    right_style: Style,
    theme: &Theme,
) {
    draw_block_base_line(buffer, area, y, bg, theme);

    let content_x = block_inner_x(area);
    let content_width = block_inner_width(area) as usize;
    let right_text = right.unwrap_or("");
    let right_len = right_text.chars().count();
    let left_max = if right_len > 0 {
        content_width.saturating_sub(right_len + 1)
    } else {
        content_width
    };

    let left_text = if left_max == 0 {
        ""
    } else {
        truncate_chars(left, left_max)
    };

    buffer.draw_text(content_x, y, left_text, left_style.with_bg(bg));

    if right_len > 0 && right_len <= content_width {
        let right_x = content_x + content_width as u32 - right_len as u32;
        buffer.draw_text(right_x, y, right_text, right_style.with_bg(bg));
    }
}

/// Draw left/right text directly in the area without block formatting.
pub(super) fn draw_plain_line_with_right(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    bg: Rgba,
    left: &str,
    right: Option<&str>,
    left_style: Style,
    right_style: Style,
) {
    let content_x = area.x;
    let content_width = area.width as usize;
    let right_text = right.unwrap_or("");
    let right_len = right_text.chars().count();
    let left_max = if right_len > 0 {
        content_width.saturating_sub(right_len + 1)
    } else {
        content_width
    };

    let left_text = if left_max == 0 {
        ""
    } else {
        truncate_chars(left, left_max)
    };

    buffer.draw_text(content_x, y, left_text, left_style.with_bg(bg));

    if right_len > 0 && right_len <= content_width {
        let right_x = content_x + content_width as u32 - right_len as u32;
        buffer.draw_text(right_x, y, right_text, right_style.with_bg(bg));
    }
}

pub(super) fn draw_file_header_line(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    theme: &Theme,
    file_path: &str,
    counts: Option<ChangeCounts>,
) {
    let bg = theme.panel_bg;
    draw_block_base_line(buffer, area, y, bg, theme);

    let content_x = block_inner_x(area);
    let content_width = block_inner_width(area) as usize;

    let mut right_len = 0usize;
    if let Some(counts) = counts {
        right_len += format!("+{}", counts.added).len();
        right_len += 3; // " / "
        right_len += format!("-{}", counts.removed).len();
    }

    let left_max = if right_len > 0 {
        content_width.saturating_sub(right_len + 1)
    } else {
        content_width
    };
    let left_text = if left_max == 0 {
        ""
    } else {
        truncate_chars(file_path, left_max)
    };

    buffer.draw_text(
        content_x,
        y,
        left_text,
        theme.style_foreground_on(bg),
    );

    if let Some(counts) = counts {
        let right_text = format!("+{} / -{}", counts.added, counts.removed);
        let right_width = right_text.len() as u32;
        if right_width > 0 && right_width as usize <= content_width {
            let mut x = content_x + block_inner_width(area) - right_width;
            let add_text = format!("+{}", counts.added);
            buffer.draw_text(x, y, &add_text, Style::fg(theme.success).with_bg(bg));
            x += add_text.len() as u32;
            buffer.draw_text(x, y, " / ", theme.style_muted_on(bg));
            x += 3;
            let rem_text = format!("-{}", counts.removed);
            buffer.draw_text(x, y, &rem_text, Style::fg(theme.error).with_bg(bg));
        }
    }
}
