//! Comment block rendering (thread comment bubbles in the diff stream).

use crate::db::ThreadSummary;
use crate::layout::{BLOCK_MARGIN, BLOCK_PADDING};
use crate::text::wrap_text;
use crate::view::components::Rect;

use super::helpers::{comment_block_area, comment_content_area, draw_comment_bar, draw_plain_line_with_right, PlainLineContent};
use super::StreamCursor;

#[derive(Clone)]
pub(super) enum CommentLineKind {
    Header,
    Author,
    Body,
}

#[derive(Clone)]
pub(super) struct CommentLine {
    pub left: String,
    pub right: Option<String>,
    pub kind: CommentLineKind,
}

fn build_comment_lines(
    thread: &ThreadSummary,
    comments: &[crate::db::Comment],
    content_width: usize,
) -> Vec<CommentLine> {
    let mut content_lines: Vec<CommentLine> = Vec::new();

    let line_range = thread.selection_end.map_or_else(
        || format!("{}", thread.selection_start),
        |end| format!("{}-{}", thread.selection_start, end),
    );
    let mut right_text = format!("{}:{}", thread.file_path, line_range);
    let right_max = content_width.saturating_sub(thread.thread_id.len().saturating_add(1));
    if right_max > 0 && right_text.len() > right_max {
        right_text = crate::view::components::truncate_path(&right_text, right_max);
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

    content_lines
}

/// Compute the total row height of a comment block (for cursor range checks).
pub(super) fn comment_block_rows(
    thread: &ThreadSummary,
    comments: &[crate::db::Comment],
    area: Rect,
) -> usize {
    if comments.is_empty() {
        return 0;
    }
    let padded = comment_content_area(comment_block_area(area));
    let content_width = padded.width as usize;
    let content_lines = build_comment_lines(thread, comments, content_width);
    let content_start = BLOCK_PADDING;
    let content_end = content_start + content_lines.len();
    content_end
        .saturating_add(BLOCK_PADDING)
        .saturating_add(BLOCK_MARGIN)
}

pub(super) fn emit_comment_block(
    cursor: &mut StreamCursor<'_>,
    area: Rect,
    thread: &ThreadSummary,
    comments: &[crate::db::Comment],
    is_highlighted: bool,
) {
    if comments.is_empty() {
        return;
    }

    // Layout: area → block (margined) → padded content
    let block = comment_block_area(area);
    let padded = comment_content_area(block);
    let content_width = padded.width as usize;
    let content_lines = build_comment_lines(thread, comments, content_width);

    let top_margin = 0usize;
    let bottom_margin = BLOCK_MARGIN;
    let content_start = top_margin + BLOCK_PADDING;
    let content_end = content_start + content_lines.len();
    let total_rows = content_end
        .saturating_add(BLOCK_PADDING)
        .saturating_add(bottom_margin);

    for row in 0..total_rows {
        cursor.emit(|buf, y, theme| {
            let block_bg = if is_highlighted {
                theme.panel_bg.lerp(theme.primary, 0.12)
            } else {
                theme.panel_bg
            };
            if row < top_margin {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            } else if row < content_start {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
                buf.fill_rect(block.x, y, block.width, 1, block_bg);
                draw_comment_bar(buf, block.x, y, block_bg, theme);
            } else if row < content_end {
                let line = &content_lines[row - content_start];
                let (left_style, right_style) = match line.kind {
                    CommentLineKind::Header => (
                        theme.style_muted_on(block_bg),
                        theme.style_muted_on(block_bg),
                    ),
                    CommentLineKind::Author => (
                        theme.style_primary_on(block_bg),
                        theme.style_muted_on(block_bg),
                    ),
                    CommentLineKind::Body => (
                        theme.style_foreground_on(block_bg),
                        theme.style_muted_on(block_bg),
                    ),
                };
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
                buf.fill_rect(block.x, y, block.width, 1, block_bg);
                draw_comment_bar(buf, block.x, y, block_bg, theme);
                draw_plain_line_with_right(
                    buf,
                    padded,
                    y,
                    block_bg,
                    &PlainLineContent {
                        left: &line.left,
                        right: line.right.as_deref(),
                        left_style,
                        right_style,
                    },
                );
            } else if row < content_end + BLOCK_PADDING {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
                buf.fill_rect(block.x, y, block.width, 1, block_bg);
                draw_comment_bar(buf, block.x, y, block_bg, theme);
            } else {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            }
        });
    }
}
