//! View rendering

mod command_palette;
mod components;
mod diff;
mod review_detail;
mod review_list;

pub use diff::{diff_line_count, render_diff, render_file_context};

use opentui::OptimizedBuffer;

use crate::model::{Model, Screen};

pub use components::Rect;

/// Render the current model state to the buffer
pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    // Clear with background color
    buffer.clear(model.theme.background);

    match model.screen {
        Screen::ReviewList => review_list::view(model, buffer),
        Screen::ReviewDetail => review_detail::view(model, buffer),
    }

    command_palette::view(model, buffer);
}
