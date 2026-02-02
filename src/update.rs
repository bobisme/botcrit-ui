//! State update logic (Elm Architecture)

use crate::command::{command_id_to_message, get_commands};
use crate::message::Message;
use crate::model::{DiffViewMode, EditorRequest, Focus, Model, PaletteMode, ReviewFilter, Screen};
use crate::stream::{active_file_index, compute_stream_layout, file_scroll_offset};
use crate::{config, theme, Highlighter};

pub fn update(model: &mut Model, msg: Message) {
    match msg {
        // === Navigation ===
        Message::SelectReview(id) => {
            if let Some(index) = model
                .filtered_reviews()
                .iter()
                .position(|review| review.review_id == id)
            {
                model.list_index = index;
                let visible = model.list_visible_height().max(1);
                if model.list_index < model.list_scroll {
                    model.list_scroll = model.list_index;
                } else if model.list_index >= model.list_scroll + visible {
                    model.list_scroll = model.list_index.saturating_sub(visible.saturating_sub(1));
                }
            }
            // Switch to review detail screen
            model.screen = Screen::ReviewDetail;
            model.focus = Focus::DiffPane;
            model.file_index = 0;
            model.sidebar_index = 0;
            model.collapsed_files.clear();
            model.diff_scroll = 0;
            model.expanded_thread = None;
            model.current_review = None; // Clear to trigger reload
            model.current_diff = None;
            model.current_file_content = None;
            model.highlighted_lines.clear();
            model.file_cache.clear();
            model.threads.clear();
            model.all_comments.clear();
            model.needs_redraw = true;
            // Note: caller should load review details from DB
        }

        Message::Back => match model.screen {
            Screen::ReviewDetail => {
                model.screen = Screen::ReviewList;
                model.focus = Focus::ReviewList;
                model.current_review = None;
                model.current_diff = None;
                model.current_file_content = None;
                model.highlighted_lines.clear();
                model.file_cache.clear();
                model.threads.clear();
                model.all_comments.clear();
                model.needs_redraw = true;
            }
            Screen::ReviewList => {
                // Already at top level, could quit or no-op
            }
        },

        // === List Navigation ===
        Message::ListUp => {
            let count = model.filtered_reviews().len();
            if count > 0 && model.list_index > 0 {
                model.list_index -= 1;
                // Adjust scroll if needed
                if model.list_index < model.list_scroll {
                    model.list_scroll = model.list_index;
                }
            }
            model.needs_redraw = true;
        }

        Message::ListDown => {
            let count = model.filtered_reviews().len();
            if count > 0 && model.list_index < count - 1 {
                model.list_index += 1;
                // Adjust scroll if needed
                let visible = model.list_visible_height();
                if model.list_index >= model.list_scroll + visible {
                    model.list_scroll = model.list_index - visible + 1;
                }
            }
            model.needs_redraw = true;
        }

        Message::ListPageUp => {
            let visible = model.list_visible_height();
            model.list_index = model.list_index.saturating_sub(visible);
            model.list_scroll = model.list_scroll.saturating_sub(visible);
            model.needs_redraw = true;
        }

        Message::ListPageDown => {
            let count = model.filtered_reviews().len();
            let visible = model.list_visible_height();
            let max_index = count.saturating_sub(1);
            let max_scroll = count.saturating_sub(visible);

            model.list_index = (model.list_index + visible).min(max_index);
            model.list_scroll = (model.list_scroll + visible).min(max_scroll);
            model.needs_redraw = true;
        }

        Message::ListTop => {
            model.list_index = 0;
            model.list_scroll = 0;
            model.needs_redraw = true;
        }

        Message::ListBottom => {
            let count = model.filtered_reviews().len();
            if count > 0 {
                model.list_index = count - 1;
                let visible = model.list_visible_height();
                model.list_scroll = count.saturating_sub(visible);
            }
            model.needs_redraw = true;
        }

        // === File Sidebar ===
        Message::NextFile => {
            let items = model.sidebar_items();
            if !items.is_empty() && model.sidebar_index < items.len() - 1 {
                model.sidebar_index += 1;
                sync_file_index_from_sidebar(model);
            }
        }

        Message::PrevFile => {
            if model.sidebar_index > 0 {
                model.sidebar_index -= 1;
                sync_file_index_from_sidebar(model);
            }
        }

        Message::SelectFile(idx) => {
            let file_count = model.files_with_threads().len();
            if idx < file_count {
                model.focus = Focus::FileSidebar;
                if let Some(pos) = model
                    .sidebar_items()
                    .iter()
                    .position(|item| matches!(item, crate::model::SidebarItem::File { file_idx, .. } if *file_idx == idx))
                {
                    model.sidebar_index = pos;
                }
                jump_to_file(model, idx);
            }
        }

        Message::ClickSidebarItem(idx) => {
            let items = model.sidebar_items();
            if let Some(item) = items.get(idx) {
                model.sidebar_index = idx;
                match item {
                    crate::model::SidebarItem::File { file_idx, .. } => {
                        model.focus = Focus::FileSidebar;
                        jump_to_file(model, *file_idx);
                    }
                    crate::model::SidebarItem::Thread { .. } => {
                        sync_file_index_from_sidebar(model);
                        model.focus = Focus::DiffPane;
                        model.needs_redraw = true;
                    }
                }
            }
        }

        Message::SidebarSelect => {
            let items = model.sidebar_items();
            if let Some(item) = items.get(model.sidebar_index) {
                match item {
                    crate::model::SidebarItem::File {
                        entry,
                        file_idx,
                        collapsed,
                    } => {
                        // Toggle collapse state
                        if *collapsed {
                            model.collapsed_files.remove(&entry.path);
                        } else {
                            model.collapsed_files.insert(entry.path.clone());
                        }
                        // Clamp sidebar_index to new tree size
                        let new_len = model.sidebar_items().len();
                        if new_len > 0 && model.sidebar_index >= new_len {
                            model.sidebar_index = new_len - 1;
                        }
                        // Also select this file
                        let target = *file_idx;
                        jump_to_file(model, target);
                    }
                    crate::model::SidebarItem::Thread { .. } => {
                        // Sync already centers on thread via sync_file_index_from_sidebar;
                        // Enter additionally switches focus to the diff pane
                        sync_file_index_from_sidebar(model);
                        model.focus = Focus::DiffPane;
                    }
                }
            }
        }

        // === Diff/Content Pane ===
        Message::ScrollUp => {
            model.diff_scroll = model.diff_scroll.saturating_sub(1);
            update_active_file_from_scroll(model);
        }

        Message::ScrollDown => {
            model.diff_scroll += 1;
            clamp_diff_scroll(model);
            update_active_file_from_scroll(model);
        }

        Message::ScrollHalfPageUp => {
            let page = model.height.saturating_sub(2) as usize;
            let half = page.max(1) / 2;
            model.diff_scroll = model.diff_scroll.saturating_sub(half.max(1));
            update_active_file_from_scroll(model);
        }

        Message::ScrollHalfPageDown => {
            let page = model.height.saturating_sub(2) as usize;
            let half = page.max(1) / 2;
            model.diff_scroll += half.max(1);
            clamp_diff_scroll(model);
            update_active_file_from_scroll(model);
        }

        Message::ScrollTenUp => {
            model.diff_scroll = model.diff_scroll.saturating_sub(10);
            update_active_file_from_scroll(model);
        }

        Message::ScrollTenDown => {
            model.diff_scroll += 10;
            clamp_diff_scroll(model);
            update_active_file_from_scroll(model);
        }

        Message::PageUp => {
            let page = model.height.saturating_sub(2) as usize;
            model.diff_scroll = model.diff_scroll.saturating_sub(page);
            update_active_file_from_scroll(model);
        }

        Message::PageDown => {
            let page = model.height.saturating_sub(2) as usize;
            model.diff_scroll += page;
            clamp_diff_scroll(model);
            update_active_file_from_scroll(model);
        }

        Message::NextThread => {
            // Only navigate through threads visible in the diff
            let threads = model.visible_threads_for_current_file();
            if let Some(current) = &model.expanded_thread {
                // Find next thread after current
                if let Some(pos) = threads.iter().position(|t| &t.thread_id == current) {
                    if pos + 1 < threads.len() {
                        model.expanded_thread = Some(threads[pos + 1].thread_id.clone());
                    }
                } else {
                    // Current thread not in visible list, start from first
                    if let Some(first) = threads.first() {
                        model.expanded_thread = Some(first.thread_id.clone());
                    }
                }
            } else if let Some(first) = threads.first() {
                model.expanded_thread = Some(first.thread_id.clone());
            }
            center_on_thread(model);
            update_active_file_from_scroll(model);
        }

        Message::PrevThread => {
            // Only navigate through threads visible in the diff
            let threads = model.visible_threads_for_current_file();
            if let Some(current) = &model.expanded_thread {
                if let Some(pos) = threads.iter().position(|t| &t.thread_id == current) {
                    if pos > 0 {
                        model.expanded_thread = Some(threads[pos - 1].thread_id.clone());
                    }
                } else {
                    // Current thread not in visible list, start from last
                    if let Some(last) = threads.last() {
                        model.expanded_thread = Some(last.thread_id.clone());
                    }
                }
            } else if let Some(last) = threads.last() {
                model.expanded_thread = Some(last.thread_id.clone());
            }
            center_on_thread(model);
            update_active_file_from_scroll(model);
        }

        Message::ExpandThread(id) => {
            model.expanded_thread = Some(id);
            model.focus = Focus::ThreadExpanded;
            center_on_thread(model);
            update_active_file_from_scroll(model);
        }

        Message::CollapseThread => {
            model.expanded_thread = None;
            model.focus = Focus::DiffPane;
            update_active_file_from_scroll(model);
        }

        // === Focus ===
        Message::ToggleFocus => {
            model.focus = match model.focus {
                Focus::ReviewList => Focus::ReviewList, // No toggle on list screen
                Focus::FileSidebar => Focus::DiffPane,
                Focus::DiffPane => Focus::FileSidebar,
                Focus::ThreadExpanded => Focus::DiffPane,
                Focus::CommandPalette => model.previous_focus.take().unwrap_or(Focus::DiffPane),
                Focus::Commenting => Focus::DiffPane,
            };
        }

        // === Actions ===
        Message::ResolveThread(_id) => {
            // TODO: Write to event log
            // For now just update local state
        }

        Message::ReopenThread(_id) => {
            // TODO: Write to event log
        }

        // === Filter/View ===
        Message::FilterOpen => {
            model.filter = ReviewFilter::Open;
            model.list_index = 0;
            model.list_scroll = 0;
            model.needs_redraw = true;
        }

        Message::FilterAll => {
            model.filter = ReviewFilter::All;
            model.list_index = 0;
            model.list_scroll = 0;
            model.needs_redraw = true;
        }

        Message::ToggleDiffView => {
            model.diff_view_mode = match model.diff_view_mode {
                DiffViewMode::Unified => DiffViewMode::SideBySide,
                DiffViewMode::SideBySide => DiffViewMode::Unified,
            };
            model.needs_redraw = true;
            update_active_file_from_scroll(model);
        }

        Message::ToggleSidebar => {
            model.sidebar_visible = !model.sidebar_visible;
            if !model.sidebar_visible && matches!(model.focus, Focus::FileSidebar) {
                model.focus = Focus::DiffPane;
            }
            model.needs_redraw = true;
            update_active_file_from_scroll(model);
        }

        Message::ToggleDiffWrap => {
            model.diff_wrap = !model.diff_wrap;
            model.needs_redraw = true;
            update_active_file_from_scroll(model);
        }

        // === Command Palette ===
        Message::ShowCommandPalette => {
            model.command_palette_mode = PaletteMode::Commands;
            model.command_palette_commands = get_commands();
            model.command_palette_input.clear();
            model.command_palette_selection = 0;
            model.previous_focus = Some(model.focus);
            model.focus = Focus::CommandPalette;
            model.needs_redraw = true;
        }
        Message::HideCommandPalette => {
            // Revert theme preview if we were in theme picker mode
            if model.command_palette_mode == PaletteMode::Themes {
                if let Some(original) = model.pre_palette_theme.take() {
                    update(model, Message::ApplyTheme(original));
                }
            }
            model.command_palette_mode = PaletteMode::Commands;
            model.focus = model.previous_focus.take().unwrap_or(Focus::DiffPane);
            model.needs_redraw = true;
        }
        Message::CommandPaletteNext => {
            let count = match model.command_palette_mode {
                PaletteMode::Commands => model.command_palette_commands.len(),
                PaletteMode::Themes => filter_theme_names(&model.command_palette_input).len(),
            };
            if count > 0 {
                model.command_palette_selection = (model.command_palette_selection + 1) % count;
            }
            preview_selected_theme(model);
            model.needs_redraw = true;
        }
        Message::CommandPalettePrev => {
            let count = match model.command_palette_mode {
                PaletteMode::Commands => model.command_palette_commands.len(),
                PaletteMode::Themes => filter_theme_names(&model.command_palette_input).len(),
            };
            if count > 0 {
                model.command_palette_selection =
                    (model.command_palette_selection + count - 1) % count;
            }
            preview_selected_theme(model);
            model.needs_redraw = true;
        }
        Message::CommandPaletteUpdateInput(input) => {
            model.command_palette_input.push_str(&input);
            model.command_palette_selection = 0;
            if model.command_palette_mode == PaletteMode::Commands {
                model.command_palette_commands = filter_commands(&model.command_palette_input);
            }
            preview_selected_theme(model);
            model.needs_redraw = true;
        }
        Message::CommandPaletteInputBackspace => {
            model.command_palette_input.pop();
            model.command_palette_selection = 0;
            if model.command_palette_mode == PaletteMode::Commands {
                model.command_palette_commands = filter_commands(&model.command_palette_input);
            }
            preview_selected_theme(model);
            model.needs_redraw = true;
        }
        Message::CommandPaletteExecute => {
            match model.command_palette_mode {
                PaletteMode::Commands => {
                    let commands = model.command_palette_commands.clone();
                    if let Some(command) = commands.get(model.command_palette_selection) {
                        update(model, Message::HideCommandPalette);
                        let msg = command_id_to_message(command.id);
                        update(model, msg);
                    }
                }
                PaletteMode::Themes => {
                    let theme_names = filter_theme_names(&model.command_palette_input);
                    if let Some(name) = theme_names.get(model.command_palette_selection) {
                        let name = name.to_string();
                        // Clear saved theme so HideCommandPalette won't revert
                        model.pre_palette_theme = None;
                        update(model, Message::HideCommandPalette);
                        update(model, Message::ApplyTheme(name));
                    }
                }
            }
        }

        Message::OpenFileInEditor => {
            let files = model.files_with_threads();
            if let Some(file) = files.get(model.file_index) {
                let line = model
                    .expanded_thread
                    .as_ref()
                    .and_then(|thread_id| model.threads.iter().find(|t| t.thread_id == *thread_id))
                    .and_then(|thread| {
                        // Only use line number if thread is for the current file
                        if thread.file_path == file.path && thread.selection_start > 0 {
                            Some(thread.selection_start as u32)
                        } else {
                            None
                        }
                    });
                model.pending_editor_request = Some(EditorRequest {
                    file_path: file.path.clone(),
                    line,
                });
            }
        }

        // === System ===
        Message::Resize { width, height } => {
            model.resize(width, height);
            model.needs_redraw = true;
            update_active_file_from_scroll(model);
        }

        Message::Tick => {
            // Could refresh data, animate, etc.
        }

        Message::Quit => {
            model.should_quit = true;
        }

        // === Commenting ===
        Message::EnterCommentMode => {
            model.comment_input.clear();
            model.comment_target_line = None;
            model.focus = Focus::Commenting;
            model.needs_redraw = true;
        }
        Message::CommentInput(text) => {
            model.comment_input.push_str(&text);
            model.needs_redraw = true;
        }
        Message::CommentInputBackspace => {
            model.comment_input.pop();
            model.needs_redraw = true;
        }
        Message::SaveComment => {
            // TODO: persist comment via crit
            model.focus = Focus::DiffPane;
            model.needs_redraw = true;
        }
        Message::CancelComment => {
            model.comment_input.clear();
            model.comment_target_line = None;
            model.focus = Focus::DiffPane;
            model.needs_redraw = true;
        }

        // === Theme Selection ===
        Message::ShowThemePicker => {
            model.pre_palette_theme = model.config.theme.clone();
            model.command_palette_mode = PaletteMode::Themes;
            model.command_palette_input.clear();
            let theme_names = filter_theme_names(&model.command_palette_input);
            model.command_palette_selection = theme_names
                .iter()
                .position(|&name| name == model.theme.name)
                .unwrap_or(0);
            model.previous_focus = Some(model.focus);
            model.focus = Focus::CommandPalette;
            model.needs_redraw = true;
        }
        Message::ApplyTheme(theme_name) => {
            if let Some(loaded) = theme::load_built_in_theme(&theme_name) {
                model.theme = loaded.theme;
                if let Some(syntax_theme) = loaded.syntax_theme {
                    model.highlighter = Highlighter::with_theme(&syntax_theme);
                } else if theme_name.to_lowercase().contains("light") {
                    model.highlighter = Highlighter::with_theme("base16-ocean.light");
                } else {
                    model.highlighter = Highlighter::with_theme("base16-ocean.dark");
                }
                model.config.theme = Some(theme_name);
                let _ = config::save_ui_config(&model.config);
                model.needs_redraw = true;
            }
        }

        Message::Noop => {}
    }
}

fn sync_file_index_from_sidebar(model: &mut Model) {
    let items = model.sidebar_items();
    if let Some(item) = items.get(model.sidebar_index) {
        match item {
            crate::model::SidebarItem::File { file_idx, .. } => {
                if *file_idx != model.file_index {
                    jump_to_file(model, *file_idx);
                } else {
                    model.expanded_thread = None;
                    model.needs_redraw = true;
                }
            }
            crate::model::SidebarItem::Thread {
                file_idx,
                thread_id,
                ..
            } => {
                let target = *file_idx;
                let tid = thread_id.clone();
                if target != model.file_index {
                    jump_to_file(model, target);
                }
                model.expanded_thread = Some(tid);
                center_on_thread(model);
                model.needs_redraw = true;
            }
        }
    }
}

fn jump_to_file(model: &mut Model, index: usize) {
    model.file_index = index;
    model.expanded_thread = None;

    let layout = stream_layout(model);
    model.diff_scroll = file_scroll_offset(&layout, index);
    model.sync_active_file_cache();
    model.needs_redraw = true;
}

fn update_active_file_from_scroll(model: &mut Model) {
    let layout = stream_layout(model);
    let active = active_file_index(&layout, model.diff_scroll);
    if active != model.file_index {
        model.file_index = active;
        model.sync_active_file_cache();
    }
    model.needs_redraw = true;
}

fn center_on_thread(model: &mut Model) {
    let Some(thread_id) = model.expanded_thread.clone() else {
        return;
    };
    // Use positions captured during the last render pass
    let positions = model.thread_positions.borrow();
    if let Some(&stream_row) = positions.get(&thread_id) {
        drop(positions);
        let view_height = model.height.saturating_sub(2) as usize;
        let center = view_height / 2;
        model.diff_scroll = stream_row.saturating_sub(center);
    } else {
        drop(positions);
        // Thread not anchored in the diff (line outside hunk range).
        // Scroll to the end of the file's section as a fallback.
        let layout = stream_layout(model);
        let files = model.files_with_threads();
        if let Some(thread) = model.threads.iter().find(|t| t.thread_id == thread_id) {
            if let Some(file_index) = files.iter().position(|f| f.path == thread.file_path) {
                let file_end = layout
                    .file_offsets
                    .get(file_index + 1)
                    .copied()
                    .unwrap_or(layout.total_lines);
                let view_height = model.height.saturating_sub(2) as usize;
                let center = view_height / 2;
                model.diff_scroll = file_end.saturating_sub(center);
            }
        }
    }
}

fn stream_layout(model: &Model) -> crate::stream::StreamLayout {
    let files = model.files_with_threads();
    let width = diff_content_width(model);
    compute_stream_layout(
        &files,
        &model.file_cache,
        &model.threads,
        &model.all_comments,
        model.diff_view_mode,
        model.diff_wrap,
        width,
    )
}

fn clamp_diff_scroll(model: &mut Model) {
    let layout = stream_layout(model);
    let visible = model.height.saturating_sub(2) as usize;
    let max_scroll = layout.total_lines.saturating_sub(visible);
    if model.diff_scroll > max_scroll {
        model.diff_scroll = max_scroll;
    }
}

fn diff_content_width(model: &Model) -> u32 {
    let total_width = model.width as u32;
    match model.layout_mode {
        crate::model::LayoutMode::Full
        | crate::model::LayoutMode::Compact
        | crate::model::LayoutMode::Overlay => {
            if model.sidebar_visible {
                total_width.saturating_sub(model.layout_mode.sidebar_width() as u32)
            } else {
                total_width
            }
        }
        crate::model::LayoutMode::Single => total_width,
    }
}

/// If the theme picker is active, apply the currently highlighted theme as a preview.
fn preview_selected_theme(model: &mut Model) {
    if model.command_palette_mode != PaletteMode::Themes {
        return;
    }
    let theme_names = filter_theme_names(&model.command_palette_input);
    if let Some(&name) = theme_names.get(model.command_palette_selection) {
        if let Some(loaded) = theme::load_built_in_theme(name) {
            model.theme = loaded.theme;
            if let Some(syntax_theme) = loaded.syntax_theme {
                model.highlighter = Highlighter::with_theme(&syntax_theme);
            }
        }
    }
}

fn filter_theme_names(query: &str) -> Vec<&'static str> {
    let names = theme::built_in_theme_names();
    let terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();
    if terms.is_empty() {
        return names;
    }
    names
        .into_iter()
        .filter(|name| {
            let name_lower = name.to_lowercase();
            terms.iter().all(|term| name_lower.contains(term.as_str()))
        })
        .collect()
}

fn filter_commands(query: &str) -> Vec<crate::command::CommandSpec> {
    let commands = get_commands();
    let terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();
    if terms.is_empty() {
        return commands;
    }
    commands
        .into_iter()
        .filter(|cmd| {
            let name_lower = cmd.name.to_lowercase();
            let cat_lower = cmd.category.to_lowercase();
            terms
                .iter()
                .all(|term| name_lower.contains(term.as_str()) || cat_lower.contains(term.as_str()))
        })
        .collect()
}
