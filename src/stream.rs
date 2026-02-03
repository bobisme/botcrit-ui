//! Stream layout helpers for the right pane

use std::collections::HashMap;

use crate::db::{Comment, ThreadSummary};
use crate::diff::ParsedDiff;
use crate::model::{DiffViewMode, FileCacheEntry, FileEntry};
use crate::text::{wrap_text, wrap_text_preserve};

pub const BLOCK_MARGIN: usize = 1;
pub const BLOCK_PADDING: usize = 1;
pub const BLOCK_SIDE_MARGIN: u32 = 2;
pub const BLOCK_LEFT_PAD: u32 = 2;
pub const BLOCK_RIGHT_PAD: u32 = 2;
pub const SIDE_BY_SIDE_MIN_WIDTH: u32 = 100;

/// Horizontal padding for diff lines (must match DIFF_H_PAD in diff.rs).
const DIFF_H_PAD: u32 = 0;
const ORPHANED_CONTEXT_LEFT_PAD: u32 = 2;

pub struct StreamLayout {
    pub file_offsets: Vec<usize>,
    pub total_lines: usize,
}

pub fn block_height(content_lines: usize) -> usize {
    content_lines + (BLOCK_MARGIN * 2) + (BLOCK_PADDING * 2)
}

/// Inner width for diff content (no block bar/margins, just horizontal padding).
fn diff_inner_width(pane_width: u32) -> u32 {
    pane_width.saturating_sub(DIFF_H_PAD * 2)
}

fn unified_wrap_width(pane_width: u32) -> usize {
    let thread_col_width: u32 = 2;
    let line_num_width: u32 = 12;
    let content_width =
        diff_inner_width(pane_width).saturating_sub(thread_col_width + line_num_width);
    let max_content = content_width.saturating_sub(2);
    max_content as usize
}

fn context_wrap_width(pane_width: u32) -> usize {
    let line_num_width: u32 = 6;
    diff_inner_width(pane_width).saturating_sub(line_num_width) as usize
}

fn orphaned_context_wrap_width(pane_width: u32) -> usize {
    let line_num_width: u32 = 6;
    diff_inner_width(pane_width)
        .saturating_sub(ORPHANED_CONTEXT_LEFT_PAD)
        .saturating_sub(line_num_width) as usize
}

fn side_by_side_wrap_widths(pane_width: u32) -> (usize, usize) {
    let thread_col_width: u32 = 2;
    let divider_width: u32 = 0;
    let line_num_width: u32 = 6;
    let available = diff_inner_width(pane_width).saturating_sub(thread_col_width + divider_width);
    let half_width = available / 2;
    let left = half_width.saturating_sub(line_num_width) as usize;
    let right = half_width.saturating_sub(line_num_width) as usize;
    (left, right)
}

fn wrap_line_count(text: &str, max_width: usize) -> usize {
    if max_width == 0 {
        return 1;
    }
    let lines = wrap_text_preserve(text, max_width);
    lines.len().max(1)
}

pub fn compute_stream_layout(
    files: &[FileEntry],
    file_cache: &HashMap<String, FileCacheEntry>,
    threads: &[ThreadSummary],
    all_comments: &HashMap<String, Vec<Comment>>,
    view_mode: DiffViewMode,
    wrap: bool,
    content_width: u32,
) -> StreamLayout {
    let mut file_offsets = Vec::with_capacity(files.len());
    let mut total = 0usize;

    for file in files {
        file_offsets.push(total);
        total += block_height(1); // file header block

        if let Some(entry) = file_cache.get(&file.path) {
            let file_threads: Vec<&ThreadSummary> = threads
                .iter()
                .filter(|t| t.file_path == file.path)
                .collect();
            let diff_lines = if let Some(diff) = &entry.diff {
                let anchors = crate::view::map_threads_to_diff(diff, &file_threads);
                let anchored_ids: std::collections::HashSet<&str> =
                    anchors.iter().map(|a| a.thread_id.as_str()).collect();
                let anchored_threads: Vec<&ThreadSummary> = file_threads
                    .iter()
                    .filter(|t| anchored_ids.contains(t.thread_id.as_str()))
                    .copied()
                    .collect();
                let orphaned_threads: Vec<&ThreadSummary> = file_threads
                    .iter()
                    .filter(|t| !anchored_ids.contains(t.thread_id.as_str()))
                    .copied()
                    .collect();

                let mut count = diff_line_count_for_view(diff, view_mode, wrap, content_width)
                    + threads_comment_height(&anchored_threads, all_comments, content_width);

                if !orphaned_threads.is_empty() {
                    if let Some(content) = &entry.file_content {
                        let hunk_ranges =
                            crate::diff::hunk_exclusion_ranges(&diff.hunks);
                        count += orphaned_context_display_count(
                            content.lines.as_slice(),
                            &orphaned_threads,
                            &hunk_ranges,
                            wrap,
                            content_width,
                        );
                    }
                    count += threads_comment_height(&orphaned_threads, all_comments, content_width);
                }

                count
            } else if let Some(content) = &entry.file_content {
                context_display_count(
                    content.lines.as_slice(),
                    threads,
                    &file.path,
                    wrap,
                    content_width,
                ) + all_context_extra_lines(
                    content.lines.len(),
                    &file_threads,
                    all_comments,
                    content_width,
                )
            } else {
                0
            };

            total += diff_lines.max(1);
        } else {
            total += 1;
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

fn diff_line_count_for_view(
    diff: &ParsedDiff,
    view_mode: DiffViewMode,
    wrap: bool,
    content_width: u32,
) -> usize {
    match view_mode {
        DiffViewMode::Unified => {
            if wrap {
                diff_line_count_wrapped(diff, unified_wrap_width(content_width))
            } else {
                diff_line_count(diff)
            }
        }
        DiffViewMode::SideBySide => {
            if wrap {
                let (left_width, right_width) = side_by_side_wrap_widths(content_width);
                side_by_side_line_count_wrapped(diff, left_width, right_width)
            } else {
                side_by_side_line_count(diff)
            }
        }
    }
}

fn diff_line_count(diff: &ParsedDiff) -> usize {
    diff.hunks.iter().map(|h| 1 + h.lines.len()).sum()
}

fn diff_line_count_wrapped(diff: &ParsedDiff, max_width: usize) -> usize {
    let mut count = 0usize;
    for hunk in &diff.hunks {
        count += 1;
        for line in &hunk.lines {
            count += wrap_line_count(&line.content, max_width);
        }
    }
    count
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

fn side_by_side_line_count_wrapped(
    diff: &ParsedDiff,
    left_width: usize,
    right_width: usize,
) -> usize {
    let mut count = 0usize;
    for hunk in &diff.hunks {
        count += 1;
        let mut i = 0;
        let lines = &hunk.lines;
        while i < lines.len() {
            match lines[i].kind {
                crate::diff::DiffLineKind::Context => {
                    count += wrap_line_count(&lines[i].content, left_width);
                    i += 1;
                }
                crate::diff::DiffLineKind::Removed => {
                    let mut removals = Vec::new();
                    while i < lines.len() && lines[i].kind == crate::diff::DiffLineKind::Removed {
                        removals.push(&lines[i]);
                        i += 1;
                    }
                    let mut additions = Vec::new();
                    while i < lines.len() && lines[i].kind == crate::diff::DiffLineKind::Added {
                        additions.push(&lines[i]);
                        i += 1;
                    }
                    let max_len = removals.len().max(additions.len());
                    for idx in 0..max_len {
                        let left_lines = removals
                            .get(idx)
                            .map(|line| wrap_line_count(&line.content, left_width))
                            .unwrap_or(1);
                        let right_lines = additions
                            .get(idx)
                            .map(|line| wrap_line_count(&line.content, right_width))
                            .unwrap_or(1);
                        count += left_lines.max(right_lines);
                    }
                }
                crate::diff::DiffLineKind::Added => {
                    count += wrap_line_count(&lines[i].content, right_width);
                    i += 1;
                }
            }
        }
    }
    count
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
    block_height(content_lines).saturating_sub(BLOCK_MARGIN)
}

fn context_display_count(
    lines: &[String],
    threads: &[ThreadSummary],
    file_path: &str,
    wrap: bool,
    content_width: u32,
) -> usize {
    let mut ranges = Vec::new();
    let total_lines = lines.len();
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
        let max_width = context_wrap_width(content_width);
        for line in start..=end {
            if let Some(text) = lines.get((line - 1) as usize) {
                if wrap {
                    count += wrap_line_count(text, max_width);
                } else {
                    count += 1;
                }
            } else {
                count += 1;
            }
        }
        prev_end = Some(end);
    }

    count
}

/// Count display lines for orphaned thread context (already-filtered threads).
/// `exclude_ranges` are (start, end) pairs of new-file lines already shown in the diff.
fn orphaned_context_display_count(
    lines: &[String],
    orphaned_threads: &[&ThreadSummary],
    exclude_ranges: &[(i64, i64)],
    wrap: bool,
    content_width: u32,
) -> usize {
    let total_lines = lines.len();
    let mut ranges: Vec<(i64, i64)> = orphaned_threads
        .iter()
        .map(|t| {
            let thread_end = t.selection_end.unwrap_or(t.selection_start);
            let start = (t.selection_start - 5).max(1);
            let end = (thread_end + 5).min(total_lines as i64);
            (start, end)
        })
        .collect();

    if ranges.is_empty() {
        return 0;
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

    // Clip against diff hunk ranges
    if !exclude_ranges.is_empty() {
        let mut clipped: Vec<(i64, i64)> = Vec::new();
        for (rs, re) in &merged {
            let mut remaining = vec![(*rs, *re)];
            for &(ex_start, ex_end) in exclude_ranges {
                let mut next = Vec::new();
                for (s, e) in remaining {
                    if e < ex_start || s > ex_end {
                        next.push((s, e));
                    } else {
                        if s < ex_start {
                            next.push((s, ex_start - 1));
                        }
                        if e > ex_end {
                            next.push((ex_end + 1, e));
                        }
                    }
                }
                remaining = next;
            }
            clipped.extend(remaining);
        }
        clipped.sort_by_key(|r| r.0);
        merged = clipped;
    }

    let mut count = 0usize;
    let mut prev_end: Option<i64> = None;
    for (start, end) in merged {
        if let Some(pe) = prev_end {
            if start > pe + 1 {
                count += 1; // separator line
            }
        }
        let max_width = context_wrap_width(content_width);
        for line in start..=end {
            if let Some(text) = lines.get((line - 1) as usize) {
                if wrap {
                    count += wrap_line_count(text, max_width);
                } else {
                    count += 1;
                }
            } else {
                count += 1;
            }
        }
        prev_end = Some(end);
    }

    count
}

/// Count comment block heights for a set of threads.
fn threads_comment_height(
    threads: &[&ThreadSummary],
    all_comments: &HashMap<String, Vec<Comment>>,
    content_width: u32,
) -> usize {
    let mut total = 0;
    for thread in threads {
        if let Some(comments) = all_comments.get(&thread.thread_id) {
            total += comment_block_height(comments, content_width);
        }
    }
    total
}

fn all_context_extra_lines(
    total_lines: usize,
    file_threads: &[&ThreadSummary],
    all_comments: &HashMap<String, Vec<Comment>>,
    content_width: u32,
) -> usize {
    let mut total = 0;
    for thread in file_threads {
        if thread.selection_start <= 0 || thread.selection_start as usize > total_lines {
            continue;
        }
        if let Some(comments) = all_comments.get(&thread.thread_id) {
            total += comment_block_height(comments, content_width);
        }
    }
    total
}
