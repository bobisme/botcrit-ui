//! Inline multi-line comment editor overlay.
//!
//! Renders a modal overlay similar to the command palette:
//! - Dimmed background
//! - Centered panel with title, existing comments, text area, and status bar

use opentui::{OptimizedBuffer, Style};

use crate::model::{Focus, InlineEditor, Model};
use crate::theme::Theme;
use crate::view::components::{dim_rect, draw_text_truncated, Rect};

/// Minimum editor panel height (title + padding + 3 text lines + status).
const MIN_HEIGHT: u32 = 8;
/// Horizontal padding inside the panel.
const H_PAD: u32 = 2;

pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    if model.focus != Focus::Commenting {
        return;
    }
    let Some(editor) = &model.inline_editor else {
        return;
    };

    let screen = Rect::from_size(model.width, model.height);
    dim_rect(buffer, screen, 0.35);

    let panel = compute_panel(screen, editor);

    // Fill panel background
    buffer.fill_rect(panel.x, panel.y, panel.width, panel.height, model.theme.panel_bg);

    let content_x = panel.x + H_PAD;
    let content_width = panel.width.saturating_sub(H_PAD * 2);

    let mut y = panel.y;

    // --- Title row ---
    y = render_title(buffer, &model.theme, editor, &panel, content_x, content_width, y);

    // --- Blank row ---
    y += 1;

    // --- Existing comments context (dimmed) ---
    y = render_existing_comments(buffer, &model.theme, editor, &panel, content_x, content_width, y);

    // --- Text area ---
    let status_y = panel.y + panel.height - 2;
    render_text_area(buffer, &model.theme, editor, content_x, content_width, y, status_y);

    // --- Status bar ---
    render_status_bar(buffer, &model.theme, &panel, content_x, status_y);
}

fn compute_panel(screen: Rect, editor: &InlineEditor) -> Rect {
    let panel_width = (screen.width * 7 / 10).clamp(40, 80).min(screen.width.saturating_sub(4));
    let panel_x = (screen.width.saturating_sub(panel_width)) / 2;

    let existing_count = editor.request.existing_comments.len() as u32;
    let context_rows = if existing_count > 0 {
        1 + existing_count.min(6) + 1
    } else {
        0
    };
    let text_area_height = 8u32;
    let ideal_height = 1 + 1 + context_rows + text_area_height + 1 + 1 + 1;
    let panel_height = ideal_height
        .clamp(MIN_HEIGHT, screen.height.saturating_sub(4))
        .min(screen.height);
    let panel_y = (screen.height.saturating_sub(panel_height)) / 3;

    Rect::new(panel_x, panel_y, panel_width, panel_height)
}

fn render_title(
    buffer: &mut OptimizedBuffer,
    theme: &Theme,
    editor: &InlineEditor,
    panel: &Rect,
    content_x: u32,
    content_width: u32,
    y: u32,
) -> u32 {
    let line_range = match editor.request.end_line {
        Some(end) if end != editor.request.start_line => {
            format!("{}:{}-{}", editor.request.file_path, editor.request.start_line, end)
        }
        _ => format!("{}:{}", editor.request.file_path, editor.request.start_line),
    };
    let title = if editor.request.thread_id.is_some() {
        format!("Reply on {line_range}")
    } else {
        format!("New comment on {line_range}")
    };
    draw_text_truncated(
        buffer,
        content_x,
        y,
        &title,
        content_width.saturating_sub(4),
        theme.style_foreground().with_bold(),
    );
    let esc_x = panel.x + panel.width - H_PAD - 3;
    buffer.draw_text(esc_x, y, "esc", theme.style_muted());
    y + 1
}

fn render_existing_comments(
    buffer: &mut OptimizedBuffer,
    theme: &Theme,
    editor: &InlineEditor,
    panel: &Rect,
    content_x: u32,
    content_width: u32,
    mut y: u32,
) -> u32 {
    if editor.request.existing_comments.is_empty() {
        return y;
    }
    let existing_count = editor.request.existing_comments.len() as u32;
    let max_comments = 6u32.min(existing_count);
    let skip = existing_count.saturating_sub(max_comments) as usize;
    for comment in editor.request.existing_comments.iter().skip(skip) {
        if y >= panel.y + panel.height - 3 {
            break;
        }
        let text = format!("{}: {}", comment.author, comment.body);
        draw_text_truncated(buffer, content_x, y, &text, content_width, theme.style_muted());
        y += 1;
    }
    y + 1 // blank separator
}

fn render_text_area(
    buffer: &mut OptimizedBuffer,
    theme: &Theme,
    editor: &InlineEditor,
    content_x: u32,
    content_width: u32,
    text_area_top: u32,
    status_y: u32,
) {
    let available_text_rows = status_y.saturating_sub(text_area_top + 1) as usize;
    let bar_style = theme.style_primary();
    let text_style = theme.style_foreground().with_bg(theme.panel_bg);
    let cursor_style = Style::fg(theme.panel_bg).with_bg(theme.foreground);

    // Draw left accent bar
    for row in 0..available_text_rows {
        let line_y = text_area_top + row as u32;
        if line_y >= status_y {
            break;
        }
        buffer.draw_text(content_x, line_y, "\u{2503}", bar_style);
    }

    let text_x = content_x + 2;
    let text_width = content_width.saturating_sub(2);
    let scroll = editor.scroll;

    for (view_row, line_idx) in (scroll..editor.lines.len())
        .enumerate()
        .take(available_text_rows)
    {
        let line_y = text_area_top + view_row as u32;
        if line_y >= status_y {
            break;
        }
        let line = &editor.lines[line_idx];
        if line_idx == editor.cursor_row {
            render_line_with_cursor(buffer, text_x, line_y, line, editor.cursor_col, text_width, text_style, cursor_style);
        } else {
            draw_text_truncated(buffer, text_x, line_y, line, text_width, text_style);
        }
    }

    // Show cursor on empty first line
    if editor.lines.len() == 1 && editor.lines[0].is_empty() && editor.cursor_col == 0 {
        buffer.draw_text(text_x, text_area_top, " ", cursor_style);
    }
}

fn render_status_bar(
    buffer: &mut OptimizedBuffer,
    theme: &Theme,
    panel: &Rect,
    content_x: u32,
    status_y: u32,
) {
    buffer.fill_rect(panel.x, status_y, panel.width, 1, theme.panel_bg);
    let status_text = "Ctrl+S submit    Esc cancel";
    let status_x = panel.x + panel.width - H_PAD - status_text.len() as u32;
    buffer.draw_text(status_x.max(content_x), status_y, status_text, theme.style_muted());
}

/// Render a line of text with the cursor shown as an inverted-color block.
#[allow(clippy::too_many_arguments)]
fn render_line_with_cursor(
    buffer: &mut OptimizedBuffer,
    x: u32,
    y: u32,
    line: &str,
    cursor_col: usize,
    max_width: u32,
    text_style: Style,
    cursor_style: Style,
) {
    let chars: Vec<char> = line.chars().collect();
    let mut col = 0u32;

    for (i, &ch) in chars.iter().enumerate() {
        if col >= max_width {
            break;
        }
        let style = if i == cursor_col { cursor_style } else { text_style };
        let s = ch.to_string();
        buffer.draw_text(x + col, y, &s, style);
        col += 1;
    }

    // If cursor is at end of line, draw cursor block on the space after
    if cursor_col >= chars.len() && col < max_width {
        buffer.draw_text(x + col, y, " ", cursor_style);
    }
}
