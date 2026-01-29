//! Review detail screen rendering

use opentui::{OptimizedBuffer, Style};

use super::components::{draw_text_truncated, truncate_path, Rect};
use super::diff::{diff_change_counts, render_diff_stream, render_pinned_header_block};
use crate::model::{Focus, LayoutMode, Model};
use crate::stream::block_height;

/// Render the review detail screen
pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    let theme = &model.theme;
    let area = Rect::from_size(model.width, model.height);

    let inner = Rect::new(area.x, area.y, area.width, area.height);

    // Layout based on mode
    match model.layout_mode {
        LayoutMode::Full | LayoutMode::Compact => {
            if model.sidebar_visible {
                let sidebar_width = model.layout_mode.sidebar_width() as u32;
                let (sidebar_area, diff_area) = inner.split_left(sidebar_width);

                draw_file_sidebar(model, buffer, sidebar_area);
                draw_diff_pane(model, buffer, diff_area);
            } else {
                draw_diff_pane(model, buffer, inner);
            }
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
            if matches!(model.focus, Focus::FileSidebar) && model.sidebar_visible {
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
    let inner = area;
    buffer.fill_rect(inner.x, inner.y, inner.width, inner.height, theme.panel_bg);
    let files = model.files_with_threads();
    let focused = matches!(model.focus, Focus::FileSidebar);

    let left_pad: u32 = 2;
    let right_pad: u32 = 2;
    let mut y = inner.y + 1;
    let text_x = inner.x + left_pad;
    let text_width = inner.width.saturating_sub(left_pad + right_pad);
    let bottom = inner.y + inner.height.saturating_sub(1);

    if let Some(review) = &model.current_review {
        draw_text_truncated(
            buffer,
            text_x,
            y,
            &review.review_id,
            text_width,
            Style::fg(theme.foreground).with_bold(),
        );
        y += 2;

        let bookmark = format!("<{}>", review.jj_change_id);
        draw_text_truncated(
            buffer,
            text_x,
            y,
            &bookmark,
            text_width,
            Style::fg(theme.muted),
        );
        y += 1;

        let from = short_hash(&review.initial_commit);
        let to = short_hash(
            review
                .final_commit
                .as_deref()
                .unwrap_or(&review.initial_commit),
        );
        let commit_range = format!("@{} -> @{}", from, to);
        draw_text_truncated(
            buffer,
            text_x,
            y,
            &commit_range,
            text_width,
            Style::fg(theme.muted),
        );
        y += 2;
    }

    if files.is_empty() {
        if y < bottom {
            buffer.draw_text(text_x, y, "No files", Style::fg(theme.muted));
        }
        return;
    }

    for (i, file) in files.iter().enumerate() {
        if y >= bottom {
            break;
        }

        let row_y = y;
        let selected = i == model.file_index;

        // Selection indicator and background
        let row_bg = if selected && focused {
            theme.selection_bg
        } else {
            theme.panel_bg
        };
        if selected && focused {
            buffer.fill_rect(inner.x, y, inner.width, 1, row_bg);
        }
        let (prefix, style) = if selected {
            ("◉ ", Style::fg(theme.primary).with_bg(row_bg))
        } else {
            ("  ", Style::fg(theme.foreground).with_bg(row_bg))
        };

        let prefix_x = inner.x + left_pad;
        buffer.draw_text(prefix_x, row_y, prefix, style);

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
        let indicator_len = thread_indicator.chars().count() as u32;
        let prefix_width: u32 = 2;
        let filename_width = inner
            .width
            .saturating_sub(prefix_width + indicator_len + left_pad + right_pad);

        // Draw filename (truncated)
        let filename = truncate_path(&file.path, filename_width as usize);
        draw_text_truncated(
            buffer,
            prefix_x + prefix_width,
            row_y,
            &filename,
            filename_width,
            style,
        );

        // Draw thread indicator at the end
        let indicator_x = inner
            .x
            .saturating_add(inner.width)
            .saturating_sub(right_pad + indicator_len);
        buffer.draw_text(
            indicator_x,
            row_y,
            &thread_indicator,
            Style::fg(indicator_color).with_bg(row_bg),
        );

        y += 1;
    }
}

fn short_hash(hash: &str) -> &str {
    let len = hash.len();
    let end = len.min(8);
    &hash[..end]
}

fn draw_diff_pane(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let inner = area;
    let content_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(2),
    );

    let files = model.files_with_threads();
    if files.is_empty() {
        buffer.draw_text(
            inner.x + 2,
            inner.y + 1,
            "No content available",
            Style::fg(theme.muted),
        );
        return;
    }

    let file_title = files
        .get(model.file_index)
        .map_or("No file selected", |f| f.path.as_str());

    let counts = files
        .get(model.file_index)
        .and_then(|file| model.file_cache.get(&file.path))
        .and_then(|entry| entry.diff.as_ref())
        .map(diff_change_counts);

    render_diff_stream(
        buffer,
        content_area,
        &files,
        &model.file_cache,
        &model.threads,
        model.expanded_thread.as_deref(),
        &model.comments,
        model.diff_scroll,
        theme,
        model.diff_view_mode,
        model.diff_wrap,
    );

    let pinned_height = block_height(1) as u32;
    let pinned_area = Rect::new(
        content_area.x,
        content_area.y,
        content_area.width,
        pinned_height.min(content_area.height),
    );
    render_pinned_header_block(buffer, pinned_area, file_title, theme, counts);

    // Bottom margin between content and footer
    if inner.height >= 2 {
        let margin_y = inner.y + inner.height - 2;
        buffer.fill_rect(inner.x, margin_y, inner.width, 1, theme.background);
    }
}

fn draw_help_bar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let y = area.y + area.height - 1;

    let mut footer_x = area.x;
    let mut footer_width = area.width;
    if model.sidebar_visible {
        let sidebar_width = model.layout_mode.sidebar_width() as u32;
        if sidebar_width < area.width
            && matches!(
                model.layout_mode,
                LayoutMode::Full | LayoutMode::Compact | LayoutMode::Overlay
            )
        {
            footer_x = area.x + sidebar_width;
            footer_width = area.width.saturating_sub(sidebar_width);
        }
    }

    if footer_width == 0 {
        return;
    }

    buffer.fill_rect(footer_x, y, footer_width, 1, theme.background);
    // Help text based on focus
    let help = match model.focus {
        Focus::FileSidebar => "j/k files  Enter/Space diff  s sidebar  h back  q quit",
        Focus::DiffPane => "j/k scroll  n/p thread  v view  w wrap  s sidebar  Esc back  q quit",
        Focus::ThreadExpanded => "j/k scroll  r resolve  Esc collapse",
        _ => "Space switch  Esc back  q quit",
    };

    let padding: u32 = 2;
    let text_width = help.len() as u32;
    let x = if footer_width > text_width + padding {
        footer_x + footer_width - text_width - padding
    } else {
        footer_x + padding.min(footer_width)
    };
    buffer.draw_text(x, y, help, Style::fg(theme.muted));
}
