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

    // Header
    buffer.fill_rect(area.x, area.y, area.width, 1, theme.background);
    buffer.draw_text(
        area.x + 2,
        area.y,
        &full_title,
        Style::fg(theme.foreground).with_bold(),
    );

    let inner = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(2),
    );

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

    let mut y = inner.y;
    let text_x = inner.x + 1;
    let text_width = inner.width.saturating_sub(2);

    if let Some(review) = &model.current_review {
        y += 1;
        draw_text_truncated(
            buffer,
            text_x,
            y,
            &review.review_id,
            text_width,
            Style::fg(theme.foreground).with_bold(),
        );
        y += 1;

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
        buffer.draw_text(text_x, y, "No files", Style::fg(theme.muted));
        return;
    }

    for (i, file) in files.iter().enumerate() {
        if y >= inner.y + inner.height {
            break;
        }

        let row_y = y;
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

        buffer.draw_text(inner.x + 1, row_y, prefix, style);

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
            .saturating_sub(prefix_width + indicator_width + 2);

        // Draw filename (truncated)
        let filename = truncate_path(&file.path, filename_width as usize);
        draw_text_truncated(
            buffer,
            inner.x + prefix_width + 1,
            row_y,
            &filename,
            filename_width,
            style,
        );

        // Draw thread indicator at the end
        buffer.draw_text(
            inner.x + inner.width - indicator_width,
            row_y,
            &thread_indicator,
            Style::fg(indicator_color),
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
        inner,
        &files,
        &model.file_cache,
        &model.threads,
        model.expanded_thread.as_deref(),
        &model.comments,
        model.diff_scroll,
        theme,
        model.diff_view_mode,
    );

    let pinned_height = block_height(1) as u32;
    let pinned_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        pinned_height.min(inner.height),
    );
    render_pinned_header_block(buffer, pinned_area, file_title, theme, counts);
}

fn draw_help_bar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let y = area.y + area.height - 1;

    buffer.fill_rect(area.x, y, area.width, 1, theme.background);
    // Help text based on focus
    let help = match model.focus {
        Focus::FileSidebar => "j/k files  Enter/Space diff  s sidebar  h back  q quit",
        Focus::DiffPane => "j/k scroll  n/p thread  v view  s sidebar  Esc back  q quit",
        Focus::ThreadExpanded => "j/k scroll  r resolve  Esc collapse",
        _ => "Space switch  Esc back  q quit",
    };

    buffer.draw_text(area.x + 2, y, help, Style::fg(theme.muted));
}
