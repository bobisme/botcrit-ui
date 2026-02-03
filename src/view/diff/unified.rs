//! Unified diff rendering (single-pane with +/- lines).

use opentui::{OptimizedBuffer, Style};

use crate::diff::{DiffLine, DiffLineKind};
use crate::layout::{THREAD_COL_WIDTH, UNIFIED_LINE_NUM_WIDTH};
use crate::syntax::HighlightSpan;
use crate::theme::Theme;
use crate::view::components::Rect;

use super::helpers::{diff_content_width, diff_content_x, draw_diff_base_line, draw_thread_range_bar};
use super::text_util::{draw_highlighted_text, draw_wrapped_line, WrappedLine};
use super::{DisplayLine, ThreadAnchor};

pub(super) fn render_unified_diff_line_block(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    display_line: &DisplayLine,
    theme: &Theme,
    anchor: Option<&ThreadAnchor>,
    show_thread_bar: bool,
    highlights: Option<&Vec<HighlightSpan>>,
) {
    let dt = &theme.diff;
    match display_line {
        DisplayLine::HunkHeader(_) => {
            draw_diff_base_line(buffer, area, y, dt.context_bg);
            let sep = "···";
            let sep_x = diff_content_x(area)
                + diff_content_width(area).saturating_sub(sep.len() as u32) / 2;
            buffer.draw_text(sep_x, y, sep, theme.style_muted_on(dt.context_bg));
        }
        DisplayLine::Diff(line) => {
            let line_bg = match line.kind {
                DiffLineKind::Added => dt.added_bg,
                DiffLineKind::Removed => dt.removed_bg,
                DiffLineKind::Context => dt.context_bg,
            };
            draw_diff_base_line(buffer, area, y, line_bg);

            let thread_x = diff_content_x(area);
            let thread_col_width = THREAD_COL_WIDTH;
            let _ = anchor;
            if show_thread_bar {
                draw_thread_range_bar(buffer, thread_x, y, theme.panel_bg, theme);
            } else {
                buffer.fill_rect(thread_x, y, thread_col_width, 1, line_bg);
            }

            let line_num_width = UNIFIED_LINE_NUM_WIDTH;
            let content_start = thread_x + thread_col_width + line_num_width;
            let content_width =
                diff_content_width(area).saturating_sub(thread_col_width + line_num_width);
            render_diff_line(
                buffer,
                thread_x + thread_col_width,
                y,
                content_start,
                content_width,
                line,
                dt,
                highlights,
            );
        }
    }
}

pub(super) fn render_unified_diff_line_wrapped_row(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    line: &DiffLine,
    theme: &Theme,
    anchor: Option<&ThreadAnchor>,
    show_thread_bar: bool,
    wrapped: &[WrappedLine],
    row: usize,
) {
    let dt = &theme.diff;
    let (bg, line_num_bg, default_fg, sign, sign_color) = match line.kind {
        DiffLineKind::Added => (
            dt.added_bg,
            dt.added_line_number_bg,
            dt.added,
            "+",
            dt.highlight_added,
        ),
        DiffLineKind::Removed => (
            dt.removed_bg,
            dt.removed_line_number_bg,
            dt.removed,
            "-",
            dt.highlight_removed,
        ),
        DiffLineKind::Context => (dt.context_bg, dt.context_bg, dt.context, " ", dt.context),
    };

    draw_diff_base_line(buffer, area, y, bg);

    let thread_x = diff_content_x(area);
    let thread_col_width = THREAD_COL_WIDTH;
    let _ = (anchor, row);
    if show_thread_bar {
        draw_thread_range_bar(buffer, thread_x, y, theme.panel_bg, theme);
    } else {
        buffer.fill_rect(thread_x, y, thread_col_width, 1, bg);
    }

    let line_num_width = UNIFIED_LINE_NUM_WIDTH;
    let line_num_x = thread_x + thread_col_width;
    buffer.fill_rect(line_num_x, y, line_num_width, 1, line_num_bg);
    if row == 0 {
        let old_ln = line
            .old_line
            .map_or("     ".to_string(), |n| format!("{:>5}", n));
        let new_ln = line
            .new_line
            .map_or("     ".to_string(), |n| format!("{:>5}", n));

        buffer.draw_text(
            line_num_x,
            y,
            &old_ln,
            dt.style_line_number(line_num_bg),
        );
        buffer.draw_text(
            line_num_x + 5,
            y,
            " ",
            dt.style_line_number(line_num_bg),
        );
        buffer.draw_text(
            line_num_x + 6,
            y,
            &new_ln,
            dt.style_line_number(line_num_bg),
        );
        buffer.draw_text(
            line_num_x + 11,
            y,
            " ",
            dt.style_line_number(line_num_bg),
        );
    }

    let content_start = line_num_x + line_num_width;
    let content_width = diff_content_width(area).saturating_sub(thread_col_width + line_num_width);
    buffer.fill_rect(content_start, y, content_width, 1, bg);
    if row == 0 {
        buffer.draw_text(content_start, y, sign, Style::fg(sign_color).with_bg(bg));
    }

    if let Some(line_content) = wrapped.get(row) {
        let max_content = content_width.saturating_sub(2);
        draw_wrapped_line(
            buffer,
            content_start + 1,
            y,
            max_content,
            line_content,
            default_fg,
            bg,
        );
    }
}

/// Render a single unified diff line (line numbers + sign + content)
pub(super) fn render_diff_line(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    content_x: u32,
    content_width: u32,
    line: &DiffLine,
    dt: &crate::theme::DiffTheme,
    highlights: Option<&Vec<HighlightSpan>>,
) {
    let (bg, line_num_bg, default_fg, sign, sign_color) = match line.kind {
        DiffLineKind::Added => (
            dt.added_bg,
            dt.added_line_number_bg,
            dt.added,
            "+",
            dt.highlight_added,
        ),
        DiffLineKind::Removed => (
            dt.removed_bg,
            dt.removed_line_number_bg,
            dt.removed,
            "-",
            dt.highlight_removed,
        ),
        DiffLineKind::Context => (dt.context_bg, dt.context_bg, dt.context, " ", dt.context),
    };

    buffer.fill_rect(x, y, 12, 1, line_num_bg);
    buffer.fill_rect(content_x, y, content_width, 1, bg);

    let old_ln = line
        .old_line
        .map_or("     ".to_string(), |n| format!("{:>5}", n));
    let new_ln = line
        .new_line
        .map_or("     ".to_string(), |n| format!("{:>5}", n));

    buffer.draw_text(x, y, &old_ln, dt.style_line_number(line_num_bg));
    buffer.draw_text(x + 5, y, " ", dt.style_line_number(line_num_bg));
    buffer.draw_text(x + 6, y, &new_ln, dt.style_line_number(line_num_bg));
    buffer.draw_text(x + 11, y, " ", dt.style_line_number(line_num_bg));

    buffer.draw_text(content_x, y, sign, Style::fg(sign_color).with_bg(bg));

    let max_content = content_width.saturating_sub(2);
    draw_highlighted_text(
        buffer,
        content_x + 1,
        y,
        max_content,
        highlights,
        &line.content,
        default_fg,
        bg,
    );
}
