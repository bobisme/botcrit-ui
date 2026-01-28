//! Stream layout helpers for the right pane

use std::collections::HashMap;

use crate::db::{Comment, ThreadSummary};
use crate::diff::ParsedDiff;
use crate::model::{DiffViewMode, FileCacheEntry, FileEntry};
use crate::text::wrap_text;

pub const BLOCK_MARGIN: usize = 1;
pub const BLOCK_PADDING: usize = 1;
pub const BLOCK_SIDE_MARGIN: u32 = 2;
pub const BLOCK_LEFT_PAD: u32 = 2;
pub const BLOCK_RIGHT_PAD: u32 = 2;
pub const SIDE_BY_SIDE_MIN_WIDTH: u32 = 120;

pub struct StreamLayout {
    pub file_offsets: Vec<usize>,
    pub total_lines: usize,
}

pub fn block_height(content_lines: usize) -> usize {
    content_lines + (BLOCK_MARGIN * 2) + (BLOCK_PADDING * 2)
}

pub fn compute_stream_layout(
    files: &[FileEntry],
    file_cache: &HashMap<String, FileCacheEntry>,
    threads: &[ThreadSummary],
    expanded_thread: Option<&str>,
    comments: &[Comment],
    view_mode: DiffViewMode,
    content_width: u32,
) -> StreamLayout {
    let effective_mode =
        if view_mode == DiffViewMode::SideBySide && content_width >= SIDE_BY_SIDE_MIN_WIDTH {
            DiffViewMode::SideBySide
        } else {
            DiffViewMode::Unified
        };
    let mut file_offsets = Vec::with_capacity(files.len());
    let mut total = 0usize;

    for file in files {
        file_offsets.push(total);
        total += block_height(1); // file header block

        if let Some(entry) = file_cache.get(&file.path) {
            let diff_lines = if let Some(diff) = &entry.diff {
                diff_line_count_for_view(diff, effective_mode)
                    + expanded_thread_extra_lines(
                        diff,
                        threads,
                        expanded_thread,
                        comments,
                        content_width,
                    )
            } else if let Some(content) = &entry.file_content {
                context_display_count(content.lines.len(), threads, &file.path)
                    + expanded_context_extra_lines(
                        content.lines.len(),
                        threads,
                        expanded_thread,
                        comments,
                        content_width,
                    )
            } else {
                0
            };

            total += block_height(diff_lines.max(1));
        } else {
            total += block_height(1);
        }
    }

    StreamLayout {
        file_offsets,
        total_lines: total,
    }
}

pub fn active_file_index(layout: &StreamLayout, scroll: usize) -> usize {
    let mut idx = 0;
    for (i, offset) in layout.file_offsets.iter().enumerate() {
        if *offset <= scroll {
            idx = i;
        } else {
            break;
        }
    }
    idx
}

pub fn file_scroll_offset(layout: &StreamLayout, index: usize) -> usize {
    layout.file_offsets.get(index).copied().unwrap_or(0)
}

pub fn thread_stream_offset(
    layout: &StreamLayout,
    files: &[FileEntry],
    file_cache: &HashMap<String, FileCacheEntry>,
    threads: &[ThreadSummary],
    thread_id: &str,
) -> Option<usize> {
    let thread = threads.iter().find(|t| t.thread_id == thread_id)?;
    let file_index = files.iter().position(|f| f.path == thread.file_path)?;
    let file_offset = layout.file_offsets.get(file_index).copied()?;
    let diff_block_start = file_offset + block_height(1);
    let content_start = diff_block_start + BLOCK_MARGIN + BLOCK_PADDING;

    let entry = file_cache.get(&thread.file_path)?;
    if let Some(diff) = &entry.diff {
        let display_line = find_display_line(diff, thread.selection_start as u32)?;
        Some(content_start + display_line)
    } else if let Some(content) = &entry.file_content {
        let display_line = context_line_index(
            content.lines.len(),
            threads,
            &thread.file_path,
            thread.selection_start,
        )?;
        Some(content_start + display_line)
    } else {
        None
    }
}

fn diff_line_count_for_view(diff: &ParsedDiff, view_mode: DiffViewMode) -> usize {
    match view_mode {
        DiffViewMode::Unified => diff_line_count(diff),
        DiffViewMode::SideBySide => side_by_side_line_count(diff),
    }
}

fn diff_line_count(diff: &ParsedDiff) -> usize {
    diff.hunks.iter().map(|h| 1 + h.lines.len()).sum()
}

fn side_by_side_line_count(diff: &ParsedDiff) -> usize {
    let mut count = 0usize;
    for hunk in &diff.hunks {
        count += 1; // header
        let mut i = 0;
        let lines = &hunk.lines;
        while i < lines.len() {
            match lines[i].kind {
                crate::diff::DiffLineKind::Context => {
                    count += 1;
                    i += 1;
                }
                crate::diff::DiffLineKind::Removed => {
                    let mut removals = 0;
                    while i < lines.len() && lines[i].kind == crate::diff::DiffLineKind::Removed {
                        removals += 1;
                        i += 1;
                    }
                    let mut additions = 0;
                    while i < lines.len() && lines[i].kind == crate::diff::DiffLineKind::Added {
                        additions += 1;
                        i += 1;
                    }
                    count += removals.max(additions);
                }
                crate::diff::DiffLineKind::Added => {
                    count += 1;
                    i += 1;
                }
            }
        }
    }
    count
}

fn expanded_thread_extra_lines(
    diff: &ParsedDiff,
    threads: &[ThreadSummary],
    expanded_thread: Option<&str>,
    comments: &[Comment],
    content_width: u32,
) -> usize {
    let Some(thread_id) = expanded_thread else {
        return 0;
    };
    let Some(thread) = threads.iter().find(|t| t.thread_id == thread_id) else {
        return 0;
    };

    let display_line = find_display_line(diff, thread.selection_start as u32);
    if display_line.is_none() {
        return 0;
    }

    comment_block_height(comments, content_width)
}

pub(crate) fn find_display_line(diff: &ParsedDiff, line: u32) -> Option<usize> {
    let mut old_line_to_display: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    let mut new_line_to_display: std::collections::HashMap<u32, usize> =
        std::collections::HashMap::new();
    let mut display_idx = 0;

    for hunk in &diff.hunks {
        display_idx += 1;
        for diff_line in &hunk.lines {
            if let Some(old_ln) = diff_line.old_line {
                old_line_to_display.insert(old_ln, display_idx);
            }
            if let Some(new_ln) = diff_line.new_line {
                new_line_to_display.insert(new_ln, display_idx);
            }
            display_idx += 1;
        }
    }

    new_line_to_display
        .get(&line)
        .or_else(|| old_line_to_display.get(&line))
        .copied()
}

fn context_line_index(
    total_lines: usize,
    threads: &[ThreadSummary],
    file_path: &str,
    line_num: i64,
) -> Option<usize> {
    let mut ranges = Vec::new();
    for thread in threads.iter().filter(|t| t.file_path == file_path) {
        let thread_end = thread.selection_end.unwrap_or(thread.selection_start);
        let start = (thread.selection_start - 5).max(1);
        let end = (thread_end + 5).min(total_lines as i64);
        ranges.push((start, end));
    }

    if ranges.is_empty() {
        return None;
    }

    ranges.sort_by_key(|r| r.0);
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 + 1 {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        } else {
            merged.push((start, end));
        }
    }

    let mut index = 0usize;
    let mut prev_end: Option<i64> = None;
    for (start, end) in merged {
        if let Some(pe) = prev_end {
            if start > pe + 1 {
                index += 1; // separator line
            }
        }

        if line_num >= start && line_num <= end {
            let offset = (line_num - start) as usize;
            return Some(index + offset);
        }

        index += (end - start + 1) as usize;
        prev_end = Some(end);
    }

    None
}

fn comment_block_height(comments: &[Comment], content_width: u32) -> usize {
    if comments.is_empty() {
        return 0;
    }
    let max_width = content_width
        .saturating_sub((BLOCK_SIDE_MARGIN * 2 + 1 + BLOCK_LEFT_PAD + BLOCK_RIGHT_PAD) as u32);
    let max_width = max_width as usize;
    let mut content_lines = 2; // thread header line + spacing
    for comment in comments {
        content_lines += 1; // author line
        let wrapped = wrap_text(&comment.body, max_width);
        content_lines += wrapped.len();
    }
    block_height(content_lines)
}

fn context_display_count(total_lines: usize, threads: &[ThreadSummary], file_path: &str) -> usize {
    let mut ranges = Vec::new();
    for thread in threads.iter().filter(|t| t.file_path == file_path) {
        let thread_end = thread.selection_end.unwrap_or(thread.selection_start);
        let start = (thread.selection_start - 5).max(1);
        let end = (thread_end + 5).min(total_lines as i64);
        ranges.push((start, end));
    }

    if ranges.is_empty() {
        return 1; // "No threads" line
    }

    ranges.sort_by_key(|r| r.0);
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 + 1 {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        } else {
            merged.push((start, end));
        }
    }

    let mut count = 0usize;
    let mut prev_end: Option<i64> = None;
    for (start, end) in merged {
        if let Some(pe) = prev_end {
            if start > pe + 1 {
                count += 1; // separator line
            }
        }
        count += (end - start + 1) as usize;
        prev_end = Some(end);
    }

    count
}

fn expanded_context_extra_lines(
    total_lines: usize,
    threads: &[ThreadSummary],
    expanded_thread: Option<&str>,
    comments: &[Comment],
    content_width: u32,
) -> usize {
    let Some(thread_id) = expanded_thread else {
        return 0;
    };
    let Some(thread) = threads.iter().find(|t| t.thread_id == thread_id) else {
        return 0;
    };
    if thread.selection_start <= 0 || thread.selection_start as usize > total_lines {
        return 0;
    }
    comment_block_height(comments, content_width)
}
