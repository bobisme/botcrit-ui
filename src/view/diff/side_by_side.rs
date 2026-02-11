//! Side-by-side diff rendering (two-pane with left=old, right=new).

use opentui::{OptimizedBuffer, Rgba, Style};

use crate::diff::DiffLineKind;
use crate::layout::{SBS_LINE_NUM_WIDTH, THREAD_COL_WIDTH};
use crate::syntax::HighlightSpan;
use crate::theme::Theme;

use super::helpers::{diff_content_width, diff_content_x, draw_diff_base_line, draw_thread_range_bar};
use super::text_util::{draw_highlighted_text, draw_wrapped_line, HighlightContent, WrappedLine};
use super::{LineRenderCtx, SideBySideLine, SideLine};

/// Layout coordinates for one side of a side-by-side diff panel.
struct SidePanelLayout<'a> {
    ln_x: u32,
    content_x: u32,
    content_width: u32,
    dt: &'a crate::theme::DiffTheme,
    line_number_color: Rgba,
}

pub(super) fn render_side_by_side_line_block(
    buffer: &mut OptimizedBuffer,
    y: u32,
    sbs_line: &SideBySideLine,
    theme: &Theme,
    ctx: &LineRenderCtx<'_>,
    highlighted_lines: &[Vec<HighlightSpan>],
) {
    let dt = &theme.diff;
    if sbs_line.is_header {
        draw_diff_base_line(buffer, ctx.area, y, dt.context_bg);
        let sep = "···";
        let sep_x =
            diff_content_x(ctx.area) + diff_content_width(ctx.area).saturating_sub(sep.len() as u32) / 2;
        buffer.draw_text(sep_x, y, sep, theme.style_muted_on(dt.context_bg));
        return;
    }

    let base_bg = dt.context_bg;
    draw_diff_base_line(buffer, ctx.area, y, base_bg);

    let thread_x = diff_content_x(ctx.area);
    let thread_col_width = THREAD_COL_WIDTH;
    let _ = ctx.anchor;
    if ctx.show_thread_bar {
        draw_thread_range_bar(buffer, thread_x, y, theme.panel_bg, theme);
    } else {
        buffer.fill_rect(thread_x, y, thread_col_width, 1, base_bg);
    }

    let divider_width: u32 = 0;
    let line_num_width = SBS_LINE_NUM_WIDTH;
    let available = diff_content_width(ctx.area).saturating_sub(thread_col_width + divider_width);
    let half_width = available / 2;
    let left_content_width = half_width.saturating_sub(line_num_width);
    let right_content_width = half_width.saturating_sub(line_num_width);

    let left_ln_x = thread_x + thread_col_width;
    let left_content_x = left_ln_x + line_num_width;
    let divider_x = thread_x + thread_col_width + half_width;
    let right_ln_x = divider_x + divider_width;
    let right_content_x = right_ln_x + line_num_width;

    let left_highlights = sbs_line
        .left
        .as_ref()
        .and_then(|line| highlighted_lines.get(line.display_index));
    let right_highlights = sbs_line
        .right
        .as_ref()
        .and_then(|line| highlighted_lines.get(line.display_index));

    render_side_line(
        buffer, y, sbs_line.left.as_ref(),
        &SidePanelLayout { ln_x: left_ln_x, content_x: left_content_x, content_width: left_content_width, dt, line_number_color: dt.line_number },
        left_highlights,
    );

    buffer.fill_rect(divider_x, y, divider_width, 1, base_bg);

    render_side_line(
        buffer, y, sbs_line.right.as_ref(),
        &SidePanelLayout { ln_x: right_ln_x, content_x: right_content_x, content_width: right_content_width, dt, line_number_color: theme.muted },
        right_highlights,
    );
}

pub(super) fn render_side_by_side_line_wrapped_row(
    buffer: &mut OptimizedBuffer,
    y: u32,
    sbs_line: &SideBySideLine,
    theme: &Theme,
    ctx: &LineRenderCtx<'_>,
    wrapped_sides: (Option<&Vec<WrappedLine>>, Option<&Vec<WrappedLine>>),
    row: usize,
) {
    let dt = &theme.diff;
    let base_bg = dt.context_bg;
    draw_diff_base_line(buffer, ctx.area, y, base_bg);

    let thread_x = diff_content_x(ctx.area);
    let thread_col_width = THREAD_COL_WIDTH;
    let _ = (ctx.anchor, row);
    if ctx.show_thread_bar {
        draw_thread_range_bar(buffer, thread_x, y, theme.panel_bg, theme);
    } else {
        buffer.fill_rect(thread_x, y, thread_col_width, 1, base_bg);
    }

    let divider_width: u32 = 0;
    let line_num_width = SBS_LINE_NUM_WIDTH;
    let available = diff_content_width(ctx.area).saturating_sub(thread_col_width + divider_width);
    let half_width = available / 2;
    let left_content_width = half_width.saturating_sub(line_num_width);
    let right_content_width = half_width.saturating_sub(line_num_width);

    let left_ln_x = thread_x + thread_col_width;
    let left_content_x = left_ln_x + line_num_width;
    let divider_x = thread_x + thread_col_width + half_width;
    let right_ln_x = divider_x + divider_width;
    let right_content_x = right_ln_x + line_num_width;

    render_side_line_wrapped_row(
        buffer, y, sbs_line.left.as_ref(),
        &SidePanelLayout { ln_x: left_ln_x, content_x: left_content_x, content_width: left_content_width, dt, line_number_color: dt.line_number },
        wrapped_sides.0, row,
    );

    buffer.fill_rect(divider_x, y, divider_width, 1, base_bg);

    render_side_line_wrapped_row(
        buffer, y, sbs_line.right.as_ref(),
        &SidePanelLayout { ln_x: right_ln_x, content_x: right_content_x, content_width: right_content_width, dt, line_number_color: theme.muted },
        wrapped_sides.1, row,
    );
}

fn render_side_line_wrapped_row(
    buffer: &mut OptimizedBuffer,
    y: u32,
    side: Option<&SideLine>,
    layout: &SidePanelLayout<'_>,
    wrapped: Option<&Vec<WrappedLine>>,
    row: usize,
) {
    if let Some(line) = side {
        let (bg, line_num_bg, fg) = match line.kind {
            DiffLineKind::Added => (layout.dt.added_bg, layout.dt.added_line_number_bg, layout.dt.added),
            DiffLineKind::Removed => (layout.dt.removed_bg, layout.dt.removed_line_number_bg, layout.dt.removed),
            DiffLineKind::Context => (layout.dt.context_bg, layout.dt.context_bg, layout.dt.context),
        };

        buffer.fill_rect(layout.ln_x, y, 6, 1, line_num_bg);
        if row == 0 {
            let ln_str = format!("{:>5} ", line.line_num);
            buffer.draw_text(
                layout.ln_x, y, &ln_str,
                Style::fg(layout.line_number_color).with_bg(line_num_bg),
            );
        }

        buffer.fill_rect(layout.content_x, y, layout.content_width, 1, bg);
        if let Some(lines) = wrapped {
            if let Some(line_content) = lines.get(row) {
                draw_wrapped_line(buffer, layout.content_x, y, layout.content_width, line_content, fg, bg);
            }
        }
    } else {
        buffer.fill_rect(layout.ln_x, y, 6, 1, layout.dt.context_bg);
        buffer.fill_rect(layout.content_x, y, layout.content_width, 1, layout.dt.context_bg);
    }
}

fn render_side_line(
    buffer: &mut OptimizedBuffer,
    y: u32,
    side: Option<&SideLine>,
    layout: &SidePanelLayout<'_>,
    highlights: Option<&Vec<HighlightSpan>>,
) {
    if let Some(line) = side {
        let (bg, line_num_bg, fg) = match line.kind {
            DiffLineKind::Added => (layout.dt.added_bg, layout.dt.added_line_number_bg, layout.dt.added),
            DiffLineKind::Removed => (layout.dt.removed_bg, layout.dt.removed_line_number_bg, layout.dt.removed),
            DiffLineKind::Context => (layout.dt.context_bg, layout.dt.context_bg, layout.dt.context),
        };

        let ln_str = format!("{:>5} ", line.line_num);
        buffer.fill_rect(layout.ln_x, y, 6, 1, line_num_bg);
        buffer.draw_text(
            layout.ln_x, y, &ln_str,
            Style::fg(layout.line_number_color).with_bg(line_num_bg),
        );

        buffer.fill_rect(layout.content_x, y, layout.content_width, 1, bg);
        draw_highlighted_text(
            buffer, layout.content_x, y, layout.content_width,
            &HighlightContent {
                spans: highlights,
                fallback_text: &line.content,
                fallback_fg: fg,
                bg,
            },
        );
    } else {
        buffer.fill_rect(layout.ln_x, y, 6, 1, layout.dt.context_bg);
        buffer.fill_rect(layout.content_x, y, layout.content_width, 1, layout.dt.context_bg);
    }
}
