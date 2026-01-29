//! Diff rendering component

use opentui::{OptimizedBuffer, Rgba, Style};

use super::components::Rect;
use crate::db::ThreadSummary;
use crate::diff::{DiffLine, DiffLineKind, ParsedDiff};
use crate::stream::{
    block_height, BLOCK_LEFT_PAD, BLOCK_MARGIN, BLOCK_PADDING, BLOCK_RIGHT_PAD, BLOCK_SIDE_MARGIN,
};
use crate::syntax::HighlightSpan;
use crate::text::wrap_text;
use crate::theme::Theme;

/// Thread anchor info for rendering
#[derive(Debug, Clone)]
pub struct ThreadAnchor {
    pub thread_id: String,
    pub display_line: usize,
    pub line_count: usize, // How many lines the thread spans
    pub status: String,
    pub comment_count: i64,
    pub is_expanded: bool,
}

fn thread_marker(anchor: &ThreadAnchor, theme: &Theme) -> (&'static str, Rgba) {
    thread_marker_from_status(anchor.is_expanded, &anchor.status, theme)
}

fn thread_marker_from_status(
    is_expanded: bool,
    status: &str,
    theme: &Theme,
) -> (&'static str, Rgba) {
    if is_expanded {
        if status == "resolved" {
            ("▽", theme.success)
        } else {
            ("▼", theme.warning)
        }
    } else if status == "resolved" {
        ("▷", theme.success)
    } else {
        ("▶", theme.warning)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChangeCounts {
    added: usize,
    removed: usize,
}

fn block_inner_x(area: Rect) -> u32 {
    area.x + BLOCK_SIDE_MARGIN + 1 + BLOCK_LEFT_PAD
}

fn block_inner_width(area: Rect) -> u32 {
    area.width
        .saturating_sub(BLOCK_SIDE_MARGIN * 2 + 1 + BLOCK_LEFT_PAD + BLOCK_RIGHT_PAD)
}

fn draw_block_bar(buffer: &mut OptimizedBuffer, x: u32, y: u32, bg: Rgba, theme: &Theme) {
    buffer.fill_rect(x, y, 1, 1, bg);
    buffer.draw_text(x, y, "┃", Style::fg(theme.muted).with_bg(bg));
}

fn draw_block_base_line(buffer: &mut OptimizedBuffer, area: Rect, y: u32, bg: Rgba, theme: &Theme) {
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

fn draw_block_text_line(
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
    let display_text = if text.len() > content_width {
        &text[..content_width]
    } else {
        text
    };
    draw_block_base_line(buffer, area, y, bg, theme);
    buffer.draw_text(content_x, y, display_text, style.with_bg(bg));
}

fn draw_block_line_with_right(
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
    let right_len = right_text.len();
    let left_max = if right_len > 0 {
        content_width.saturating_sub(right_len + 1)
    } else {
        content_width
    };

    let left_text = if left_max == 0 {
        ""
    } else if left.len() > left_max {
        &left[..left_max]
    } else {
        left
    };

    buffer.draw_text(content_x, y, left_text, left_style.with_bg(bg));

    if right_len > 0 && right_len <= content_width {
        let right_x = content_x + content_width as u32 - right_len as u32;
        buffer.draw_text(right_x, y, right_text, right_style.with_bg(bg));
    }
}

pub fn diff_change_counts(diff: &ParsedDiff) -> ChangeCounts {
    let mut added = 0usize;
    let mut removed = 0usize;
    for hunk in &diff.hunks {
        for line in &hunk.lines {
            match line.kind {
                DiffLineKind::Added => added += 1,
                DiffLineKind::Removed => removed += 1,
                DiffLineKind::Context => {}
            }
        }
    }
    ChangeCounts { added, removed }
}

fn draw_file_header_line(
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
    } else if file_path.len() > left_max {
        &file_path[..left_max]
    } else {
        file_path
    };

    buffer.draw_text(
        content_x,
        y,
        left_text,
        Style::fg(theme.foreground).with_bg(bg),
    );

    if let Some(counts) = counts {
        let right_text = format!("+{} / -{}", counts.added, counts.removed);
        let right_width = right_text.len() as u32;
        if right_width > 0 && right_width as usize <= content_width {
            let mut x = content_x + block_inner_width(area) - right_width;
            let add_text = format!("+{}", counts.added);
            buffer.draw_text(x, y, &add_text, Style::fg(theme.success).with_bg(bg));
            x += add_text.len() as u32;
            buffer.draw_text(x, y, " / ", Style::fg(theme.muted).with_bg(bg));
            x += 3;
            let rem_text = format!("-{}", counts.removed);
            buffer.draw_text(x, y, &rem_text, Style::fg(theme.error).with_bg(bg));
        }
    }
}

/// Map threads to display line indices within the diff
pub fn map_threads_to_diff(
    diff: &ParsedDiff,
    threads: &[&ThreadSummary],
    expanded_thread: Option<&str>,
) -> Vec<ThreadAnchor> {
    let mut anchors = Vec::new();

    // Build maps from line numbers to display line index
    // Check both old and new line numbers since threads could reference either
    let mut old_line_to_display: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    let mut new_line_to_display: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    let mut display_idx = 0;

    for hunk in &diff.hunks {
        display_idx += 1; // hunk header
        for line in &hunk.lines {
            if let Some(old_ln) = line.old_line {
                old_line_to_display.insert(old_ln, display_idx);
            }
            if let Some(new_ln) = line.new_line {
                new_line_to_display.insert(new_ln, display_idx);
            }
            display_idx += 1;
        }
    }

    // Map each thread to its display position
    // Try new line first (most common for comments on new code), then old line
    for thread in threads {
        let start_line = thread.selection_start as u32;
        let display_line = new_line_to_display
            .get(&start_line)
            .or_else(|| old_line_to_display.get(&start_line));

        if let Some(&display_line) = display_line {
            let line_count = thread
                .selection_end
                .map_or(1, |end| (end - thread.selection_start + 1) as usize);

            anchors.push(ThreadAnchor {
                thread_id: thread.thread_id.clone(),
                display_line,
                line_count,
                status: thread.status.clone(),
                comment_count: thread.comment_count,
                is_expanded: expanded_thread == Some(&thread.thread_id),
            });
        }
    }

    // Sort by display line
    anchors.sort_by_key(|a| a.display_line);
    anchors
}

/// Render a parsed diff with thread anchors into the buffer
pub fn render_diff_with_threads(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    diff: &ParsedDiff,
    scroll: usize,
    theme: &Theme,
    anchors: &[ThreadAnchor],
    comments: &[crate::db::Comment],
    highlighted_lines: &[Vec<HighlightSpan>],
) {
    let dt = &theme.diff;

    // Calculate column widths
    // Layout: [thread indicator 2] [old line 5] [space 1] [new line 5] [space 1] [content...]
    let thread_col_width: u32 = 2;
    let line_num_width: u32 = 12; // "XXXXX XXXXX "
    let content_start = area.x + thread_col_width + line_num_width;
    let content_width = area.width.saturating_sub(thread_col_width + line_num_width);

    // Collect all displayable lines (hunks + their lines)
    let mut display_lines: Vec<DisplayLine> = Vec::new();

    for hunk in &diff.hunks {
        display_lines.push(DisplayLine::HunkHeader(hunk.header.clone()));
        for line in &hunk.lines {
            display_lines.push(DisplayLine::Diff(line.clone()));
        }
    }

    // Build a set of display lines that have thread anchors
    let mut thread_at_line: std::collections::HashMap<usize, &ThreadAnchor> =
        std::collections::HashMap::new();
    for anchor in anchors {
        thread_at_line.insert(anchor.display_line, anchor);
    }

    // Calculate which lines are visible, accounting for expanded comment bubbles
    let visible_height = area.height as usize;
    let start = scroll.min(display_lines.len().saturating_sub(1));

    // Render visible lines
    let mut screen_row = 0;
    let mut line_idx = start;

    while screen_row < visible_height && line_idx < display_lines.len() {
        let y = area.y + screen_row as u32;
        let display_line = &display_lines[line_idx];

        // Check if this line has a thread anchor
        let thread_anchor = thread_at_line.get(&line_idx);

        // Draw thread indicator column
        if let Some(anchor) = thread_anchor {
            let (indicator, color) = thread_marker(anchor, theme);
            buffer.fill_rect(area.x, y, thread_col_width, 1, theme.panel_bg);
            buffer.draw_text(area.x, y, indicator, Style::fg(color));
        } else {
            buffer.fill_rect(area.x, y, thread_col_width, 1, theme.background);
        }

        // Draw the diff line
        match display_line {
            DisplayLine::HunkHeader(header) => {
                buffer.fill_rect(
                    area.x + thread_col_width,
                    y,
                    area.width - thread_col_width,
                    1,
                    dt.context_bg,
                );
                let header_display = if header.len() > (area.width - thread_col_width) as usize {
                    &header[..(area.width - thread_col_width) as usize]
                } else {
                    header
                };
                buffer.draw_text(
                    area.x + thread_col_width,
                    y,
                    header_display,
                    Style::fg(dt.hunk_header),
                );
            }
            DisplayLine::Diff(line) => {
                // Get highlights for this line if available
                let highlights = highlighted_lines.get(line_idx);
                render_diff_line(
                    buffer,
                    area.x + thread_col_width,
                    y,
                    content_start,
                    content_width,
                    line,
                    dt,
                    highlights,
                );
            }
        }

        screen_row += 1;
        line_idx += 1;

        // If this line has an expanded thread, render comment bubble
        if let Some(anchor) = thread_anchor {
            if anchor.is_expanded && screen_row < visible_height {
                let bubble_rows = render_comment_bubble(
                    buffer,
                    Rect::new(
                        area.x + thread_col_width + 4,
                        area.y + screen_row as u32,
                        area.width.saturating_sub(thread_col_width + 8),
                        (visible_height - screen_row).min(10) as u32,
                    ),
                    comments,
                    theme,
                );
                screen_row += bubble_rows;
            }
        }
    }

    // Fill remaining area
    if screen_row < visible_height {
        let remaining_start = area.y + screen_row as u32;
        let remaining_height = (visible_height - screen_row) as u32;
        buffer.fill_rect(
            area.x,
            remaining_start,
            area.width,
            remaining_height,
            theme.background,
        );
    }
}

/// Render a comment bubble, returns number of rows used
fn render_comment_bubble(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    comments: &[crate::db::Comment],
    theme: &Theme,
) -> usize {
    if comments.is_empty() || area.height == 0 {
        return 0;
    }

    let content_width = block_inner_width(area) as usize;
    let mut content_lines = Vec::new();
    for comment in comments {
        content_lines.push(format!("@{}", comment.author));
        let wrapped = wrap_text(&comment.body, content_width);
        content_lines.extend(wrapped);
    }

    let total_rows = block_height(content_lines.len());
    let max_rows = area.height as usize;
    let mut rows_used = 0;

    let mut content_idx = 0usize;
    for row in 0..total_rows {
        if rows_used >= max_rows {
            break;
        }
        let y = area.y + rows_used as u32;
        if row < BLOCK_MARGIN {
            buffer.fill_rect(area.x, y, area.width, 1, theme.background);
        } else if row < BLOCK_MARGIN + BLOCK_PADDING {
            draw_block_base_line(buffer, area, y, theme.panel_bg, theme);
        } else if row < BLOCK_MARGIN + BLOCK_PADDING + content_lines.len() {
            let text = &content_lines[content_idx];
            let style = if text.starts_with('@') {
                Style::fg(theme.primary)
            } else {
                Style::fg(theme.foreground)
            };
            draw_block_text_line(buffer, area, y, theme.panel_bg, text, style, theme);
            content_idx += 1;
        } else if row < BLOCK_MARGIN + BLOCK_PADDING + content_lines.len() + BLOCK_PADDING {
            draw_block_base_line(buffer, area, y, theme.panel_bg, theme);
        } else {
            buffer.fill_rect(area.x, y, area.width, 1, theme.background);
        }
        rows_used += 1;
    }

    rows_used
}

/// Render a parsed diff into the buffer (legacy version without threads)
pub fn render_diff(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    diff: &ParsedDiff,
    scroll: usize,
    theme: &Theme,
) {
    render_diff_with_threads(buffer, area, diff, scroll, theme, &[], &[], &[]);
}

struct StreamCursor<'a> {
    buffer: &'a mut OptimizedBuffer,
    area: Rect,
    scroll: usize,
    screen_row: usize,
    stream_row: usize,
    theme: &'a Theme,
}

impl<'a> StreamCursor<'a> {
    fn emit<F>(&mut self, draw: F)
    where
        F: FnOnce(&mut OptimizedBuffer, u32, &Theme),
    {
        if self.stream_row >= self.scroll && self.screen_row < self.area.height as usize {
            let y = self.area.y + self.screen_row as u32;
            draw(self.buffer, y, self.theme);
            self.screen_row += 1;
        }
        self.stream_row += 1;
    }

    fn remaining_rows(&self) -> usize {
        self.area.height.saturating_sub(self.screen_row as u32) as usize
    }
}

pub fn render_pinned_header_block(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    file_path: &str,
    theme: &Theme,
    counts: Option<ChangeCounts>,
) -> usize {
    let content_lines = 1usize;
    let height = block_height(content_lines) as u32;
    if area.height < height {
        return 0;
    }

    let mut cursor = StreamCursor {
        buffer,
        area: Rect::new(area.x, area.y, area.width, height),
        scroll: 0,
        screen_row: 0,
        stream_row: 0,
        theme,
    };

    for _ in 0..BLOCK_MARGIN {
        cursor.emit(|buf, y, _| {
            buf.fill_rect(area.x, y, area.width, 1, theme.background);
        });
    }
    for _ in 0..BLOCK_PADDING {
        cursor.emit(|buf, y, theme| {
            draw_block_base_line(buf, area, y, theme.panel_bg, theme);
        });
    }
    cursor.emit(|buf, y, theme| {
        draw_file_header_line(buf, area, y, theme, file_path, counts);
    });
    for _ in 0..BLOCK_PADDING {
        cursor.emit(|buf, y, theme| {
            draw_block_base_line(buf, area, y, theme.panel_bg, theme);
        });
    }
    for _ in 0..BLOCK_MARGIN {
        cursor.emit(|buf, y, _| {
            buf.fill_rect(area.x, y, area.width, 1, theme.background);
        });
    }

    height as usize
}

pub fn render_diff_stream(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    files: &[crate::model::FileEntry],
    file_cache: &std::collections::HashMap<String, crate::model::FileCacheEntry>,
    threads: &[ThreadSummary],
    expanded_thread: Option<&str>,
    comments: &[crate::db::Comment],
    scroll: usize,
    theme: &Theme,
    view_mode: crate::model::DiffViewMode,
) {
    let mut cursor = StreamCursor {
        buffer,
        area,
        scroll,
        screen_row: 0,
        stream_row: 0,
        theme,
    };

    for file in files {
        if cursor.remaining_rows() == 0 {
            break;
        }

        // File header block
        for _ in 0..BLOCK_MARGIN {
            cursor.emit(|buf, y, _| {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            });
        }
        for _ in 0..BLOCK_PADDING {
            cursor.emit(|buf, y, theme| {
                draw_block_base_line(buf, area, y, theme.panel_bg, theme);
            });
        }
        let counts = file_cache
            .get(&file.path)
            .and_then(|entry| entry.diff.as_ref())
            .map(diff_change_counts);
        cursor.emit(|buf, y, theme| {
            draw_file_header_line(buf, area, y, theme, &file.path, counts);
        });
        for _ in 0..BLOCK_PADDING {
            cursor.emit(|buf, y, theme| {
                draw_block_base_line(buf, area, y, theme.panel_bg, theme);
            });
        }
        for _ in 0..BLOCK_MARGIN {
            cursor.emit(|buf, y, _| {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            });
        }

        if cursor.remaining_rows() == 0 {
            break;
        }

        // Diff/context block
        for _ in 0..BLOCK_MARGIN {
            cursor.emit(|buf, y, _| {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            });
        }
        for _ in 0..BLOCK_PADDING {
            cursor.emit(|buf, y, theme| {
                draw_block_base_line(buf, area, y, theme.diff.context_bg, theme);
            });
        }

        let file_threads: Vec<&ThreadSummary> = threads
            .iter()
            .filter(|t| t.file_path == file.path)
            .collect();

        if let Some(entry) = file_cache.get(&file.path) {
            if let Some(diff) = &entry.diff {
                let anchors = map_threads_to_diff(diff, &file_threads, expanded_thread);
                let mut anchor_map: std::collections::HashMap<usize, &ThreadAnchor> =
                    std::collections::HashMap::new();
                for anchor in &anchors {
                    anchor_map.insert(anchor.display_line, anchor);
                }

                match view_mode {
                    crate::model::DiffViewMode::Unified => {
                        let mut display_lines: Vec<DisplayLine> = Vec::new();
                        for hunk in &diff.hunks {
                            display_lines.push(DisplayLine::HunkHeader(hunk.header.clone()));
                            for line in &hunk.lines {
                                display_lines.push(DisplayLine::Diff(line.clone()));
                            }
                        }

                        for (idx, display_line) in display_lines.iter().enumerate() {
                            let anchor = anchor_map.get(&idx).copied();
                            cursor.emit(|buf, y, theme| {
                                render_unified_diff_line_block(
                                    buf,
                                    area,
                                    y,
                                    display_line,
                                    theme,
                                    anchor,
                                    entry.highlighted_lines.get(idx),
                                );
                            });

                            if let Some(anchor) = anchor {
                                if anchor.is_expanded {
                                    if let Some(thread) = file_threads
                                        .iter()
                                        .find(|t| t.thread_id == anchor.thread_id)
                                    {
                                        emit_comment_block(&mut cursor, area, thread, comments);
                                    }
                                }
                            }

                            if cursor.remaining_rows() == 0 {
                                break;
                            }
                        }
                    }
                    crate::model::DiffViewMode::SideBySide => {
                        let sbs_lines = build_side_by_side_lines(diff);
                        for (idx, sbs_line) in sbs_lines.iter().enumerate() {
                            let anchor = anchor_map.get(&idx).copied();
                            cursor.emit(|buf, y, theme| {
                                render_side_by_side_line_block(
                                    buf,
                                    area,
                                    y,
                                    sbs_line,
                                    theme,
                                    anchor,
                                    entry.highlighted_lines.as_slice(),
                                );
                            });

                            if let Some(anchor) = anchor {
                                if anchor.is_expanded {
                                    if let Some(thread) = file_threads
                                        .iter()
                                        .find(|t| t.thread_id == anchor.thread_id)
                                    {
                                        emit_comment_block(&mut cursor, area, thread, comments);
                                    }
                                }
                            }

                            if cursor.remaining_rows() == 0 {
                                break;
                            }
                        }
                    }
                }
            } else if let Some(content) = &entry.file_content {
                let display_items = build_context_items(content.lines.as_slice(), &file_threads);
                for item in display_items {
                    cursor.emit(|buf, y, theme| {
                        render_context_item_block(
                            buf,
                            area,
                            y,
                            &item,
                            theme,
                            entry.highlighted_lines.as_slice(),
                        );
                    });

                    if let DisplayItem::Line { line_num, .. } = &item {
                        if let Some(thread) =
                            file_threads.iter().find(|t| t.selection_start == *line_num)
                        {
                            if expanded_thread == Some(thread.thread_id.as_str()) {
                                emit_comment_block(&mut cursor, area, thread, comments);
                            }
                        }
                    }

                    if cursor.remaining_rows() == 0 {
                        break;
                    }
                }
            } else {
                cursor.emit(|buf, y, theme| {
                    draw_block_text_line(
                        buf,
                        area,
                        y,
                        theme.panel_bg,
                        "No content available",
                        Style::fg(theme.muted),
                        theme,
                    );
                });
            }
        }

        for _ in 0..BLOCK_PADDING {
            cursor.emit(|buf, y, theme| {
                draw_block_base_line(buf, area, y, theme.diff.context_bg, theme);
            });
        }
        for _ in 0..BLOCK_MARGIN {
            cursor.emit(|buf, y, _| {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            });
        }
    }

    if cursor.remaining_rows() > 0 {
        let remaining_start = area.y + cursor.screen_row as u32;
        let remaining_height = area.height.saturating_sub(cursor.screen_row as u32);
        buffer.fill_rect(
            area.x,
            remaining_start,
            area.width,
            remaining_height,
            theme.background,
        );
    }
}

fn render_unified_diff_line_block(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    display_line: &DisplayLine,
    theme: &Theme,
    anchor: Option<&ThreadAnchor>,
    highlights: Option<&Vec<HighlightSpan>>,
) {
    let dt = &theme.diff;
    match display_line {
        DisplayLine::HunkHeader(_) => {
            draw_block_base_line(buffer, area, y, dt.context_bg, theme);
            let sep = "···";
            let sep_x =
                block_inner_x(area) + block_inner_width(area).saturating_sub(sep.len() as u32) / 2;
            buffer.draw_text(sep_x, y, sep, Style::fg(theme.muted).with_bg(dt.context_bg));
        }
        DisplayLine::Diff(line) => {
            let line_bg = match line.kind {
                DiffLineKind::Added => dt.added_bg,
                DiffLineKind::Removed => dt.removed_bg,
                DiffLineKind::Context => dt.context_bg,
            };
            draw_block_base_line(buffer, area, y, line_bg, theme);

            let thread_x = block_inner_x(area);
            let thread_col_width: u32 = 2;
            buffer.fill_rect(thread_x, y, thread_col_width, 1, line_bg);
            if let Some(anchor) = anchor {
                let (indicator, color) = thread_marker(anchor, theme);
                buffer.draw_text(thread_x, y, indicator, Style::fg(color).with_bg(line_bg));
            }

            let line_num_width: u32 = 12;
            let content_start = thread_x + thread_col_width + line_num_width;
            let content_width =
                block_inner_width(area).saturating_sub(thread_col_width + line_num_width);
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

fn render_side_by_side_line_block(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    sbs_line: &SideBySideLine,
    theme: &Theme,
    anchor: Option<&ThreadAnchor>,
    highlighted_lines: &[Vec<HighlightSpan>],
) {
    let dt = &theme.diff;
    if sbs_line.is_header {
        draw_block_base_line(buffer, area, y, dt.context_bg, theme);
        let sep = "···";
        let sep_x =
            block_inner_x(area) + block_inner_width(area).saturating_sub(sep.len() as u32) / 2;
        buffer.draw_text(sep_x, y, sep, Style::fg(theme.muted).with_bg(dt.context_bg));
        return;
    }

    let base_bg = dt.context_bg;
    draw_block_base_line(buffer, area, y, base_bg, theme);

    let thread_x = block_inner_x(area);
    let thread_col_width: u32 = 2;
    buffer.fill_rect(thread_x, y, thread_col_width, 1, base_bg);
    if let Some(anchor) = anchor {
        let (indicator, color) = thread_marker(anchor, theme);
        buffer.draw_text(thread_x, y, indicator, Style::fg(color).with_bg(base_bg));
    }

    let divider_width: u32 = 1;
    let line_num_width: u32 = 6;
    let available = block_inner_width(area).saturating_sub(thread_col_width + divider_width);
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
        buffer,
        left_ln_x,
        left_content_x,
        y,
        left_content_width,
        &sbs_line.left,
        dt,
        dt.line_number,
        left_highlights,
    );

    buffer.fill_rect(divider_x, y, divider_width, 1, base_bg);

    render_side_line(
        buffer,
        right_ln_x,
        right_content_x,
        y,
        right_content_width,
        &sbs_line.right,
        dt,
        theme.muted,
        right_highlights,
    );
}

#[derive(Clone)]
enum CommentLineKind {
    Header,
    Author,
    Body,
}

#[derive(Clone)]
struct CommentLine {
    left: String,
    right: Option<String>,
    kind: CommentLineKind,
}

fn emit_comment_block(
    cursor: &mut StreamCursor<'_>,
    area: Rect,
    thread: &ThreadSummary,
    comments: &[crate::db::Comment],
) {
    if comments.is_empty() {
        return;
    }

    let content_width = block_inner_width(area) as usize;
    let mut content_lines: Vec<CommentLine> = Vec::new();

    let line_range = if let Some(end) = thread.selection_end {
        format!("{}-{}", thread.selection_start, end)
    } else {
        format!("{}", thread.selection_start)
    };
    let mut right_text = format!("{}:{}", thread.file_path, line_range);
    let right_max = content_width.saturating_sub(thread.thread_id.len().saturating_add(1));
    if right_max > 0 && right_text.len() > right_max {
        right_text = super::components::truncate_path(&right_text, right_max);
    } else if right_max == 0 {
        right_text.clear();
    }
    content_lines.push(CommentLine {
        left: thread.thread_id.clone(),
        right: if right_text.is_empty() {
            None
        } else {
            Some(right_text)
        },
        kind: CommentLineKind::Header,
    });
    content_lines.push(CommentLine {
        left: String::new(),
        right: None,
        kind: CommentLineKind::Body,
    });

    for comment in comments {
        let left = format!("@{}", comment.author);
        let right_max = content_width.saturating_sub(left.len().saturating_add(1));
        let right = if right_max > 0 {
            let mut id = comment.comment_id.clone();
            if id.len() > right_max {
                id.truncate(right_max);
            }
            Some(id)
        } else {
            None
        };
        content_lines.push(CommentLine {
            left,
            right,
            kind: CommentLineKind::Author,
        });
        let wrapped = wrap_text(&comment.body, content_width);
        for line in wrapped {
            content_lines.push(CommentLine {
                left: line,
                right: None,
                kind: CommentLineKind::Body,
            });
        }
    }

    let total_rows = block_height(content_lines.len());
    let mut content_idx = 0usize;

    for row in 0..total_rows {
        cursor.emit(|buf, y, theme| {
            if row < BLOCK_MARGIN {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            } else if row < BLOCK_MARGIN + BLOCK_PADDING {
                draw_block_base_line(buf, area, y, theme.panel_bg, theme);
            } else if row < BLOCK_MARGIN + BLOCK_PADDING + content_lines.len() {
                let line = &content_lines[content_idx];
                let (left_style, right_style) = match line.kind {
                    CommentLineKind::Header => (Style::fg(theme.muted), Style::fg(theme.muted)),
                    CommentLineKind::Author => (Style::fg(theme.primary), Style::fg(theme.muted)),
                    CommentLineKind::Body => (Style::fg(theme.foreground), Style::fg(theme.muted)),
                };
                draw_block_line_with_right(
                    buf,
                    area,
                    y,
                    theme.panel_bg,
                    &line.left,
                    line.right.as_deref(),
                    left_style,
                    right_style,
                    theme,
                );
                content_idx += 1;
            } else if row < BLOCK_MARGIN + BLOCK_PADDING + content_lines.len() + BLOCK_PADDING {
                draw_block_base_line(buf, area, y, theme.panel_bg, theme);
            } else {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            }
        });
    }
}

fn build_context_items(lines: &[String], threads: &[&ThreadSummary]) -> Vec<DisplayItem> {
    let ranges = calculate_context_ranges(threads, lines.len());
    if ranges.is_empty() {
        return vec![DisplayItem::Separator(0)];
    }

    let mut display_items: Vec<DisplayItem> = Vec::new();
    let mut prev_end: Option<i64> = None;

    for range in &ranges {
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

fn render_context_item_block(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    y: u32,
    item: &DisplayItem,
    theme: &Theme,
    highlighted_lines: &[Vec<HighlightSpan>],
) {
    match item {
        DisplayItem::Separator(gap) => {
            draw_block_base_line(buffer, area, y, theme.panel_bg, theme);
            let sep_text = if *gap > 0 {
                format!("··· {} lines ···", gap)
            } else {
                "···".to_string()
            };
            let sep_x = block_inner_x(area)
                + block_inner_width(area).saturating_sub(sep_text.len() as u32) / 2;
            buffer.draw_text(
                sep_x,
                y,
                &sep_text,
                Style::fg(theme.muted).with_bg(theme.panel_bg),
            );
        }
        DisplayItem::Line { line_num, content } => {
            draw_block_base_line(buffer, area, y, theme.background, theme);

            let ln_str = format!("{:5} ", line_num);
            let line_num_width: u32 = 6;
            let ln_x = block_inner_x(area);
            buffer.fill_rect(ln_x, y, line_num_width, 1, theme.panel_bg);
            buffer.draw_text(
                ln_x,
                y,
                &ln_str,
                Style::fg(theme.muted).with_bg(theme.panel_bg),
            );

            let content_x = ln_x + line_num_width;
            let content_width = block_inner_width(area).saturating_sub(line_num_width);
            buffer.fill_rect(content_x, y, content_width, 1, theme.background);
            let highlight = highlighted_lines.get((*line_num as usize).saturating_sub(1));
            draw_highlighted_text(
                buffer,
                content_x,
                y,
                content_width,
                highlight,
                content,
                theme.foreground,
                theme.background,
            );
        }
    }
}

fn draw_highlighted_text(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    max_width: u32,
    spans: Option<&Vec<HighlightSpan>>,
    fallback_text: &str,
    fallback_fg: Rgba,
    bg: Rgba,
) {
    let max_chars = max_width as usize;

    if let Some(spans) = spans {
        if spans.is_empty() {
            let content = if fallback_text.len() > max_chars {
                &fallback_text[..max_chars]
            } else {
                fallback_text
            };
            buffer.draw_text(x, y, content, Style::fg(fallback_fg).with_bg(bg));
            return;
        }

        let mut col = x;
        let mut chars_drawn = 0;
        for span in spans {
            if chars_drawn >= max_chars {
                break;
            }
            let remaining = max_chars - chars_drawn;
            let text = if span.text.len() > remaining {
                &span.text[..remaining]
            } else {
                &span.text
            };
            if !text.is_empty() {
                buffer.draw_text(col, y, text, Style::fg(span.fg).with_bg(bg));
                col += text.len() as u32;
                chars_drawn += text.len();
            }
        }
    } else {
        let content = if fallback_text.len() > max_chars {
            &fallback_text[..max_chars]
        } else {
            fallback_text
        };
        buffer.draw_text(x, y, content, Style::fg(fallback_fg).with_bg(bg));
    }
}

/// Render a single diff line
fn render_diff_line(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    content_x: u32,
    content_width: u32,
    line: &DiffLine,
    dt: &crate::theme::DiffTheme,
    highlights: Option<&Vec<HighlightSpan>>,
) {
    // Determine colors based on line type
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

    // Fill line background
    buffer.fill_rect(x, y, 12, 1, line_num_bg); // Line number area
    buffer.fill_rect(content_x, y, content_width, 1, bg); // Content area

    // Draw line numbers
    let old_ln = line
        .old_line
        .map_or("     ".to_string(), |n| format!("{:>5}", n));
    let new_ln = line
        .new_line
        .map_or("     ".to_string(), |n| format!("{:>5}", n));

    buffer.draw_text(
        x,
        y,
        &old_ln,
        Style::fg(dt.line_number).with_bg(line_num_bg),
    );
    buffer.draw_text(
        x + 5,
        y,
        " ",
        Style::fg(dt.line_number).with_bg(line_num_bg),
    );
    buffer.draw_text(
        x + 6,
        y,
        &new_ln,
        Style::fg(dt.line_number).with_bg(line_num_bg),
    );
    buffer.draw_text(
        x + 11,
        y,
        " ",
        Style::fg(dt.line_number).with_bg(line_num_bg),
    );

    // Draw sign
    buffer.draw_text(content_x, y, sign, Style::fg(sign_color).with_bg(bg));

    // Draw content with syntax highlighting if available
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

/// A line to display (either hunk header or diff line)
enum DisplayLine {
    HunkHeader(String),
    Diff(DiffLine),
}

/// A paired line for side-by-side display
#[derive(Debug, Clone)]
struct SideBySideLine {
    /// Left side (old file) - None means empty/padding
    left: Option<SideLine>,
    /// Right side (new file) - None means empty/padding
    right: Option<SideLine>,
    /// Is this a hunk header?
    is_header: bool,
    /// Header text (if is_header)
    header: String,
}

/// One side of a side-by-side line
#[derive(Debug, Clone)]
struct SideLine {
    line_num: u32,
    content: String,
    kind: DiffLineKind,
    /// Display line index in the unified diff (for syntax highlighting lookup)
    display_index: usize,
}

/// Convert a parsed diff into side-by-side lines
fn build_side_by_side_lines(diff: &ParsedDiff) -> Vec<SideBySideLine> {
    let mut result = Vec::new();
    let mut display_index = 0;

    for hunk in &diff.hunks {
        // Add hunk header
        result.push(SideBySideLine {
            left: None,
            right: None,
            is_header: true,
            header: hunk.header.clone(),
        });
        display_index += 1;

        // Process lines in the hunk, pairing removals with additions
        let mut i = 0;
        let lines = &hunk.lines;

        while i < lines.len() {
            let line = &lines[i];

            match line.kind {
                DiffLineKind::Context => {
                    let line_index = display_index;
                    // Context line: show on both sides
                    result.push(SideBySideLine {
                        left: Some(SideLine {
                            line_num: line.old_line.unwrap_or(0),
                            content: line.content.clone(),
                            kind: DiffLineKind::Context,
                            display_index: line_index,
                        }),
                        right: Some(SideLine {
                            line_num: line.new_line.unwrap_or(0),
                            content: line.content.clone(),
                            kind: DiffLineKind::Context,
                            display_index: line_index,
                        }),
                        is_header: false,
                        header: String::new(),
                    });
                    i += 1;
                    display_index += 1;
                }
                DiffLineKind::Removed => {
                    // Collect consecutive removals
                    let mut removals: Vec<(&DiffLine, usize)> = Vec::new();
                    while i < lines.len() && lines[i].kind == DiffLineKind::Removed {
                        removals.push((&lines[i], display_index));
                        i += 1;
                        display_index += 1;
                    }

                    // Collect consecutive additions that follow
                    let mut additions: Vec<(&DiffLine, usize)> = Vec::new();
                    while i < lines.len() && lines[i].kind == DiffLineKind::Added {
                        additions.push((&lines[i], display_index));
                        i += 1;
                        display_index += 1;
                    }

                    // Pair removals with additions
                    let max_len = removals.len().max(additions.len());
                    for j in 0..max_len {
                        let left = removals.get(j).map(|(l, idx)| SideLine {
                            line_num: l.old_line.unwrap_or(0),
                            content: l.content.clone(),
                            kind: DiffLineKind::Removed,
                            display_index: *idx,
                        });
                        let right = additions.get(j).map(|(l, idx)| SideLine {
                            line_num: l.new_line.unwrap_or(0),
                            content: l.content.clone(),
                            kind: DiffLineKind::Added,
                            display_index: *idx,
                        });

                        result.push(SideBySideLine {
                            left,
                            right,
                            is_header: false,
                            header: String::new(),
                        });
                    }
                }
                DiffLineKind::Added => {
                    let line_index = display_index;
                    // Standalone addition (no preceding removal)
                    result.push(SideBySideLine {
                        left: None,
                        right: Some(SideLine {
                            line_num: line.new_line.unwrap_or(0),
                            content: line.content.clone(),
                            kind: DiffLineKind::Added,
                            display_index: line_index,
                        }),
                        is_header: false,
                        header: String::new(),
                    });
                    i += 1;
                    display_index += 1;
                }
            }
        }
    }

    result
}

/// Get the total number of display lines in a diff (for scroll bounds)
#[must_use]
pub fn diff_line_count(diff: &ParsedDiff) -> usize {
    diff.hunks
        .iter()
        .map(|h| 1 + h.lines.len()) // 1 for header + lines
        .sum()
}

/// Render a parsed diff in side-by-side mode with thread anchors
pub fn render_diff_side_by_side(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    diff: &ParsedDiff,
    scroll: usize,
    theme: &Theme,
    anchors: &[ThreadAnchor],
    comments: &[crate::db::Comment],
    highlighted_lines: &[Vec<HighlightSpan>],
) {
    let dt = &theme.diff;

    // Build side-by-side lines
    let sbs_lines = build_side_by_side_lines(diff);

    // Layout: [thread 2] [left_ln 5] [space] [left_content] | [right_ln 5] [space] [right_content]
    let thread_col_width: u32 = 2;
    let divider_width: u32 = 1;
    let line_num_width: u32 = 6; // "XXXXX "

    // Calculate content widths for each side
    let available = area.width.saturating_sub(thread_col_width + divider_width);
    let half_width = available / 2;
    let left_content_width = half_width.saturating_sub(line_num_width);
    let right_content_width = half_width.saturating_sub(line_num_width);

    // Column positions
    let left_ln_x = area.x + thread_col_width;
    let left_content_x = left_ln_x + line_num_width;
    let divider_x = area.x + thread_col_width + half_width;
    let right_ln_x = divider_x + divider_width;
    let right_content_x = right_ln_x + line_num_width;

    // Build thread anchor lookup by display line
    let mut thread_at_line: std::collections::HashMap<usize, &ThreadAnchor> =
        std::collections::HashMap::new();
    for anchor in anchors {
        thread_at_line.insert(anchor.display_line, anchor);
    }

    // Render visible lines
    let visible_height = area.height as usize;
    let start = scroll.min(sbs_lines.len().saturating_sub(1));

    let mut screen_row = 0;
    let mut line_idx = start;

    while screen_row < visible_height && line_idx < sbs_lines.len() {
        let y = area.y + screen_row as u32;
        let sbs_line = &sbs_lines[line_idx];

        // Check for thread anchor
        let thread_anchor = thread_at_line.get(&line_idx);

        // Draw thread indicator column
        if let Some(anchor) = thread_anchor {
            let (indicator, color) = thread_marker(anchor, theme);
            buffer.fill_rect(area.x, y, thread_col_width, 1, theme.panel_bg);
            buffer.draw_text(area.x, y, indicator, Style::fg(color));
        } else {
            buffer.fill_rect(area.x, y, thread_col_width, 1, theme.background);
        }

        if sbs_line.is_header {
            // Subtle separator instead of full @@ header text
            buffer.fill_rect(
                left_ln_x,
                y,
                area.width - thread_col_width,
                1,
                dt.context_bg,
            );
            let separator = "···";
            let sep_x = left_ln_x
                + (area.width - thread_col_width).saturating_sub(separator.len() as u32) / 2;
            buffer.draw_text(sep_x, y, separator, Style::fg(theme.muted));
        } else {
            let left_highlights = sbs_line
                .left
                .as_ref()
                .and_then(|line| highlighted_lines.get(line.display_index));
            let right_highlights = sbs_line
                .right
                .as_ref()
                .and_then(|line| highlighted_lines.get(line.display_index));

            // Render left side
            render_side_line(
                buffer,
                left_ln_x,
                left_content_x,
                y,
                left_content_width,
                &sbs_line.left,
                dt,
                dt.line_number,
                left_highlights,
            );

            // Render divider (subtle gap, no visible line)
            buffer.fill_rect(divider_x, y, divider_width, 1, theme.background);

            // Render right side (dim line numbers)
            render_side_line(
                buffer,
                right_ln_x,
                right_content_x,
                y,
                right_content_width,
                &sbs_line.right,
                dt,
                theme.muted,
                right_highlights,
            );
        }

        screen_row += 1;
        line_idx += 1;

        // If this line has an expanded thread, render comment bubble
        if let Some(anchor) = thread_anchor {
            if anchor.is_expanded && screen_row < visible_height {
                let bubble_rows = render_comment_bubble(
                    buffer,
                    Rect::new(
                        area.x + thread_col_width + 4,
                        area.y + screen_row as u32,
                        area.width.saturating_sub(thread_col_width + 8),
                        (visible_height - screen_row).min(10) as u32,
                    ),
                    comments,
                    theme,
                );
                screen_row += bubble_rows;
            }
        }
    }

    // Fill remaining area
    if screen_row < visible_height {
        let remaining_start = area.y + screen_row as u32;
        let remaining_height = (visible_height - screen_row) as u32;
        buffer.fill_rect(
            area.x,
            remaining_start,
            area.width,
            remaining_height,
            theme.background,
        );
    }
}

/// Render one side of a side-by-side line
fn render_side_line(
    buffer: &mut OptimizedBuffer,
    ln_x: u32,
    content_x: u32,
    y: u32,
    content_width: u32,
    side: &Option<SideLine>,
    dt: &crate::theme::DiffTheme,
    line_number_color: Rgba,
    highlights: Option<&Vec<HighlightSpan>>,
) {
    match side {
        Some(line) => {
            // Determine colors based on line type
            let (bg, line_num_bg, fg) = match line.kind {
                DiffLineKind::Added => (dt.added_bg, dt.added_line_number_bg, dt.added),
                DiffLineKind::Removed => (dt.removed_bg, dt.removed_line_number_bg, dt.removed),
                DiffLineKind::Context => (dt.context_bg, dt.context_bg, dt.context),
            };

            // Line number
            let ln_str = format!("{:>5} ", line.line_num);
            buffer.fill_rect(ln_x, y, 6, 1, line_num_bg);
            buffer.draw_text(
                ln_x,
                y,
                &ln_str,
                Style::fg(line_number_color).with_bg(line_num_bg),
            );

            // Content
            buffer.fill_rect(content_x, y, content_width, 1, bg);
            draw_highlighted_text(
                buffer,
                content_x,
                y,
                content_width,
                highlights,
                &line.content,
                fg,
                bg,
            );
        }
        None => {
            // Empty side - fill with subtle background
            buffer.fill_rect(ln_x, y, 6, 1, dt.context_bg);
            buffer.fill_rect(content_x, y, content_width, 1, dt.context_bg);
        }
    }
}

/// Get the total number of side-by-side display lines (for scroll bounds)
#[must_use]
pub fn diff_side_by_side_line_count(diff: &ParsedDiff) -> usize {
    build_side_by_side_lines(diff).len()
}

/// Context lines to show before/after each thread
const CONTEXT_LINES: i64 = 5;

/// A range of lines to display
#[derive(Debug, Clone, Copy)]
struct LineRange {
    start: i64, // 1-indexed, inclusive
    end: i64,   // 1-indexed, inclusive
}

/// Calculate context ranges around threads, merging overlapping ranges
fn calculate_context_ranges(
    threads: &[&crate::db::ThreadSummary],
    total_lines: usize,
) -> Vec<LineRange> {
    if threads.is_empty() {
        return Vec::new();
    }

    // Calculate raw ranges with context
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

    // Sort by start line
    ranges.sort_by_key(|r| r.start);

    // Merge overlapping ranges
    let mut merged: Vec<LineRange> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            // If ranges overlap or are adjacent, merge them
            if range.start <= last.end + 1 {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        } else {
            merged.push(range);
        }
    }

    merged
}

/// Render file content with thread anchors (for files without diffs)
/// Shows only context windows around threads, not the entire file
pub fn render_file_context(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    lines: &[String],
    scroll: usize,
    theme: &Theme,
    threads: &[&crate::db::ThreadSummary],
    expanded_thread: Option<&str>,
    comments: &[crate::db::Comment],
    highlighted_lines: &[Vec<HighlightSpan>],
) {
    // Build a map of line numbers that have threads
    let mut thread_at_line: std::collections::HashMap<i64, &crate::db::ThreadSummary> =
        std::collections::HashMap::new();
    for thread in threads {
        thread_at_line.insert(thread.selection_start, *thread);
    }

    // Calculate context ranges around threads
    let ranges = calculate_context_ranges(threads, lines.len());

    if ranges.is_empty() {
        buffer.draw_text(
            area.x + 2,
            area.y + 1,
            "No threads in this file",
            Style::fg(theme.muted),
        );
        return;
    }

    // Build display lines: (line_number or None for separator, line_content or separator text)
    let mut display_items: Vec<DisplayItem> = Vec::new();
    let mut prev_end: Option<i64> = None;

    for range in &ranges {
        // Add separator if there's a gap from previous range
        if let Some(pe) = prev_end {
            if range.start > pe + 1 {
                let gap = range.start - pe - 1;
                display_items.push(DisplayItem::Separator(gap));
            }
        }

        // Add lines in this range
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

    // Layout: [thread indicator 2] [line number 6] [space 1] [content...]
    let thread_col_width: u32 = 2;
    let line_num_width: u32 = 7;
    let content_start = area.x + thread_col_width + line_num_width;
    let content_width = area.width.saturating_sub(thread_col_width + line_num_width);

    let visible_height = area.height as usize;
    let start = scroll.min(display_items.len().saturating_sub(1));

    let mut screen_row = 0;
    let mut item_idx = start;

    while screen_row < visible_height && item_idx < display_items.len() {
        let y = area.y + screen_row as u32;

        match &display_items[item_idx] {
            DisplayItem::Separator(gap) => {
                // Draw separator line
                buffer.fill_rect(area.x, y, area.width, 1, theme.background);
                let sep_text = format!("  ··· {} lines ···", gap);
                buffer.draw_text(area.x + 2, y, &sep_text, Style::fg(theme.muted));
                screen_row += 1;
                item_idx += 1;
            }
            DisplayItem::Line { line_num, content } => {
                // Check if this line has a thread
                let thread = thread_at_line.get(line_num);
                let is_expanded = thread
                    .map(|t| expanded_thread == Some(t.thread_id.as_str()))
                    .unwrap_or(false);

                // Draw thread indicator column
                if let Some(t) = thread {
                    let (indicator, color) =
                        thread_marker_from_status(is_expanded, &t.status, theme);
                    buffer.fill_rect(area.x, y, thread_col_width, 1, theme.panel_bg);
                    buffer.draw_text(area.x, y, indicator, Style::fg(color));
                } else {
                    buffer.fill_rect(area.x, y, thread_col_width, 1, theme.background);
                }

                // Draw line number
                let ln_str = format!("{:5} ", line_num);
                buffer.fill_rect(
                    area.x + thread_col_width,
                    y,
                    line_num_width,
                    1,
                    theme.panel_bg,
                );
                buffer.draw_text(
                    area.x + thread_col_width,
                    y,
                    &ln_str,
                    Style::fg(theme.muted),
                );

                // Draw content
                buffer.fill_rect(content_start, y, content_width, 1, theme.background);
                let highlight = highlighted_lines.get((line_num - 1) as usize);
                draw_highlighted_text(
                    buffer,
                    content_start,
                    y,
                    content_width,
                    highlight,
                    content,
                    theme.foreground,
                    theme.background,
                );

                screen_row += 1;
                item_idx += 1;

                // If this line has an expanded thread, render comment bubble
                if is_expanded && screen_row < visible_height {
                    let bubble_rows = render_comment_bubble(
                        buffer,
                        Rect::new(
                            area.x + thread_col_width + 4,
                            area.y + screen_row as u32,
                            area.width.saturating_sub(thread_col_width + 8),
                            (visible_height - screen_row).min(10) as u32,
                        ),
                        comments,
                        theme,
                    );
                    screen_row += bubble_rows;
                }
            }
        }
    }

    // Fill remaining area
    if screen_row < visible_height {
        let remaining_start = area.y + screen_row as u32;
        let remaining_height = (visible_height - screen_row) as u32;
        buffer.fill_rect(
            area.x,
            remaining_start,
            area.width,
            remaining_height,
            theme.background,
        );
    }
}

/// Display item for file context view
enum DisplayItem {
    /// A separator showing how many lines are skipped
    Separator(i64),
    /// A line of code with its line number
    Line { line_num: i64, content: String },
}
