//! State update logic (Elm Architecture)

use crate::message::Message;
use crate::model::{DiffViewMode, Focus, Model, ReviewFilter, Screen};

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
        }

        Message::ListPageUp => {
            let visible = model.list_visible_height();
            model.list_index = model.list_index.saturating_sub(visible);
            model.list_scroll = model.list_scroll.saturating_sub(visible);
        }

        Message::ListPageDown => {
            let count = model.filtered_reviews().len();
            let visible = model.list_visible_height();
            let max_index = count.saturating_sub(1);
            let max_scroll = count.saturating_sub(visible);

            model.list_index = (model.list_index + visible).min(max_index);
            model.list_scroll = (model.list_scroll + visible).min(max_scroll);
        }

        Message::ListTop => {
            model.list_index = 0;
            model.list_scroll = 0;
        }

        Message::ListBottom => {
            let count = model.filtered_reviews().len();
            if count > 0 {
                model.list_index = count - 1;
                let visible = model.list_visible_height();
                model.list_scroll = count.saturating_sub(visible);
            }
        }

        // === File Sidebar ===
        Message::NextFile => {
            let file_count = model.files_with_threads().len();
            if file_count > 0 && model.file_index < file_count - 1 {
                model.file_index += 1;
                model.diff_scroll = 0;
                model.expanded_thread = None;
                model.comments.clear();
                model.current_diff = None;
                model.current_file_content = None;
                model.highlighted_lines.clear();
            }
        }

        Message::PrevFile => {
            if model.file_index > 0 {
                model.file_index -= 1;
                model.diff_scroll = 0;
                model.expanded_thread = None;
                model.comments.clear();
                model.current_diff = None;
                model.current_file_content = None;
                model.highlighted_lines.clear();
            }
        }

        Message::SelectFile(idx) => {
            let file_count = model.files_with_threads().len();
            if idx < file_count {
                model.file_index = idx;
                model.diff_scroll = 0;
                model.expanded_thread = None;
                model.comments.clear();
                model.current_diff = None;
                model.current_file_content = None;
                model.highlighted_lines.clear();
            }
        }

        // === Diff/Content Pane ===
        Message::ScrollUp => {
            model.diff_scroll = model.diff_scroll.saturating_sub(1);
        }

        Message::ScrollDown => {
            // TODO: clamp to content height
            model.diff_scroll += 1;
        }

        Message::PageUp => {
            let page = model.height.saturating_sub(4) as usize;
            model.diff_scroll = model.diff_scroll.saturating_sub(page);
        }

        Message::PageDown => {
            let page = model.height.saturating_sub(4) as usize;
            // TODO: clamp to content height
            model.diff_scroll += page;
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
        }

        Message::CollapseThread => {
            model.expanded_thread = None;
            model.focus = Focus::DiffPane;
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
        }

        Message::FilterAll => {
            model.filter = ReviewFilter::All;
            model.list_index = 0;
            model.list_scroll = 0;
        }

        Message::ToggleDiffView => {
            model.diff_view_mode = match model.diff_view_mode {
                DiffViewMode::Unified => DiffViewMode::SideBySide,
                DiffViewMode::SideBySide => DiffViewMode::Unified,
            };
            model.needs_redraw = true;
        }

        // === System ===
        Message::Resize { width, height } => {
            model.resize(width, height);
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
