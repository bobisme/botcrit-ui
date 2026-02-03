//! Orphaned context building, rendering, and calculate_context_ranges.

use opentui::OptimizedBuffer;

use crate::db::ThreadSummary;
use crate::layout::{CONTEXT_LINES, SBS_LINE_NUM_WIDTH};
use crate::syntax::HighlightSpan;
use crate::theme::Theme;
use crate::view::components::Rect;

use super::analysis::{build_thread_ranges, line_in_thread_ranges};
use super::comments::emit_comment_block;
use super::helpers::{
    diff_content_x, draw_diff_base_line, draw_thread_range_bar, orphaned_context_width,
    orphaned_context_x,
};
use super::text_util::{draw_highlighted_text, draw_wrapped_line, wrap_content, WrappedLine};
use super::{DisplayItem, LineRange, OrphanedContext, StreamCursor};

// --- Context range calculation ---

/// Calculate context ranges around threads, merging overlapping ranges.
/// `exclude_ranges` are line ranges already shown in the diff; context lines
/// that fall inside them are trimmed so the same code doesn't appear twice.
pub(super) fn calculate_context_ranges(
    threads: &[&ThreadSummary],
    total_lines: usize,
    exclude_ranges: &[(i64, i64)],
) -> Vec<LineRange> {
    if threads.is_empty() {
        return Vec::new();
    }

    let mut ranges: Vec<LineRange> = threads
        .iter()
        .map(|t| {
            let thread_end = t.selection_end.unwrap_or(t.selection_start);
            LineRange {
                start: (t.selection_start - CONTEXT_LINES).max(1),
                end: (thread_end + CONTEXT_LINES).min(total_lines as i64),
            }
        })
        .collect();

    ranges.sort_by_key(|r| r.start);

    let mut merged: Vec<LineRange> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end + 1 {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        } else {
            merged.push(range);
        }
    }

    if exclude_ranges.is_empty() {
        return merged;
    }

    let mut clipped: Vec<LineRange> = Vec::new();
    for range in merged {
        let mut remaining = vec![range];
        for &(ex_start, ex_end) in exclude_ranges {
            let mut next = Vec::new();
            for r in remaining {
                if r.end < ex_start || r.start > ex_end {
                    next.push(r);
                } else {
                    if r.start < ex_start {
                        next.push(LineRange {
                            start: r.start,
                            end: ex_start - 1,
                        });
                    }
                    if r.end > ex_end {
                        next.push(LineRange {
                            start: ex_end + 1,
                            end: r.end,
                        });
                    }
                }
            }
            remaining = next;
        }
        clipped.extend(remaining);
    }
    clipped.sort_by_key(|r| r.start);
    clipped
}

// --- Context item building ---

pub(super) fn build_context_items(
    lines: &[String],
    threads: &[&ThreadSummary],
    exclude_ranges: &[(i64, i64)],
) -> Vec<DisplayItem> {
    let ranges = calculate_context_ranges(threads, lines.len(), exclude_ranges);
    if ranges.is_empty() {
        return vec![DisplayItem::Separator(0)];
    }

    build_context_items_from_ranges(lines, &ranges)
}

pub(super) fn build_context_items_from_ranges(
    lines: &[String],
    ranges: &[LineRange],
) -> Vec<DisplayItem> {
    if ranges.is_empty() {
        return Vec::new();
    }

    let mut display_items: Vec<DisplayItem> = Vec::new();
    let mut prev_end: Option<i64> = None;

    for range in ranges {
        if let Some(pe) = prev_end {
            if range.start > pe + 1 {
                let gap = range.start - pe - 1;
                display_items.push(DisplayItem::Separator(gap));
            }
        }

        for line_num in range.start..=range.end {
            let idx = (line_num - 1) as usize;
            if idx < lines.len() {
                display_items.push(DisplayItem::Line {
                    line_num,
                    content: lines[idx].clone(),
                });
            }
        }

        prev_end = Some(range.end);
    }

    display_items
}

pub(super) fn group_context_ranges_by_hunks(
    ranges: Vec<LineRange>,
    hunk_ranges: &[(i64, i64)],
) -> Vec<Vec<LineRange>> {
    let mut sections: Vec<Vec<LineRange>> = vec![Vec::new(); hunk_ranges.len() + 1];
    if ranges.is_empty() {
        return sections;
    }

    let mut hunk_idx = 0usize;
    for range in ranges {
        while hunk_idx < hunk_ranges.len() && hunk_ranges[hunk_idx].0 <= range.end {
            hunk_idx += 1;
        }
        sections[hunk_idx].push(range);
    }

    sections
}

// --- Context rendering ---

pub(super) fn emit_orphaned_context_section(
    cursor: &mut StreamCursor<'_>,
    area: Rect,
    comment_area: Rect,
    context: &OrphanedContext<'_>,
    ranges: &[LineRange],
    wrap: bool,
    all_comments: &std::collections::HashMap<String, Vec<crate::db::Comment>>,
    thread_positions: &std::cell::RefCell<std::collections::HashMap<String, usize>>,
    emitted_threads: &mut std::collections::HashSet<String>,
    last_line_num: &mut Option<i64>,
) {
    if ranges.is_empty() {
        return;
    }

    let thread_ranges = build_thread_ranges(&context.threads);
    let dt = &cursor.theme.diff;
    cursor.emit(|buf, y, _| {
        draw_diff_base_line(buf, area, y, dt.context_bg);
    });

    let display_items = build_context_items_from_ranges(context.lines, ranges);
    for item in &display_items {
        if let DisplayItem::Line { line_num, .. } = item {
            if let Some(prev) = last_line_num {
                for thread in &context.threads {
                    let end = thread.selection_end.unwrap_or(thread.selection_start);
                    if !emitted_threads.contains(thread.thread_id.as_str())
                        && end > *prev
                        && end < *line_num
                    {
                        emitted_threads.insert(thread.thread_id.clone());
                        thread_positions
                            .borrow_mut()
                            .insert(thread.thread_id.clone(), cursor.stream_row);
                        if let Some(comments) = all_comments.get(&thread.thread_id) {
                            emit_comment_block(cursor, comment_area, thread, comments);
                        }
                    }
                }
            }
        }

        let show_thread_bar = match item {
            DisplayItem::Line { line_num, .. } => {
                line_in_thread_ranges(Some(*line_num), &thread_ranges)
            }
            DisplayItem::Separator(_) => false,
        };
        match item {
            DisplayItem::Separator(_) => {
                cursor.emit(|buf, y, theme| {
                    render_context_item_block(buf, area, y, item, theme, show_thread_bar, context.highlights);
                });
            }
            DisplayItem::Line {
                line_num,
                content: line_content,
            } => {
                if wrap {
                    let line_index = (*line_num).saturating_sub(1) as usize;
                    let highlight = context.highlights.get(line_index);
                    let line_num_width = SBS_LINE_NUM_WIDTH;
                    let cw = orphaned_context_width(area).saturating_sub(line_num_width) as usize;
                    let wrapped = wrap_content(highlight, line_content, cw);
                    let rows = wrapped.len().max(1);
                    cursor.emit_rows(rows, |buf, y, theme, row| {
                        render_context_line_wrapped_row(
                            buf, area, y, *line_num, theme, &wrapped, row, show_thread_bar,
                        );
                    });
                } else {
                    cursor.emit(|buf, y, theme| {
                        render_context_item_block(
                            buf, area, y, item, theme, show_thread_bar, context.highlights,
                        );
                    });
                }

                let end_match = context.threads.iter().find(|t| {
                    let end = t.selection_end.unwrap_or(t.selection_start);
                    end == *line_num && !emitted_threads.contains(t.thread_id.as_str())
                });
                if let Some(thread) = end_match {
                    emitted_threads.insert(thread.thread_id.clone());
                    thread_positions
                        .borrow_mut()
                        .insert(thread.thread_id.clone(), cursor.stream_row);
                    if let Some(comments) = all_comments.get(&thread.thread_id) {
                        emit_comment_block(cursor, comment_area, thread, comments);
                    }
                }
                *last_line_num = Some(*line_num);
            }
        }
    }
}

pub(super) fn emit_remaining_orphaned_comments(
    cursor: &mut StreamCursor<'_>,
    comment_area: Rect,
    context: &OrphanedContext<'_>,
    all_comments: &std::collections::HashMap<String, Vec<crate::db::Comment>>,
    thread_positions: &std::cell::RefCell<std::collections::HashMap<String, usize>>,
    emitted_threads: &mut std::collections::HashSet<String>,
) {
    let mut remaining: Vec<&&ThreadSummary> = context
        .threads
        .iter()
        .filter(|t| !emitted_threads.contains(t.thread_id.as_str()))
        .collect();
    remaining.sort_by_key(|t| t.selection_start);
    for thread in remaining {
        thread_positions
            .borrow_mut()
            .insert(thread.thread_id.clone(), cursor.stream_row);
        if let Some(comments) = all_comments.get(&thread.thread_id) {
            emit_comment_block(cursor, comment_area, thread, comments);
        }
    }
}

pub(super) fn render_context_item_block(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    item: &DisplayItem,
    theme: &Theme,
    show_thread_bar: bool,
    highlighted_lines: &[Vec<HighlightSpan>],
) {
    let dt = &theme.diff;
    match item {
        DisplayItem::Separator(gap) => {
            draw_diff_base_line(buffer, area, y, dt.context_bg);
            if show_thread_bar {
                draw_thread_range_bar(buffer, diff_content_x(area), y, theme.panel_bg, theme);
            }
            let sep_text = if *gap > 0 {
                format!("··· {} lines ···", gap)
            } else {
                "···".to_string()
            };
            let sep_x = orphaned_context_x(area)
                + orphaned_context_width(area).saturating_sub(sep_text.len() as u32) / 2;
            buffer.draw_text(
                sep_x,
                y,
                &sep_text,
                theme.style_muted_on(dt.context_bg),
            );
        }
        DisplayItem::Line { line_num, content } => {
            draw_diff_base_line(buffer, area, y, dt.context_bg);
            if show_thread_bar {
                draw_thread_range_bar(buffer, diff_content_x(area), y, theme.panel_bg, theme);
            }

            let ln_str = format!("{:5} ", line_num);
            let line_num_width = SBS_LINE_NUM_WIDTH;
            let ln_x = orphaned_context_x(area);
            buffer.fill_rect(ln_x, y, line_num_width, 1, dt.context_bg);
            buffer.draw_text(
                ln_x,
                y,
                &ln_str,
                dt.style_line_number(dt.context_bg),
            );

            let content_x = ln_x + line_num_width;
            let content_width = orphaned_context_width(area).saturating_sub(line_num_width);
            buffer.fill_rect(content_x, y, content_width, 1, dt.context_bg);
            let highlight = highlighted_lines.get((*line_num as usize).saturating_sub(1));
            draw_highlighted_text(
                buffer, content_x, y, content_width,
                highlight, content, dt.context, dt.context_bg,
            );
        }
    }
}

pub(super) fn render_context_line_wrapped_row(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    line_num: i64,
    theme: &Theme,
    wrapped: &[WrappedLine],
    row: usize,
    show_thread_bar: bool,
) {
    let dt = &theme.diff;
    draw_diff_base_line(buffer, area, y, dt.context_bg);
    if show_thread_bar {
        draw_thread_range_bar(buffer, diff_content_x(area), y, theme.panel_bg, theme);
    }

    let ln_str = format!("{:5} ", line_num);
    let line_num_width = SBS_LINE_NUM_WIDTH;
    let ln_x = orphaned_context_x(area);
    buffer.fill_rect(ln_x, y, line_num_width, 1, dt.context_bg);
    if row == 0 {
        buffer.draw_text(
            ln_x, y, &ln_str,
            dt.style_line_number(dt.context_bg),
        );
    }

    let content_x = ln_x + line_num_width;
    let content_width = orphaned_context_width(area).saturating_sub(line_num_width);
    buffer.fill_rect(content_x, y, content_width, 1, dt.context_bg);
    if let Some(line_content) = wrapped.get(row) {
        draw_wrapped_line(
            buffer, content_x, y, content_width,
            line_content, dt.context, dt.context_bg,
        );
    }
}
