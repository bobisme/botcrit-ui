//! Diff rendering component

use opentui::{OptimizedBuffer, Style};

use super::components::Rect;
use crate::db::ThreadSummary;
use crate::diff::{DiffLine, DiffLineKind, ParsedDiff};
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
            // Show different indicator if expanded vs collapsed
            let (indicator, color) = if anchor.is_expanded {
                ("*", theme.primary) // expanded thread
            } else if anchor.status == "resolved" {
                ("o", theme.success) // resolved thread
            } else {
                ("o", theme.warning) // open thread
            };
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
                render_diff_line(
                    buffer,
                    area.x + thread_col_width,
                    y,
                    content_start,
                    content_width,
                    line,
                    dt,
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

    let mut rows_used = 0;
    let max_rows = area.height as usize;

    // Draw bubble border top
    if rows_used < max_rows {
        let border = format!(
            "{}{}{}",
            "",
            "".repeat(area.width.saturating_sub(2) as usize),
            ""
        );
        buffer.fill_rect(
            area.x,
            area.y + rows_used as u32,
            area.width,
            1,
            theme.panel_bg,
        );
        buffer.draw_text(
            area.x,
            area.y + rows_used as u32,
            &border,
            Style::fg(theme.border),
        );
        rows_used += 1;
    }

    // Draw comments (limit to fit in bubble)
    for comment in comments.iter().take(max_rows.saturating_sub(2)) {
        if rows_used >= max_rows - 1 {
            break;
        }

        // Author line
        let author_line = format!(" {} ", comment.author);
        buffer.fill_rect(
            area.x,
            area.y + rows_used as u32,
            area.width,
            1,
            theme.panel_bg,
        );
        buffer.draw_text(
            area.x,
            area.y + rows_used as u32,
            "",
            Style::fg(theme.border),
        );
        buffer.draw_text(
            area.x + 1,
            area.y + rows_used as u32,
            &author_line,
            Style::fg(theme.primary),
        );
        rows_used += 1;

        if rows_used >= max_rows - 1 {
            break;
        }

        // Comment body (first line only, truncated)
        let body_line = comment.body.lines().next().unwrap_or("");
        let max_body_len = area.width.saturating_sub(4) as usize;
        let truncated_body = if body_line.len() > max_body_len {
            format!("{}...", &body_line[..max_body_len.saturating_sub(3)])
        } else {
            body_line.to_string()
        };

        buffer.fill_rect(
            area.x,
            area.y + rows_used as u32,
            area.width,
            1,
            theme.panel_bg,
        );
        buffer.draw_text(
            area.x,
            area.y + rows_used as u32,
            "",
            Style::fg(theme.border),
        );
        buffer.draw_text(
            area.x + 2,
            area.y + rows_used as u32,
            &truncated_body,
            Style::fg(theme.foreground),
        );
        rows_used += 1;
    }

    // Draw bubble border bottom
    if rows_used < max_rows {
        let border = format!(
            "{}{}{}",
            "",
            "".repeat(area.width.saturating_sub(2) as usize),
            ""
        );
        buffer.fill_rect(
            area.x,
            area.y + rows_used as u32,
            area.width,
            1,
            theme.panel_bg,
        );
        buffer.draw_text(
            area.x,
            area.y + rows_used as u32,
            &border,
            Style::fg(theme.border),
        );
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
    render_diff_with_threads(buffer, area, diff, scroll, theme, &[], &[]);
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
) {
    // Determine colors based on line type
    let (bg, line_num_bg, fg, sign, sign_color) = match line.kind {
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

    // Draw content (truncated if needed)
    let max_content = content_width.saturating_sub(2) as usize;
    let content = if line.content.len() > max_content {
        &line.content[..max_content]
    } else {
        &line.content
    };
    buffer.draw_text(content_x + 1, y, content, Style::fg(fg).with_bg(bg));
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
}

/// Convert a parsed diff into side-by-side lines
fn build_side_by_side_lines(diff: &ParsedDiff) -> Vec<SideBySideLine> {
    let mut result = Vec::new();

    for hunk in &diff.hunks {
        // Add hunk header
        result.push(SideBySideLine {
            left: None,
            right: None,
            is_header: true,
            header: hunk.header.clone(),
        });

        // Process lines in the hunk, pairing removals with additions
        let mut i = 0;
        let lines = &hunk.lines;

        while i < lines.len() {
            let line = &lines[i];

            match line.kind {
                DiffLineKind::Context => {
                    // Context line: show on both sides
                    result.push(SideBySideLine {
                        left: Some(SideLine {
                            line_num: line.old_line.unwrap_or(0),
                            content: line.content.clone(),
                            kind: DiffLineKind::Context,
                        }),
                        right: Some(SideLine {
                            line_num: line.new_line.unwrap_or(0),
                            content: line.content.clone(),
                            kind: DiffLineKind::Context,
                        }),
                        is_header: false,
                        header: String::new(),
                    });
                    i += 1;
                }
                DiffLineKind::Removed => {
                    // Collect consecutive removals
                    let mut removals = Vec::new();
                    while i < lines.len() && lines[i].kind == DiffLineKind::Removed {
                        removals.push(&lines[i]);
                        i += 1;
                    }

                    // Collect consecutive additions that follow
                    let mut additions = Vec::new();
                    while i < lines.len() && lines[i].kind == DiffLineKind::Added {
                        additions.push(&lines[i]);
                        i += 1;
                    }

                    // Pair removals with additions
                    let max_len = removals.len().max(additions.len());
                    for j in 0..max_len {
                        let left = removals.get(j).map(|l| SideLine {
                            line_num: l.old_line.unwrap_or(0),
                            content: l.content.clone(),
                            kind: DiffLineKind::Removed,
                        });
                        let right = additions.get(j).map(|l| SideLine {
                            line_num: l.new_line.unwrap_or(0),
                            content: l.content.clone(),
                            kind: DiffLineKind::Added,
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
                    // Standalone addition (no preceding removal)
                    result.push(SideBySideLine {
                        left: None,
                        right: Some(SideLine {
                            line_num: line.new_line.unwrap_or(0),
                            content: line.content.clone(),
                            kind: DiffLineKind::Added,
                        }),
                        is_header: false,
                        header: String::new(),
                    });
                    i += 1;
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

/// Minimum width for side-by-side mode (each side needs ~60 cols)
pub const SIDE_BY_SIDE_MIN_WIDTH: u32 = 120;

/// Render a parsed diff in side-by-side mode with thread anchors
pub fn render_diff_side_by_side(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    diff: &ParsedDiff,
    scroll: usize,
    theme: &Theme,
    anchors: &[ThreadAnchor],
    comments: &[crate::db::Comment],
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
            let (indicator, color) = if anchor.is_expanded {
                ("*", theme.primary)
            } else if anchor.status == "resolved" {
                ("o", theme.success)
            } else {
                ("o", theme.warning)
            };
            buffer.fill_rect(area.x, y, thread_col_width, 1, theme.panel_bg);
            buffer.draw_text(area.x, y, indicator, Style::fg(color));
        } else {
            buffer.fill_rect(area.x, y, thread_col_width, 1, theme.background);
        }

        if sbs_line.is_header {
            // Hunk header spans the full width
            buffer.fill_rect(
                left_ln_x,
                y,
                area.width - thread_col_width,
                1,
                dt.context_bg,
            );
            let header_display = if sbs_line.header.len() > (area.width - thread_col_width) as usize
            {
                &sbs_line.header[..(area.width - thread_col_width) as usize]
            } else {
                &sbs_line.header
            };
            buffer.draw_text(left_ln_x, y, header_display, Style::fg(dt.hunk_header));
        } else {
            // Render left side
            render_side_line(
                buffer,
                left_ln_x,
                left_content_x,
                y,
                left_content_width,
                &sbs_line.left,
                dt,
                true, // is_left
            );

            // Render divider
            buffer.fill_rect(divider_x, y, 1, 1, theme.border);

            // Render right side
            render_side_line(
                buffer,
                right_ln_x,
                right_content_x,
                y,
                right_content_width,
                &sbs_line.right,
                dt,
                false, // is_left
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
    _is_left: bool,
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
                Style::fg(dt.line_number).with_bg(line_num_bg),
            );

            // Content
            let max_content = content_width as usize;
            let content = if line.content.len() > max_content {
                &line.content[..max_content]
            } else {
                &line.content
            };
            buffer.fill_rect(content_x, y, content_width, 1, bg);
            buffer.draw_text(content_x, y, content, Style::fg(fg).with_bg(bg));
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
                    let (indicator, color) = if is_expanded {
                        ("*", theme.primary)
                    } else if t.status == "resolved" {
                        ("o", theme.success)
                    } else {
                        ("o", theme.warning)
                    };
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
                let max_content = content_width as usize;
                let display_content = if content.len() > max_content {
                    &content[..max_content]
                } else {
                    content.as_str()
                };
                buffer.fill_rect(content_start, y, content_width, 1, theme.background);
                buffer.draw_text(
                    content_start,
                    y,
                    display_content,
                    Style::fg(theme.foreground),
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
