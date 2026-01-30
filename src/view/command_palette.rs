//! Command palette rendering.
use opentui::{OptimizedBuffer, Style};

use crate::{
    model::{Focus, Model},
    view::components::{draw_block, draw_text_truncated, Rect},
};

pub fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    if model.focus != Focus::CommandPalette {
        return;
    }

    let area = Rect::from_size(model.width, model.height);
    let palette_height = (model.command_palette_commands.len() + 2).min(10) as u32;
    let palette_width = 60u32.min(area.width.saturating_sub(4));
    let palette_area = Rect::new(
        (area.width - palette_width) / 2,
        area.height / 4,
        palette_width,
        palette_height,
    );

    draw_block(buffer, palette_area, "Command Palette", true, &model.theme);

    let input_area = Rect::new(
        palette_area.x + 2,
        palette_area.y + 1,
        palette_area.width - 4,
        1,
    );
    let input_text = format!("> {}", model.command_palette_input);
    draw_text_truncated(
        buffer,
        input_area.x,
        input_area.y,
        &input_text,
        input_area.width,
        Style::fg(model.theme.foreground),
    );

    let list_area = Rect::new(
        palette_area.x + 2,
        palette_area.y + 2,
        palette_area.width - 4,
        palette_area.height - 3,
    );

    for (i, command) in model.command_palette_commands.iter().enumerate() {
        if i as u32 >= list_area.height {
            break;
        }
        let y = list_area.y + i as u32;
        let selected = i == model.command_palette_selection;
        let (bg, fg) = if selected {
            (model.theme.selection_bg, model.theme.selection_fg)
        } else {
            (model.theme.background, model.theme.foreground)
        };
        buffer.fill_rect(list_area.x, y, list_area.width, 1, bg);
        draw_text_truncated(
            buffer,
            list_area.x,
            y,
            command.name,
            list_area.width,
            Style::fg(fg),
        );
    }
}
