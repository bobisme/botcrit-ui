//! Review detail screen rendering

use opentui::{OptimizedBuffer, Style};

use super::components::{dim_rect, draw_help_bar, draw_text_truncated, truncate_path, HotkeyHint, Rect};
use super::diff::{diff_change_counts, render_diff_stream, render_pinned_header_block, DiffStreamParams};
use crate::model::{Focus, LayoutMode, Model, SidebarItem};
use crate::layout::{BLOCK_MARGIN, BLOCK_PADDING, DIFF_MARGIN};
use crate::stream::{block_height, description_block_height};

/// Render the review detail screen
pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    let area = Rect::from_size(model.width, model.height);

    let inner = Rect::new(area.x, area.y, area.width, area.height);

    if model.current_review.is_none() {
        draw_loading_splash(model, buffer, inner);
        render_help_bar(model, buffer, area);
        return;
    }

    // Layout based on mode
    match model.layout_mode {
        LayoutMode::Full | LayoutMode::Compact | LayoutMode::Overlay => {
            if model.sidebar_visible {
                let sidebar_width = u32::from(model.layout_mode.sidebar_width());
                let (sidebar_area, diff_area) = inner.split_left(sidebar_width);

                draw_file_sidebar(model, buffer, sidebar_area);
                draw_diff_pane(model, buffer, diff_area);
            } else {
                draw_diff_pane(model, buffer, inner);
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
    render_help_bar(model, buffer, area);
}

fn draw_loading_splash(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    buffer.fill_rect(area.x, area.y, area.width, area.height, theme.background);

    let title = "Loading review...";
    let title_width = title.len() as u32;
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(title_width) / 2);
    let y = area.y.saturating_add(area.height / 2);
    buffer.draw_text(x, y, title, Style::fg(theme.foreground).with_bold());
}

fn draw_file_sidebar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    let inner = area;
    buffer.fill_rect(inner.x, inner.y, inner.width, inner.height, theme.panel_bg);
    let items = model.sidebar_items();
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
            theme.style_muted(),
        );
        y += 1;

        let from = short_hash(&review.initial_commit);
        let to = short_hash(
            review
                .final_commit
                .as_deref()
                .unwrap_or(&review.initial_commit),
        );
        let commit_range = format!("@{from} -> @{to}");
        draw_text_truncated(
            buffer,
            text_x,
            y,
            &commit_range,
            text_width,
            theme.style_muted(),
        );
        y += 2;
    }

    if items.is_empty() {
        if y < bottom {
            buffer.draw_text(text_x, y, "No files", theme.style_muted());
        }
        return;
    }

    let start_index = model.sidebar_scroll.min(items.len());
    for (item_idx, item) in items.iter().enumerate().skip(start_index) {
        if y >= bottom {
            break;
        }

        let is_cursor = item_idx == model.sidebar_index;

        match item {
            SidebarItem::File {
                entry,
                file_idx,
                collapsed,
            } => {
                let selected = is_cursor;
                let row_bg = if selected && focused {
                    theme.selection_bg
                } else if selected {
                    theme.panel_bg.lerp(theme.selection_bg, 0.5)
                } else {
                    theme.panel_bg
                };
                if selected {
                    buffer.fill_rect(inner.x, y, inner.width, 1, row_bg);
                }
                let collapse_indicator = if *collapsed { "▸ " } else { "▾ " };
                let (prefix, style) = if *file_idx == model.file_index {
                    (collapse_indicator, theme.style_primary().with_bg(row_bg))
                } else {
                    (
                        collapse_indicator,
                        theme.style_foreground_on(row_bg),
                    )
                };

                let prefix_x = inner.x + left_pad;
                buffer.draw_text(prefix_x, y, prefix, style);

                // Thread count indicator
                let thread_indicator = if entry.open_threads > 0 {
                    format!("{}", entry.open_threads)
                } else if entry.resolved_threads > 0 {
                    "✓".to_string()
                } else {
                    " ".to_string()
                };

                let indicator_color = if entry.open_threads > 0 {
                    theme.warning
                } else {
                    theme.success
                };

                let indicator_len = thread_indicator.chars().count() as u32;
                let prefix_width: u32 = 2;
                let filename_width = inner
                    .width
                    .saturating_sub(prefix_width + indicator_len + left_pad + right_pad);

                let filename = truncate_path(&entry.path, filename_width as usize);
                draw_text_truncated(
                    buffer,
                    prefix_x + prefix_width,
                    y,
                    &filename,
                    filename_width,
                    style,
                );

                let indicator_x = inner
                    .x
                    .saturating_add(inner.width)
                    .saturating_sub(right_pad + indicator_len);
                buffer.draw_text(
                    indicator_x,
                    y,
                    &thread_indicator,
                    Style::fg(indicator_color).with_bg(row_bg),
                );
            }
            SidebarItem::Thread {
                thread_id,
                status,
                comment_count,
                ..
            } => {
                let row_bg = if is_cursor && focused {
                    theme.selection_bg
                } else if is_cursor {
                    theme.panel_bg.lerp(theme.selection_bg, 0.5)
                } else {
                    theme.panel_bg
                };
                if is_cursor {
                    buffer.fill_rect(inner.x, y, inner.width, 1, row_bg);
                }

                let indent: u32 = 4;
                let thread_x = inner.x + left_pad + indent;

                // Right-aligned comment count indicator
                let count_text = format!("{comment_count}");
                let count_len = count_text.chars().count() as u32;
                let count_color = if status == "open" {
                    theme.warning
                } else {
                    theme.muted
                };

                let indicator_x = inner
                    .x
                    .saturating_add(inner.width)
                    .saturating_sub(right_pad + count_len);

                let id_width = indicator_x.saturating_sub(thread_x + 1);

                let text_style = if is_cursor {
                    theme.style_foreground_on(row_bg)
                } else {
                    theme.style_muted_on(row_bg)
                };
                draw_text_truncated(buffer, thread_x, y, thread_id, id_width, text_style);

                buffer.draw_text(
                    indicator_x,
                    y,
                    &count_text,
                    Style::fg(count_color).with_bg(row_bg),
                );
            }
        }

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
        inner.height.saturating_sub(3),
    );

    let files = model.files_with_threads();
    if files.is_empty() {
        buffer.draw_text(
            inner.x + 2,
            inner.y + 1,
            "No content available",
            theme.style_muted(),
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

    let description = model
        .current_review
        .as_ref()
        .and_then(|r| r.description.as_deref());

    // Pinned header at the top
    let pinned_height = block_height(1) as u32;
    let pinned_area = Rect::new(
        content_area.x,
        content_area.y,
        content_area.width,
        pinned_height.min(content_area.height),
    );

    // Stream area starts below the pinned header
    let stream_area = Rect::new(
        content_area.x,
        content_area.y + pinned_height,
        content_area.width,
        content_area.height.saturating_sub(pinned_height),
    );

    buffer.fill_rect(
        content_area.x,
        content_area.y,
        content_area.width,
        content_area.height,
        theme.background,
    );

    // Render stream content (description block + files) below pinned header
    render_diff_stream(
        buffer,
        stream_area,
        &DiffStreamParams {
            files: &files,
            file_cache: &model.file_cache,
            threads: &model.threads,
            all_comments: &model.all_comments,
            scroll: model.diff_scroll,
            theme,
            view_mode: model.diff_view_mode,
            wrap: model.diff_wrap,
            thread_positions: &model.thread_positions,
            description,
        },
    );

    // Render pinned header:
    // - When at top (description visible): show review title
    // - When file header reaches pinned position: show current file header
    // The file header text is at: desc_lines + BLOCK_MARGIN + BLOCK_PADDING
    // (accounting for the file block's margin and padding before the header text)
    let layout_width = stream_area.width.saturating_sub(DIFF_MARGIN * 2);
    let desc_lines = description_block_height(description, layout_width);
    let file_header_offset = desc_lines + BLOCK_MARGIN + BLOCK_PADDING;
    if model.diff_scroll >= file_header_offset {
        // Scrolled past description - show file header
        render_pinned_header_block(buffer, pinned_area, file_title, theme, counts);
    } else if let Some(review) = &model.current_review {
        // At top - show review title
        render_pinned_header_block(buffer, pinned_area, &review.title, theme, None);
    }

    // Bottom margin between content and footer
    if inner.height >= 3 {
        let margin_y = inner.y + inner.height - 3;
        buffer.fill_rect(inner.x, margin_y, inner.width, 1, theme.background);
    }

    if model.focus == Focus::FileSidebar {
        dim_rect(buffer, inner, 0.7);
    }
}

/// A hotkey hint: label in dim, key in bright
fn render_help_bar(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let mut footer_x = area.x;
    let mut footer_width = area.width;
    if model.sidebar_visible {
        let sidebar_width = u32::from(model.layout_mode.sidebar_width());
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

    let mut all_hints: Vec<HotkeyHint> = vec![HotkeyHint::new("Commands", "ctrl+p")];

    match model.focus {
        Focus::FileSidebar => {
            all_hints.extend([
                HotkeyHint::new("Navigate", "j/k"),
                HotkeyHint::new("Open", "Enter"),
                HotkeyHint::new("Sidebar", "s"),
                HotkeyHint::new("Back", "h"),
                HotkeyHint::new("Quit", "q"),
            ]);
        }
        Focus::DiffPane => {
            all_hints.extend([
                HotkeyHint::new("Scroll", "j/k"),
                HotkeyHint::new("Thread", "n/p"),
                HotkeyHint::new("View", "v"),
                HotkeyHint::new("Wrap", "w"),
                HotkeyHint::new("Open", "o"),
                HotkeyHint::new("Sidebar", "s"),
                HotkeyHint::new("Back", "Esc"),
                HotkeyHint::new("Quit", "q"),
            ]);
        }
        Focus::ThreadExpanded => {
            all_hints.extend([
                HotkeyHint::new("Scroll", "j/k"),
                HotkeyHint::new("Resolve", "r"),
                HotkeyHint::new("Collapse", "Esc"),
            ]);
        }
        _ => {
            all_hints.extend([
                HotkeyHint::new("Back", "Esc"),
                HotkeyHint::new("Quit", "q"),
            ]);
        }
    }

    let footer = Rect::new(footer_x, area.y, footer_width, area.height);
    draw_help_bar(buffer, footer, &model.theme, &all_hints);
}
