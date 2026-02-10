//! Diff rendering component
//!
//! Sub-modules:
//! - `text_util`: wrapping, truncation, highlighted-text drawing
//! - `helpers`: block/diff/comment bar draw primitives
//! - `analysis`: thread→diff mapping, change counting
//! - `unified`: unified diff line rendering
//! - `side_by_side`: SBS diff line rendering
//! - `comments`: comment block rendering
//! - `context`: orphaned context building + rendering

mod analysis;
mod comments;
mod context;
mod helpers;
mod side_by_side;
mod text_util;
mod unified;

use opentui::OptimizedBuffer;

use super::components::Rect;
use crate::db::ThreadSummary;
use crate::diff::{DiffLine, DiffLineKind, ParsedDiff};
use crate::layout::{
    block_height, BLOCK_MARGIN, BLOCK_PADDING, SBS_LINE_NUM_WIDTH, THREAD_COL_WIDTH,
    UNIFIED_LINE_NUM_WIDTH,
};
use crate::syntax::HighlightSpan;
use crate::theme::Theme;

// Re-export public API
pub use analysis::{diff_change_counts, map_threads_to_diff};

use analysis::{build_thread_ranges, line_in_thread_ranges};
use comments::emit_comment_block;
use context::{
    build_context_items, calculate_context_ranges, emit_orphaned_context_section,
    emit_remaining_orphaned_comments, group_context_ranges_by_hunks, render_context_item_block,
    render_context_line_wrapped_row,
};
use helpers::{
    diff_content_width, diff_margin_area, draw_block_base_line, draw_block_text_line,
    draw_comment_block_base_line, draw_comment_block_text_line, draw_file_header_line,
};
use side_by_side::{render_side_by_side_line_block, render_side_by_side_line_wrapped_row};
use text_util::wrap_content;
use unified::{render_unified_diff_line_block, render_unified_diff_line_wrapped_row};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// Thread anchor info for rendering
#[derive(Debug, Clone)]
pub struct ThreadAnchor {
    pub thread_id: String,
    pub display_line: usize,
    /// Display line after which the comment block should render (end of range)
    pub comment_after_line: usize,
    pub line_count: usize, // How many lines the thread spans
    pub status: String,
    pub comment_count: i64,
    pub is_expanded: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChangeCounts {
    pub(super) added: usize,
    pub(super) removed: usize,
}

/// A line to display (either hunk header or diff line)
enum DisplayLine {
    HunkHeader,
    Diff(DiffLine),
}

/// A paired line for side-by-side display
#[derive(Debug, Clone)]
struct SideBySideLine {
    left: Option<SideLine>,
    right: Option<SideLine>,
    is_header: bool,
}

/// One side of a side-by-side line
#[derive(Debug, Clone)]
struct SideLine {
    line_num: u32,
    content: String,
    kind: DiffLineKind,
    display_index: usize,
}

/// Display item for file context view
enum DisplayItem {
    Separator(#[allow(dead_code)] i64),
    Line { line_num: i64, content: String },
}

/// A range of lines to display
#[derive(Debug, Clone, Copy)]
struct LineRange {
    start: i64,
    end: i64,
}

struct StreamCursor<'a> {
    buffer: &'a mut OptimizedBuffer,
    area: Rect,
    scroll: usize,
    screen_row: usize,
    stream_row: usize,
    theme: &'a Theme,
}

struct OrphanedContext<'a> {
    sections: Vec<Vec<LineRange>>,
    threads: Vec<&'a ThreadSummary>,
    lines: &'a [String],
    highlights: &'a [Vec<HighlightSpan>],
}

impl StreamCursor<'_> {
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

    fn emit_rows<F>(&mut self, rows: usize, mut draw: F)
    where
        F: FnMut(&mut OptimizedBuffer, u32, &Theme, usize),
    {
        for row in 0..rows {
            if self.stream_row >= self.scroll && self.screen_row < self.area.height as usize {
                let y = self.area.y + self.screen_row as u32;
                draw(self.buffer, y, self.theme, row);
                self.screen_row += 1;
            }
            self.stream_row += 1;
        }
    }

    fn remaining_rows(&self) -> usize {
        self.area.height.saturating_sub(self.screen_row as u32) as usize
    }
}

// ---------------------------------------------------------------------------
// build_side_by_side_lines (used by stream + SBS rendering)
// ---------------------------------------------------------------------------

fn build_side_by_side_lines(diff: &ParsedDiff) -> Vec<SideBySideLine> {
    let mut result = Vec::new();
    let mut display_index = 0;

    for hunk in &diff.hunks {
        result.push(SideBySideLine {
            left: None,
            right: None,
            is_header: true,
        });
        display_index += 1;

        let mut i = 0;
        let lines = &hunk.lines;

        while i < lines.len() {
            let line = &lines[i];
            match line.kind {
                DiffLineKind::Context => {
                    let line_index = display_index;
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
                    });
                    i += 1;
                    display_index += 1;
                }
                DiffLineKind::Removed => {
                    let mut removals: Vec<(&DiffLine, usize)> = Vec::new();
                    while i < lines.len() && lines[i].kind == DiffLineKind::Removed {
                        removals.push((&lines[i], display_index));
                        i += 1;
                        display_index += 1;
                    }
                    let mut additions: Vec<(&DiffLine, usize)> = Vec::new();
                    while i < lines.len() && lines[i].kind == DiffLineKind::Added {
                        additions.push((&lines[i], display_index));
                        i += 1;
                        display_index += 1;
                    }
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
                            });
                    }
                }
                DiffLineKind::Added => {
                    let line_index = display_index;
                    result.push(SideBySideLine {
                        left: None,
                        right: Some(SideLine {
                            line_num: line.new_line.unwrap_or(0),
                            content: line.content.clone(),
                            kind: DiffLineKind::Added,
                            display_index: line_index,
                        }),
                        is_header: false,
                    });
                    i += 1;
                    display_index += 1;
                }
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Public stream rendering
// ---------------------------------------------------------------------------

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

/// Render a description block at the top of the stream.
/// Uses comment-style bar color (theme.background) to match comment blocks.
fn render_description_block(
    cursor: &mut StreamCursor<'_>,
    area: Rect,
    description: &str,
    theme: &Theme,
) {
    use crate::text::wrap_text;

    let wrap_width = crate::layout::block_inner_width(area.width) as usize;
    let lines = wrap_text(description, wrap_width);

    // Margin before block
    for _ in 0..BLOCK_MARGIN {
        cursor.emit(|buf, y, _| {
            buf.fill_rect(area.x, y, area.width, 1, theme.background);
        });
    }
    // Padding
    for _ in 0..BLOCK_PADDING {
        cursor.emit(|buf, y, theme| {
            draw_comment_block_base_line(buf, area, y, theme.panel_bg, theme);
        });
    }
    // Content lines
    for line in &lines {
        cursor.emit(|buf, y, theme| {
            draw_comment_block_text_line(buf, area, y, theme.panel_bg, line, theme.style_foreground(), theme);
        });
    }
    // Padding
    for _ in 0..BLOCK_PADDING {
        cursor.emit(|buf, y, theme| {
            draw_comment_block_base_line(buf, area, y, theme.panel_bg, theme);
        });
    }
    // Margin after block
    for _ in 0..BLOCK_MARGIN {
        cursor.emit(|buf, y, _| {
            buf.fill_rect(area.x, y, area.width, 1, theme.background);
        });
    }
}

pub fn render_diff_stream(
    buffer: &mut OptimizedBuffer,
    area: Rect,
    files: &[crate::model::FileEntry],
    file_cache: &std::collections::HashMap<String, crate::model::FileCacheEntry>,
    threads: &[ThreadSummary],
    all_comments: &std::collections::HashMap<String, Vec<crate::db::Comment>>,
    scroll: usize,
    theme: &Theme,
    view_mode: crate::model::DiffViewMode,
    wrap: bool,
    thread_positions: &std::cell::RefCell<std::collections::HashMap<String, usize>>,
    description: Option<&str>,
) {
    thread_positions.borrow_mut().clear();
    let mut cursor = StreamCursor {
        buffer,
        area,
        scroll,
        screen_row: 0,
        stream_row: 0,
        theme,
    };

    // Render description block if present
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            render_description_block(&mut cursor, area, desc, theme);
        }
    }

    for file in files {
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

        let file_threads: Vec<&ThreadSummary> = threads
            .iter()
            .filter(|t| t.file_path == file.path)
            .collect();
        let thread_ranges = build_thread_ranges(&file_threads);

        if let Some(entry) = file_cache.get(&file.path) {
            if let Some(diff) = &entry.diff {
                let anchors = map_threads_to_diff(diff, &file_threads);
                let anchored_ids: std::collections::HashSet<&str> =
                    anchors.iter().map(|a| a.thread_id.as_str()).collect();
                let orphaned_threads: Vec<&&ThreadSummary> = file_threads
                    .iter()
                    .filter(|t| !anchored_ids.contains(t.thread_id.as_str()))
                    .collect();
                let mut orphaned_context: Option<OrphanedContext<'_>> = None;
                if !orphaned_threads.is_empty() {
                    if let Some(content) = &entry.file_content {
                        let orphaned_deref: Vec<&ThreadSummary> =
                            orphaned_threads.iter().map(|t| **t).collect();
                        let hunk_ranges =
                            crate::diff::hunk_exclusion_ranges(&diff.hunks);
                        let ranges = calculate_context_ranges(
                            &orphaned_deref,
                            content.lines.len(),
                            &hunk_ranges,
                        );
                        let sections = group_context_ranges_by_hunks(ranges, &hunk_ranges);
                        if sections.iter().any(|section| !section.is_empty()) {
                            orphaned_context = Some(OrphanedContext {
                                sections,
                                threads: orphaned_deref,
                                lines: content.lines.as_slice(),
                                highlights: entry.file_highlighted_lines.as_slice(),
                            });
                        }
                    }
                }
                let mut emitted_threads: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut last_line_num: Option<i64> = None;
                let mut anchor_map: std::collections::HashMap<usize, &ThreadAnchor> =
                    std::collections::HashMap::new();
                let mut comment_map: std::collections::HashMap<usize, &ThreadAnchor> =
                    std::collections::HashMap::new();
                for anchor in &anchors {
                    anchor_map.insert(anchor.display_line, anchor);
                    comment_map.insert(anchor.comment_after_line, anchor);
                }

                let line_area = diff_margin_area(area);

                match view_mode {
                    crate::model::DiffViewMode::Unified => {
                        let mut display_lines: Vec<DisplayLine> = Vec::new();
                        for hunk in &diff.hunks {
                            display_lines.push(DisplayLine::HunkHeader);
                            for line in &hunk.lines {
                                display_lines.push(DisplayLine::Diff(line.clone()));
                            }
                        }

                        let mut section_idx = 0usize;
                        for (idx, display_line) in display_lines.iter().enumerate() {
                            if matches!(display_line, DisplayLine::HunkHeader) {
                                if let Some(context) = &orphaned_context {
                                    if let Some(section) = context.sections.get(section_idx) {
                                        emit_orphaned_context_section(
                                            &mut cursor,
                                            line_area,
                                            area,
                                            context,
                                            section,
                                            wrap,
                                            all_comments,
                                            thread_positions,
                                            &mut emitted_threads,
                                            &mut last_line_num,
                                        );
                                    }
                                }
                                section_idx = section_idx.saturating_add(1);
                            }
                            let show_thread_bar = match display_line {
                                DisplayLine::Diff(line) => line_in_thread_ranges(
                                    line.new_line.map(|n| n as i64),
                                    &thread_ranges,
                                ),
                                DisplayLine::HunkHeader => false,
                            };
                            let anchor = anchor_map.get(&idx).copied();
                            if let Some(anchor) = anchor {
                                thread_positions
                                    .borrow_mut()
                                    .entry(anchor.thread_id.clone())
                                    .or_insert(cursor.stream_row);
                            }
                            match display_line {
                                DisplayLine::HunkHeader => {
                                    cursor.emit(|buf, y, theme| {
                                        render_unified_diff_line_block(
                                            buf,
                                            line_area,
                                            y,
                                            display_line,
                                            theme,
                                            anchor,
                                            show_thread_bar,
                                            entry.highlighted_lines.get(idx),
                                        );
                                    });
                                }
                                DisplayLine::Diff(line) => {
                                    if wrap {
                                        let thread_col_width = THREAD_COL_WIDTH;
                                        let line_num_width = UNIFIED_LINE_NUM_WIDTH;
                                        let content_width = diff_content_width(line_area)
                                            .saturating_sub(thread_col_width + line_num_width);
                                        let max_content = content_width.saturating_sub(2) as usize;
                                        let wrapped = wrap_content(
                                            entry.highlighted_lines.get(idx),
                                            &line.content,
                                            max_content,
                                        );
                                        let rows = wrapped.len().max(1);
                                        cursor.emit_rows(rows, |buf, y, theme, row| {
                                            render_unified_diff_line_wrapped_row(
                                                buf,
                                                line_area,
                                                y,
                                                line,
                                                theme,
                                                anchor,
                                                show_thread_bar,
                                                &wrapped,
                                                row,
                                            );
                                        });
                                    } else {
                                        cursor.emit(|buf, y, theme| {
                                            render_unified_diff_line_block(
                                                buf,
                                                line_area,
                                                y,
                                                display_line,
                                                theme,
                                                anchor,
                                                show_thread_bar,
                                                entry.highlighted_lines.get(idx),
                                            );
                                        });
                                    }
                                }
                            }

                            // Emit comment block after the last line of the thread range
                            if let Some(comment_anchor) = comment_map.get(&idx) {
                                thread_positions
                                    .borrow_mut()
                                    .entry(comment_anchor.thread_id.clone())
                                    .or_insert(cursor.stream_row);
                                if let Some(thread) = file_threads
                                    .iter()
                                    .find(|t| t.thread_id == comment_anchor.thread_id)
                                {
                                    if let Some(comments) =
                                        all_comments.get(&comment_anchor.thread_id)
                                    {
                                        emit_comment_block(&mut cursor, area, thread, comments);
                                    }
                                }
                            }
                        }
                        if let Some(context) = &orphaned_context {
                            if let Some(section) = context.sections.get(section_idx) {
                                emit_orphaned_context_section(
                                    &mut cursor,
                                    line_area,
                                    area,
                                    context,
                                    section,
                                    wrap,
                                    all_comments,
                                    thread_positions,
                                    &mut emitted_threads,
                                    &mut last_line_num,
                                );
                            }
                        }
                    }
                    crate::model::DiffViewMode::SideBySide => {
                        let sbs_lines = build_side_by_side_lines(diff);

                        let mut sbs_anchor_map: std::collections::HashMap<usize, &ThreadAnchor> =
                            std::collections::HashMap::new();
                        let mut sbs_comment_map: std::collections::HashMap<usize, &ThreadAnchor> =
                            std::collections::HashMap::new();
                        for anchor in &anchors {
                            if let Some(thread) = file_threads
                                .iter()
                                .find(|t| t.thread_id == anchor.thread_id)
                            {
                                let start = thread.selection_start as u32;
                                let end =
                                    thread.selection_end.unwrap_or(thread.selection_start) as u32;
                                for (si, sl) in sbs_lines.iter().enumerate() {
                                    if sl.right.as_ref().is_some_and(|l| l.line_num == start) {
                                        sbs_anchor_map.insert(si, anchor);
                                    }
                                    if sl.right.as_ref().is_some_and(|l| l.line_num == end) {
                                        sbs_comment_map.insert(si, anchor);
                                    }
                                }
                            }
                        }

                        let mut section_idx = 0usize;
                        for (idx, sbs_line) in sbs_lines.iter().enumerate() {
                            if sbs_line.is_header {
                                if let Some(context) = &orphaned_context {
                                    if let Some(section) = context.sections.get(section_idx) {
                                        emit_orphaned_context_section(
                                            &mut cursor,
                                            line_area,
                                            area,
                                            context,
                                            section,
                                            wrap,
                                            all_comments,
                                            thread_positions,
                                            &mut emitted_threads,
                                            &mut last_line_num,
                                        );
                                    }
                                }
                                section_idx = section_idx.saturating_add(1);
                            }
                            let show_thread_bar = if sbs_line.is_header {
                                false
                            } else {
                                line_in_thread_ranges(
                                    sbs_line.right.as_ref().map(|line| line.line_num as i64),
                                    &thread_ranges,
                                )
                            };
                            let anchor = sbs_anchor_map.get(&idx).copied();
                            if let Some(anchor) = anchor {
                                thread_positions
                                    .borrow_mut()
                                    .entry(anchor.thread_id.clone())
                                    .or_insert(cursor.stream_row);
                            }
                            if wrap && !sbs_line.is_header {
                                let thread_col_width = THREAD_COL_WIDTH;
                                let divider_width: u32 = 1;
                                let line_num_width = SBS_LINE_NUM_WIDTH;
                                let available = diff_content_width(line_area)
                                    .saturating_sub(thread_col_width + divider_width);
                                let half_width = available / 2;
                                let left_width = half_width.saturating_sub(line_num_width) as usize;
                                let right_width =
                                    half_width.saturating_sub(line_num_width) as usize;

                                let left_highlights = sbs_line.left.as_ref().and_then(|line| {
                                    entry.highlighted_lines.get(line.display_index)
                                });
                                let right_highlights = sbs_line.right.as_ref().and_then(|line| {
                                    entry.highlighted_lines.get(line.display_index)
                                });

                                let left_wrapped = sbs_line.left.as_ref().map(|line| {
                                    wrap_content(left_highlights, &line.content, left_width)
                                });
                                let right_wrapped = sbs_line.right.as_ref().map(|line| {
                                    wrap_content(right_highlights, &line.content, right_width)
                                });

                                let left_rows =
                                    left_wrapped.as_ref().map(Vec::len).unwrap_or(1);
                                let right_rows =
                                    right_wrapped.as_ref().map(Vec::len).unwrap_or(1);
                                let rows = left_rows.max(right_rows);

                                cursor.emit_rows(rows, |buf, y, theme, row| {
                                    render_side_by_side_line_wrapped_row(
                                        buf,
                                        line_area,
                                        y,
                                        sbs_line,
                                        theme,
                                        anchor,
                                        show_thread_bar,
                                        left_wrapped.as_ref(),
                                        right_wrapped.as_ref(),
                                        row,
                                    );
                                });
                            } else {
                                cursor.emit(|buf, y, theme| {
                                    render_side_by_side_line_block(
                                        buf,
                                        line_area,
                                        y,
                                        sbs_line,
                                        theme,
                                        anchor,
                                        show_thread_bar,
                                        entry.highlighted_lines.as_slice(),
                                    );
                                });
                            }

                            // Emit comment block after the last line of the thread range
                            if let Some(comment_anchor) = sbs_comment_map.get(&idx) {
                                thread_positions
                                    .borrow_mut()
                                    .entry(comment_anchor.thread_id.clone())
                                    .or_insert(cursor.stream_row);
                                if let Some(thread) = file_threads
                                    .iter()
                                    .find(|t| t.thread_id == comment_anchor.thread_id)
                                {
                                    if let Some(comments) =
                                        all_comments.get(&comment_anchor.thread_id)
                                    {
                                        emit_comment_block(&mut cursor, area, thread, comments);
                                    }
                                }
                            }
                        }
                        if let Some(context) = &orphaned_context {
                            if let Some(section) = context.sections.get(section_idx) {
                                emit_orphaned_context_section(
                                    &mut cursor,
                                    line_area,
                                    area,
                                    context,
                                    section,
                                    wrap,
                                    all_comments,
                                    thread_positions,
                                    &mut emitted_threads,
                                    &mut last_line_num,
                                );
                            }
                        }
                    }
                }

                if let Some(context) = &orphaned_context {
                    emit_remaining_orphaned_comments(
                        &mut cursor,
                        area,
                        context,
                        all_comments,
                        thread_positions,
                        &emitted_threads,
                    );
                } else if !orphaned_threads.is_empty() {
                    let mut orphaned_sorted = orphaned_threads.clone();
                    orphaned_sorted.sort_by_key(|t| t.selection_start);
                    for thread in &orphaned_sorted {
                        thread_positions
                            .borrow_mut()
                            .insert(thread.thread_id.clone(), cursor.stream_row);
                        if let Some(comments) = all_comments.get(&thread.thread_id) {
                            emit_comment_block(&mut cursor, area, thread, comments);
                        }
                    }
                }
            } else if let Some(content) = &entry.file_content {
                let line_area = diff_margin_area(area);
                let display_items =
                    build_context_items(content.lines.as_slice(), &file_threads, &[]);
                for item in display_items {
                    let show_thread_bar = match &item {
                        DisplayItem::Line { line_num, .. } => {
                            line_in_thread_ranges(Some(*line_num), &thread_ranges)
                        }
                        DisplayItem::Separator(_) => false,
                    };
                    match &item {
                        DisplayItem::Separator(_) => {
                            cursor.emit(|buf, y, theme| {
                                render_context_item_block(
                                    buf,
                                    line_area,
                                    y,
                                    &item,
                                    theme,
                                    show_thread_bar,
                                    entry.highlighted_lines.as_slice(),
                                );
                            });
                        }
                        DisplayItem::Line { line_num, content } => {
                            if wrap {
                                let line_index = (*line_num).saturating_sub(1) as usize;
                                let highlight = entry.highlighted_lines.get(line_index);
                                let line_num_width = SBS_LINE_NUM_WIDTH;
                                let content_width = diff_content_width(line_area)
                                    .saturating_sub(line_num_width)
                                    as usize;
                                let wrapped = wrap_content(highlight, content, content_width);
                                let rows = wrapped.len().max(1);
                                cursor.emit_rows(rows, |buf, y, theme, row| {
                                    render_context_line_wrapped_row(
                                        buf,
                                        line_area,
                                        y,
                                        *line_num,
                                        theme,
                                        &wrapped,
                                        row,
                                        show_thread_bar,
                                    );
                                });
                            } else {
                                cursor.emit(|buf, y, theme| {
                                    render_context_item_block(
                                        buf,
                                        line_area,
                                        y,
                                        &item,
                                        theme,
                                        show_thread_bar,
                                        entry.highlighted_lines.as_slice(),
                                    );
                                });
                            }
                        }
                    }

                    if let DisplayItem::Line { line_num, .. } = &item {
                        if let Some(thread) = file_threads.iter().find(|t| {
                            let end = t.selection_end.unwrap_or(t.selection_start);
                            end == *line_num
                        }) {
                            thread_positions
                                .borrow_mut()
                                .insert(thread.thread_id.clone(), cursor.stream_row);
                            if let Some(comments) = all_comments.get(&thread.thread_id) {
                                emit_comment_block(&mut cursor, area, thread, comments);
                            }
                        }
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
                        theme.style_muted(),
                        theme,
                    );
                });
            }
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
