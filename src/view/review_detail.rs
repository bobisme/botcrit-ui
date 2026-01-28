//! Review detail screen rendering

use opentui::{OptimizedBuffer, Style};

use super::components::{draw_box, draw_text_truncated, truncate_path, Rect};
use super::diff::{
    map_threads_to_diff, render_diff_side_by_side, render_diff_with_threads, render_file_context,
    SIDE_BY_SIDE_MIN_WIDTH,
};
use crate::model::{DiffViewMode, Focus, LayoutMode, Model};

/// Render the review detail screen
pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    let theme = &model.theme;
    let area = Rect::from_size(model.width, model.height);

    // Get review title for the main box
    let title = model
        .current_review
        .as_ref()
        .map_or("Review", |r| r.title.as_str());

    let review_id = model
        .current_review
        .as_ref()
        .map_or("", |r| r.review_id.as_str());

    let full_title = format!("{review_id}: {title}");

    // Main outer box
    draw_box(
        buffer,
        area,
        theme.border,
        Some(&full_title),
        theme.foreground,
    );

    let inner = area.inner();

    // Layout based on mode
    match model.layout_mode {
        LayoutMode::Full | LayoutMode::Compact => {
            let sidebar_width = model.layout_mode.sidebar_width() as u32;
            let (sidebar_area, diff_area) = inner.split_left(sidebar_width);

            draw_file_sidebar(model, buffer, sidebar_area);
            draw_diff_pane(model, buffer, diff_area);
        }
        LayoutMode::Overlay => {
            // Draw diff pane full width, overlay sidebar if visible
            draw_diff_pane(model, buffer, inner);
            if model.sidebar_visible {
                let sidebar_width = 20;
                let sidebar_area = Rect::new(inner.x, inner.y, sidebar_width, inner.height);
                // Draw with panel background to overlay
                buffer.fill_rect(
                    sidebar_area.x,
                    sidebar_area.y,
                    sidebar_area.width,
                    sidebar_area.height,
                    theme.panel_bg,
                );
                draw_file_sidebar(model, buffer, sidebar_area);
            }
        }
        LayoutMode::Single => {
            // Show either sidebar or diff based on focus
            if matches!(model.focus, Focus::FileSidebar) {
                draw_file_sidebar(model, buffer, inner);
            } else {
                draw_diff_pane(model, buffer, inner);
            }
        }
    }

    // Help bar at bottom
    draw_help_bar(model, buffer, area);
}

fn draw_file_sidebar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let focused = matches!(model.focus, Focus::FileSidebar);

    // Box around sidebar
    let border_color = if focused {
        theme.border_focused
    } else {
        theme.border
    };

    draw_box(buffer, area, border_color, Some("Files"), theme.foreground);

    let inner = area.inner();
    let files = model.files_with_threads();

    if files.is_empty() {
        buffer.draw_text(inner.x, inner.y, "No files", Style::fg(theme.muted));
        return;
    }

    for (i, file) in files.iter().enumerate() {
        if i >= inner.height as usize {
            break;
        }

        let y = inner.y + i as u32;
        let selected = i == model.file_index;

        // Selection indicator and background
        let (prefix, style) = if selected {
            buffer.fill_rect(inner.x, y, inner.width, 1, theme.selection_bg);
            (
                "▸ ",
                Style::fg(theme.selection_fg).with_bg(theme.selection_bg),
            )
        } else {
            ("  ", Style::fg(theme.foreground))
        };

        buffer.draw_text(inner.x, y, prefix, style);

        // Thread count indicator
        let thread_indicator = if file.open_threads > 0 {
            format!("{}", file.open_threads)
        } else if file.resolved_threads > 0 {
            "✓".to_string()
        } else {
            " ".to_string()
        };

        let indicator_color = if file.open_threads > 0 {
            theme.warning
        } else {
            theme.success
        };

        // Calculate available width for filename
        let indicator_width: u32 = 2;
        let prefix_width: u32 = 2;
        let filename_width = inner
            .width
            .saturating_sub(prefix_width + indicator_width + 1);

        // Draw filename (truncated)
        let filename = truncate_path(&file.path, filename_width as usize);
        draw_text_truncated(
            buffer,
            inner.x + prefix_width,
            y,
            &filename,
            filename_width,
            style,
        );

        // Draw thread indicator at the end
        buffer.draw_text(
            inner.x + inner.width - indicator_width,
            y,
            &thread_indicator,
            Style::fg(indicator_color),
        );
    }
}

fn draw_diff_pane(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let focused = matches!(model.focus, Focus::DiffPane | Focus::ThreadExpanded);

    // Box around diff pane
    let border_color = if focused {
        theme.border_focused
    } else {
        theme.border
    };

    // Get current file name for title
    let files = model.files_with_threads();
    let file_title = files
        .get(model.file_index)
        .map_or("No file selected", |f| f.path.as_str());

    draw_box(
        buffer,
        area,
        border_color,
        Some(file_title),
        theme.foreground,
    );

    let inner = area.inner();

    // Get threads for current file
    let threads = model.threads_for_current_file();

    // Render diff if available
    if let Some(diff) = &model.current_diff {
        // Map threads to diff display lines
        let anchors = map_threads_to_diff(diff, &threads, model.expanded_thread.as_deref());

        // Choose side-by-side or unified based on width and preference
        let use_side_by_side = model.diff_view_mode == DiffViewMode::SideBySide
            && inner.width >= SIDE_BY_SIDE_MIN_WIDTH;

        if use_side_by_side {
            render_diff_side_by_side(
                buffer,
                inner,
                diff,
                model.diff_scroll,
                theme,
                &anchors,
                &model.comments,
                &model.highlighted_lines,
            );
        } else {
            render_diff_with_threads(
                buffer,
                inner,
                diff,
                model.diff_scroll,
                theme,
                &anchors,
                &model.comments,
                &model.highlighted_lines,
            );
        }
        return;
    }

    // Render file content if available (for files without diffs)
    if let Some(file_content) = &model.current_file_content {
        render_file_context(
            buffer,
            inner,
            &file_content.lines,
            model.diff_scroll,
            theme,
            &threads,
            model.expanded_thread.as_deref(),
            &model.comments,
        );
        return;
    }

    // Fallback: show threads as a simple list when no diff or file content

    if threads.is_empty() {
        buffer.draw_text(
            inner.x + 2,
            inner.y + 1,
            "No content available",
            Style::fg(theme.muted),
        );
        return;
    }

    // Show threads as a simple list when no diff is loaded
    buffer.draw_text(
        inner.x + 2,
        inner.y,
        &format!("{} thread(s) - no diff loaded", threads.len()),
        Style::fg(theme.muted),
    );

    let mut row = 2u32;
    for thread in threads.iter() {
        if row >= inner.height {
            break;
        }

        let y = inner.y + row;
        let is_expanded = model
            .expanded_thread
            .as_ref()
            .is_some_and(|id| id == &thread.thread_id);

        // Line range
        let line_range = if let Some(end) = thread.selection_end {
            format!("L{}-{}", thread.selection_start, end)
        } else {
            format!("L{}", thread.selection_start)
        };

        // Status indicator
        let (status_char, status_color) = if thread.status == "resolved" {
            ("✓", theme.success)
        } else {
            ("●", theme.warning)
        };

        let prefix = if is_expanded { "▼ " } else { "▸ " };

        buffer.draw_text(inner.x + 2, y, prefix, Style::fg(theme.foreground));
        buffer.draw_text(inner.x + 4, y, status_char, Style::fg(status_color));
        buffer.draw_text(inner.x + 6, y, &line_range, Style::fg(theme.primary));
        buffer.draw_text(
            inner.x + 16,
            y,
            &format!("({} comments)", thread.comment_count),
            Style::fg(theme.muted),
        );

        row += 1;

        // If expanded, show comments inline
        if is_expanded && !model.comments.is_empty() {
            for comment in &model.comments {
                if row >= inner.height {
                    break;
                }

                // Author
                buffer.draw_text(
                    inner.x + 6,
                    inner.y + row,
                    &format!("  {} ", comment.author),
                    Style::fg(theme.primary),
                );
                row += 1;

                if row >= inner.height {
                    break;
                }

                // Comment body (first line, truncated)
                let body_line = comment.body.lines().next().unwrap_or("");
                let max_len = inner.width.saturating_sub(10) as usize;
                let display_body = if body_line.len() > max_len {
                    format!("{}...", &body_line[..max_len.saturating_sub(3)])
                } else {
                    body_line.to_string()
                };
                buffer.draw_text(
                    inner.x + 8,
                    inner.y + row,
                    &display_body,
                    Style::fg(theme.foreground),
                );
                row += 1;
            }
            // Add spacing after comments
            row += 1;
        }
    }
}

fn draw_help_bar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let y = area.y + area.height - 1;

    // Help text based on focus
    let help = match model.focus {
        Focus::FileSidebar => "j/k files  Enter/Space diff  h back  q quit",
        Focus::DiffPane => "j/k scroll  n/p thread  v view  Space files  Esc back",
        Focus::ThreadExpanded => "j/k scroll  r resolve  Esc collapse",
        _ => "Space switch  Esc back  q quit",
    };

    buffer.draw_text(area.x + 2, y, help, Style::fg(theme.muted));
}
