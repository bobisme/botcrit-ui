//! Comment block rendering (thread comment bubbles in the diff stream).

use opentui::Style;

use crate::db::ThreadSummary;
use crate::layout::{BLOCK_MARGIN, BLOCK_PADDING};
use crate::text::wrap_text;
use crate::view::components::Rect;

use super::helpers::{comment_block_area, comment_content_area, draw_comment_bar, draw_plain_line_with_right};
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

pub(super) fn emit_comment_block(
    cursor: &mut StreamCursor<'_>,
    area: Rect,
    thread: &ThreadSummary,
    comments: &[crate::db::Comment],
) {
    if comments.is_empty() {
        return;
    }

    // Layout: area → block (margined) → padded content
    let block = comment_block_area(area);
    let padded = comment_content_area(block);
    let content_width = padded.width as usize;
    let mut content_lines: Vec<CommentLine> = Vec::new();

    let line_range = if let Some(end) = thread.selection_end {
        format!("{}-{}", thread.selection_start, end)
    } else {
        format!("{}", thread.selection_start)
    };
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

    let top_margin = 0usize;
    let bottom_margin = BLOCK_MARGIN;
    let total_rows = content_lines
        .len()
        .saturating_add(BLOCK_PADDING * 2)
        .saturating_add(top_margin)
        .saturating_add(bottom_margin);
    let mut content_idx = 0usize;

    for row in 0..total_rows {
        cursor.emit(|buf, y, theme| {
            if row < top_margin {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            } else if row < top_margin + BLOCK_PADDING {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
                buf.fill_rect(block.x, y, block.width, 1, theme.panel_bg);
                draw_comment_bar(buf, block.x, y, theme.panel_bg, theme);
            } else if row < top_margin + BLOCK_PADDING + content_lines.len() {
                let line = &content_lines[content_idx];
                let (left_style, right_style) = match line.kind {
                    CommentLineKind::Header => (Style::fg(theme.muted), Style::fg(theme.muted)),
                    CommentLineKind::Author => (Style::fg(theme.primary), Style::fg(theme.muted)),
                    CommentLineKind::Body => (Style::fg(theme.foreground), Style::fg(theme.muted)),
                };
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
                buf.fill_rect(block.x, y, block.width, 1, theme.panel_bg);
                draw_comment_bar(buf, block.x, y, theme.panel_bg, theme);
                draw_plain_line_with_right(
                    buf,
                    padded,
                    y,
                    theme.panel_bg,
                    &line.left,
                    line.right.as_deref(),
                    left_style,
                    right_style,
                );
                content_idx += 1;
            } else if row < top_margin + BLOCK_PADDING + content_lines.len() + BLOCK_PADDING {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
                buf.fill_rect(block.x, y, block.width, 1, theme.panel_bg);
                draw_comment_bar(buf, block.x, y, theme.panel_bg, theme);
            } else {
                buf.fill_rect(area.x, y, area.width, 1, theme.background);
            }
        });
    }
}
