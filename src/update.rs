//! State update logic (Elm Architecture)

use crate::message::Message;
use crate::model::{DiffViewMode, EditorRequest, Focus, Model, ReviewFilter, Screen};
use crate::stream::{
    active_file_index, compute_stream_layout, file_scroll_offset, thread_stream_offset,
};

/// Update the model based on a message, returning an optional command
pub fn update(model: &mut Model, msg: Message) {
    match msg {
        // === Navigation ===
        Message::SelectReview(_id) => {
            // Switch to review detail screen
            model.screen = Screen::ReviewDetail;
            model.focus = Focus::FileSidebar;
            model.file_index = 0;
            model.diff_scroll = 0;
            model.expanded_thread = None;
            model.current_review = None; // Clear to trigger reload
            model.current_diff = None;
            model.current_file_content = None;
            model.highlighted_lines.clear();
            model.file_cache.clear();
            model.threads.clear();
            model.comments.clear();
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
                model.comments.clear();
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
            let file_count = model.files_with_threads().len();
            if file_count > 0 && model.file_index < file_count - 1 {
                let target = model.file_index + 1;
                jump_to_file(model, target);
            }
        }

        Message::PrevFile => {
            if model.file_index > 0 {
                let target = model.file_index - 1;
                jump_to_file(model, target);
            }
        }

        Message::SelectFile(idx) => {
            let file_count = model.files_with_threads().len();
            if idx < file_count {
                jump_to_file(model, idx);
            }
        }

        // === Diff/Content Pane ===
        Message::ScrollUp => {
            model.diff_scroll = model.diff_scroll.saturating_sub(1);
            update_active_file_from_scroll(model);
        }

        Message::ScrollDown => {
            // TODO: clamp to content height
            model.diff_scroll += 1;
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
            update_active_file_from_scroll(model);
        }

        Message::ScrollTenUp => {
            model.diff_scroll = model.diff_scroll.saturating_sub(10);
            update_active_file_from_scroll(model);
        }

        Message::ScrollTenDown => {
            model.diff_scroll += 10;
            update_active_file_from_scroll(model);
        }

        Message::PageUp => {
            let page = model.height.saturating_sub(2) as usize;
            model.diff_scroll = model.diff_scroll.saturating_sub(page);
            update_active_file_from_scroll(model);
        }

        Message::PageDown => {
            let page = model.height.saturating_sub(2) as usize;
            // TODO: clamp to content height
            model.diff_scroll += page;
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
        }

        Message::OpenFileInEditor => {
            let files = model.files_with_threads();
            if let Some(file) = files.get(model.file_index) {
                let line = model
                    .expanded_thread
                    .as_ref()
                    .and_then(|thread_id| model.threads.iter().find(|t| t.thread_id == *thread_id))
                    .and_then(|thread| {
                        if thread.selection_start > 0 {
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

        Message::Noop => {}
    }
}

fn jump_to_file(model: &mut Model, index: usize) {
    model.file_index = index;
    model.expanded_thread = None;
    model.comments.clear();

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
    let layout = stream_layout(model);
    let files = model.files_with_threads();
    let width = diff_content_width(model);
    if let Some(stream_row) = thread_stream_offset(
        &layout,
        &files,
        &model.file_cache,
        &model.threads,
        &thread_id,
        model.diff_view_mode,
        model.diff_wrap,
        width,
    ) {
        let view_height = model.height.saturating_sub(2) as usize;
        let center = view_height / 2;
        model.diff_scroll = stream_row.saturating_sub(center);
    }
}

fn stream_layout(model: &Model) -> crate::stream::StreamLayout {
    let files = model.files_with_threads();
    let width = diff_content_width(model);
    compute_stream_layout(
        &files,
        &model.file_cache,
        &model.threads,
        model.expanded_thread.as_deref(),
        &model.comments,
        model.diff_view_mode,
        model.diff_wrap,
        width,
    )
}

fn diff_content_width(model: &Model) -> u32 {
    let outer_inner_width = model.width as u32;
    let diff_pane_width = match model.layout_mode {
        crate::model::LayoutMode::Full | crate::model::LayoutMode::Compact => {
            if model.sidebar_visible {
                outer_inner_width.saturating_sub(model.layout_mode.sidebar_width() as u32)
            } else {
                outer_inner_width
            }
        }
        crate::model::LayoutMode::Overlay | crate::model::LayoutMode::Single => outer_inner_width,
    };
    diff_pane_width.saturating_sub(2)
}
