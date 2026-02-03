//! botcrit-ui - GitHub-style code review TUI
//!
//! Usage: crit-ui [path-to-crit-db]
//!
//! If no path is provided, looks for .crit/index.db in current directory.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use opentui::input::{MouseButton, MouseEventKind, ParseError};
use opentui::{
    enable_raw_mode, terminal_size, Event, InputParser, KeyCode, KeyModifiers, Renderer,
    RendererOptions,
};

use botcrit_ui::config::{load_ui_config, save_ui_config};
use botcrit_ui::model::{DiffViewMode, EditorRequest};
use botcrit_ui::stream::{compute_stream_layout, file_scroll_offset, SIDE_BY_SIDE_MIN_WIDTH};
use botcrit_ui::theme::{load_built_in_theme, load_theme_from_path};
use botcrit_ui::{
    update, vcs, view, CliClient, CritClient, Focus, Highlighter, LayoutMode, Message, Model,
    Screen, Theme,
};

fn main() -> Result<()> {
    let args = parse_args()?;

    // Build client: --path or auto-detect .crit/ → CliClient, else demo
    let client: Option<Box<dyn CritClient>> = args
        .repo_path
        .as_ref()
        .map(|repo| -> Box<dyn CritClient> { Box::new(CliClient::new(repo)) });

    // Repo root for vcs diff loading
    let repo_path = args.repo_path.clone();

    // Load theme (optional)
    let mut config = load_ui_config()?.unwrap_or_default();
    let theme_override = args
        .theme
        .clone()
        .or_else(|| std::env::var("BOTCRIT_UI_THEME").ok());
    let theme_selection = theme_override.clone().or_else(|| config.theme.clone());

    let default_theme =
        load_built_in_theme("default-dark").unwrap_or_else(|| botcrit_ui::theme::ThemeLoadResult {
            theme: Theme::default(),
            syntax_theme: None,
        });

    let mut selected_builtin: Option<String> = None;
    let (theme, syntax_theme) = if let Some(selection) = theme_selection {
        if let Some(loaded) = load_built_in_theme(&selection) {
            selected_builtin = Some(selection);
            (loaded.theme, loaded.syntax_theme)
        } else {
            let path = Path::new(&selection);
            if path.exists() {
                let loaded = load_theme_from_path(path)
                    .with_context(|| format!("Failed to load theme: {}", path.display()))?;
                (loaded.theme, loaded.syntax_theme)
            } else if theme_override.is_some() {
                anyhow::bail!("Unknown theme: {selection}");
            } else {
                (default_theme.theme, default_theme.syntax_theme)
            }
        }
    } else {
        (default_theme.theme, default_theme.syntax_theme)
    };

    if theme_override.is_some() {
        if let Some(name) = selected_builtin {
            config.theme = Some(name);
            save_ui_config(&config)?;
        }
    }

    // Get terminal size
    let (width, height) = terminal_size().unwrap_or((80, 24));

    // Create model
    let mut model = Model::new(width as u16, height as u16, config);
    model.theme = theme;
    if let Some(theme_name) = syntax_theme {
        model.highlighter = Highlighter::with_theme(&theme_name);
    } else if model.theme.name.to_lowercase().contains("light") {
        model.highlighter = Highlighter::with_theme("base16-ocean.light");
    }

    apply_default_diff_view(&mut model);

    // Store pending CLI navigation targets
    model.pending_review = args.review;
    model.pending_file = args.file;
    model.pending_thread = args.thread;

    // Load initial data
    if let Some(c) = &client {
        model.reviews = c.list_reviews(None).unwrap_or_default();
    } else {
        // Demo data for testing without a database
        load_demo_data(&mut model);
    }

    // Apply --review: jump directly to a review if specified
    if let Some(review_id) = model.pending_review.take() {
        if let Some(index) = model
            .reviews
            .iter()
            .position(|r| r.review_id == review_id)
        {
            model.list_index = index;
            model.screen = Screen::ReviewDetail;
            model.focus = Focus::DiffPane;
            model.file_index = 0;
            model.sidebar_index = 0;
            model.sidebar_scroll = 0;
            model.diff_scroll = 0;
            model.expanded_thread = None;
            model.current_review = None; // trigger lazy load
            model.current_diff = None;
            model.current_file_content = None;
            model.highlighted_lines.clear();
            model.file_cache.clear();
            model.threads.clear();
            model.all_comments.clear();
        } else {
            // Review not found — clear pending file/thread and stay on review list
            model.pending_file = None;
            model.pending_thread = None;
        }
    } else {
        // No --review, ignore --file and --thread
        model.pending_file = None;
        model.pending_thread = None;
    }

    // Enter raw mode for input handling
    let mut raw_guard = Some(enable_raw_mode().context("Failed to enable raw mode")?);

    // Initialize renderer
    let options = RendererOptions {
        use_alt_screen: true,
        hide_cursor: true,
        enable_mouse: true,
        query_capabilities: false,
    };
    let mut renderer = Renderer::new_with_options(width.into(), height.into(), options)
        .context("Failed to initialize renderer")?;
    let mut wrap_guard = Some(AutoWrapGuard::new().context("Failed to disable line wrap")?);
    let mut cursor_guard = Some(CursorGuard::new().context("Failed to hide cursor")?);
    renderer.set_background(model.theme.background);

    // Input parser
    let mut input = InputParser::new();
    // Track pending standalone Escape (parser returns Incomplete for bare 0x1b)
    let mut pending_esc = false;

    // Main loop
    loop {
        // Detect external terminal resize even if no input events are received
        if let Ok((term_width, term_height)) = terminal_size() {
            let term_width_u16 = term_width as u16;
            let term_height_u16 = term_height as u16;
            if term_width_u16 != model.width || term_height_u16 != model.height {
                model.resize(term_width_u16, term_height_u16);
                model.needs_redraw = true;
                renderer
                    .resize(term_width.into(), term_height.into())
                    .context("Failed to resize renderer")?;
            }
        }

        // Force a full redraw to avoid render artifacts
        renderer.invalidate();
        model.needs_redraw = false;

        // Render
        renderer.clear();
        view(&model, renderer.buffer());
        renderer.present().context("Failed to present frame")?;

        if model.should_quit {
            break;
        }

        if let Some(c) = &client {
            handle_data_loading(&mut model, c.as_ref(), repo_path.as_deref());
        } else {
            handle_demo_data_loading(&mut model);
        }

        // Poll for input (with timeout for potential refresh)
        let mut buf = [0u8; 32];
        if let Ok(n) = read_with_timeout(&mut buf, Duration::from_millis(100)) {
            if n > 0 {
                // If we had a pending escape and new data arrived, feed ESC + new data
                // together so the parser can resolve the sequence.
                if pending_esc {
                    pending_esc = false;
                    // Prepend 0x1b to the buffer
                    let mut combined = Vec::with_capacity(1 + n);
                    combined.push(0x1b);
                    combined.extend_from_slice(&buf[..n]);
                    let combined_len = combined.len();
                    let mut offset = 0usize;
                    while offset < combined_len {
                        match input.parse(&combined[offset..combined_len]) {
                            Ok((event, consumed)) => {
                                offset = offset.saturating_add(consumed);
                                process_event(
                                    &event,
                                    &mut model,
                                    &mut renderer,
                                    &mut raw_guard,
                                    &mut wrap_guard,
                                    &mut cursor_guard,
                                    &client,
                                    repo_path.as_deref(),
                                    options,
                                )?;
                            }
                            Err(ParseError::Empty) | Err(ParseError::Incomplete) => {
                                // Check if stuck on a bare escape again
                                if offset < combined_len
                                    && combined[offset] == 0x1b
                                    && offset + 1 == combined_len
                                {
                                    pending_esc = true;
                                }
                                break;
                            }
                            Err(_) => {
                                offset = offset.saturating_add(1);
                            }
                        }
                    }
                } else {
                    let mut offset = 0usize;
                    while offset < n {
                        match input.parse(&buf[offset..n]) {
                            Ok((event, consumed)) => {
                                offset = offset.saturating_add(consumed);
                                process_event(
                                    &event,
                                    &mut model,
                                    &mut renderer,
                                    &mut raw_guard,
                                    &mut wrap_guard,
                                    &mut cursor_guard,
                                    &client,
                                    repo_path.as_deref(),
                                    options,
                                )?;
                            }
                            Err(ParseError::Empty) | Err(ParseError::Incomplete) => {
                                // If the remaining buffer is just 0x1b, mark pending
                                if offset < n && buf[offset] == 0x1b && offset + 1 == n {
                                    pending_esc = true;
                                }
                                break;
                            }
                            Err(_) => {
                                offset = offset.saturating_add(1);
                            }
                        }
                    }
                }
            } else if pending_esc {
                // No new data arrived — resolve pending escape as standalone Esc key
                pending_esc = false;
                let esc_event = Event::Key(opentui::KeyEvent::key(KeyCode::Esc));
                process_event(
                    &esc_event,
                    &mut model,
                    &mut renderer,
                    &mut raw_guard,
                    &mut wrap_guard,
                    &mut cursor_guard,
                    &client,
                    repo_path.as_deref(),
                    options,
                )?;
            }
        } else if pending_esc {
            // Read error/timeout — resolve pending escape
            pending_esc = false;
            let esc_event = Event::Key(opentui::KeyEvent::key(KeyCode::Esc));
            process_event(
                &esc_event,
                &mut model,
                &mut renderer,
                &mut raw_guard,
                &mut wrap_guard,
                &mut cursor_guard,
                &client,
                repo_path.as_deref(),
                options,
            )?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_event(
    event: &Event,
    model: &mut Model,
    renderer: &mut Renderer,
    raw_guard: &mut Option<opentui::RawModeGuard>,
    wrap_guard: &mut Option<AutoWrapGuard>,
    cursor_guard: &mut Option<CursorGuard>,
    _client: &Option<Box<dyn CritClient>>,
    repo_path: Option<&Path>,
    options: RendererOptions,
) -> Result<()> {
    let msg = map_event_to_message(model, event.clone());
    let resize = if let Message::Resize { width, height } = &msg {
        Some((*width, *height))
    } else {
        None
    };
    update(model, msg);

    if let Some((width, height)) = resize {
        renderer
            .resize(width.into(), height.into())
            .context("Failed to resize renderer")?;
        model.needs_redraw = true;
    }

    if let Some(request) = model.pending_editor_request.take() {
        let (prev_width, prev_height) = renderer.size();
        let prev_width = prev_width as u16;
        let prev_height = prev_height as u16;
        drop(std::mem::replace(
            renderer,
            Renderer::new_with_options(1, 1, options).unwrap(),
        ));
        raw_guard.take();
        wrap_guard.take();
        cursor_guard.take();

        let _ = open_file_in_editor(repo_path, request);

        *raw_guard = Some(enable_raw_mode().context("Failed to enable raw mode")?);
        let (width, height) = terminal_size().unwrap_or((prev_width, prev_height));
        *renderer = Renderer::new_with_options(width.into(), height.into(), options)
            .context("Failed to initialize renderer")?;
        renderer.set_background(model.theme.background);
        *wrap_guard = Some(AutoWrapGuard::new().context("Failed to disable line wrap")?);
        *cursor_guard = Some(CursorGuard::new().context("Failed to hide cursor")?);
        model.resize(width as u16, height as u16);
        model.needs_redraw = true;
        renderer.invalidate();
    }

    Ok(())
}

struct AutoWrapGuard;

impl AutoWrapGuard {
    fn new() -> std::io::Result<Self> {
        let mut out = std::io::stdout();
        out.write_all(b"\x1b[?7l")?; // Disable line wrap
        out.flush()?;
        Ok(Self)
    }
}

impl Drop for AutoWrapGuard {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x1b[?7h"); // Re-enable line wrap
        let _ = out.flush();
    }
}

struct CursorGuard;

impl CursorGuard {
    fn new() -> std::io::Result<Self> {
        let mut out = std::io::stdout();
        out.write_all(b"\x1b[?25l")?; // Hide cursor
        out.flush()?;
        Ok(Self)
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x1b[?25h"); // Show cursor
        let _ = out.flush();
    }
}

struct CliArgs {
    theme: Option<String>,
    repo_path: Option<PathBuf>,
    review: Option<String>,
    file: Option<String>,
    thread: Option<String>,
}

fn parse_args() -> Result<CliArgs> {
    let args: Vec<String> = std::env::args().collect();
    let mut theme: Option<String> = None;
    let mut repo_path: Option<PathBuf> = None;
    let mut review: Option<String> = None;
    let mut file: Option<String> = None;
    let mut thread: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("Usage: crit-ui [options]");
                println!();
                println!("Options:");
                println!("  --theme <name|path>   Load theme by name or JSON path");
                println!("  --path <path>    Path to repo root (uses crit CLI)");
                println!("  --review <id>    Open directly to a review (skip review list)");
                println!("  --file <path>    Navigate to a specific file (requires --review)");
                println!("  --thread <id>    Expand a specific thread (requires --review)");
                println!();
                println!("Environment:");
                println!("  BOTCRIT_UI_THEME  Theme name or JSON path");
                println!();
                println!("If no path is provided, auto-detects .crit/ in the current directory.");
                println!("If that doesn't exist, runs in demo mode with sample data.");
                std::process::exit(0);
            }
            "--theme" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--theme requires a path");
                }
                theme = Some(args[i].clone());
            }
            "--path" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--path requires a path");
                }
                repo_path = Some(PathBuf::from(&args[i]));
            }
            "--review" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--review requires a review ID");
                }
                review = Some(args[i].clone());
            }
            "--file" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--file requires a file path");
                }
                file = Some(args[i].clone());
            }
            "--thread" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--thread requires a thread ID");
                }
                thread = Some(args[i].clone());
            }
            arg if arg.starts_with('-') => {
                anyhow::bail!("Unknown option: {arg}");
            }
            arg => {
                anyhow::bail!("Unexpected argument: {arg}");
            }
        }
        i += 1;
    }

    // Auto-detect: .crit/ exists in cwd → use CliClient with cwd
    if repo_path.is_none() {
        let crit_dir = PathBuf::from(".crit");
        if crit_dir.is_dir() {
            repo_path = Some(PathBuf::from("."));
        }
    }

    Ok(CliArgs {
        theme,
        repo_path,
        review,
        file,
        thread,
    })
}

fn apply_default_diff_view(model: &mut Model) {
    if let Some(value) = model.config.default_diff_view.as_deref() {
        if let Some(mode) = parse_diff_view_mode(value) {
            model.diff_view_mode = mode;
        }
        return;
    }

    if should_default_side_by_side(model) {
        model.diff_view_mode = DiffViewMode::SideBySide;
    }
}

fn parse_diff_view_mode(value: &str) -> Option<DiffViewMode> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "unified" | "unify" | "uni" => Some(DiffViewMode::Unified),
        "side-by-side" | "side_by_side" | "sidebyside" | "sbs" => Some(DiffViewMode::SideBySide),
        _ => None,
    }
}

fn should_default_side_by_side(model: &Model) -> bool {
    let diff_pane_width = match model.layout_mode {
        LayoutMode::Full | LayoutMode::Compact => {
            if model.sidebar_visible {
                model
                    .width
                    .saturating_sub(model.layout_mode.sidebar_width())
            } else {
                model.width
            }
        }
        LayoutMode::Overlay | LayoutMode::Single => model.width,
    };

    u32::from(diff_pane_width) >= SIDE_BY_SIDE_MIN_WIDTH
}

fn open_file_in_editor(repo_path: Option<&Path>, request: EditorRequest) -> Result<()> {
    let Some(repo_root) = repo_path else {
        return Ok(());
    };

    let file_path = repo_root.join(&request.file_path);
    if !file_path.exists() {
        return Ok(());
    }

    let mut cmd = Command::new("nvim");
    if let Some(line) = request.line {
        cmd.arg(format!("+{}", line));
    }
    cmd.arg(file_path);
    let _ = cmd.status();
    Ok(())
}

fn map_event_to_message(model: &mut Model, event: Event) -> Message {
    match event {
        Event::Key(key) => {
            // Check for Ctrl+C to quit
            if key.modifiers.contains(KeyModifiers::CTRL) && key.code == KeyCode::Char('c') {
                return Message::Quit;
            }

            if key.modifiers.contains(KeyModifiers::CTRL) && key.code == KeyCode::Char('p') {
                return Message::ShowCommandPalette;
            }

            match model.focus {
                Focus::CommandPalette => return map_command_palette_key(key.code),
                _ => {}
            }

            match model.screen {
                Screen::ReviewList => map_review_list_key(key.code, model),
                Screen::ReviewDetail => map_review_detail_key(model, key.code, key.modifiers),
            }
        }
        Event::Resize(resize) => Message::Resize {
            width: resize.width,
            height: resize.height,
        },
        Event::Mouse(mouse) => match model.screen {
            Screen::ReviewList => map_review_list_mouse(model, mouse),
            Screen::ReviewDetail => map_review_detail_mouse(model, mouse),
        },
        Event::Paste(_) => Message::Noop,
        Event::FocusGained | Event::FocusLost => Message::Noop,
    }
}

fn map_review_list_key(key: KeyCode, model: &Model) -> Message {
    match key {
        KeyCode::Char('q') => Message::Quit,
        KeyCode::Char('j') | KeyCode::Down => Message::ListDown,
        KeyCode::Char('k') | KeyCode::Up => Message::ListUp,
        KeyCode::Char('g') | KeyCode::Home => Message::ListTop,
        KeyCode::Char('G') | KeyCode::End => Message::ListBottom,
        KeyCode::PageUp => Message::ListPageUp,
        KeyCode::PageDown => Message::ListPageDown,
        KeyCode::Enter | KeyCode::Char('l') => {
            // Select the current review
            let reviews = model.filtered_reviews();
            if let Some(review) = reviews.get(model.list_index) {
                Message::SelectReview(review.review_id.clone())
            } else {
                Message::Noop
            }
        }
        KeyCode::Char('o') => Message::FilterOpen,
        KeyCode::Char('a') => Message::FilterAll,
        _ => Message::Noop,
    }
}

fn map_review_list_mouse(model: &mut Model, mouse: opentui::MouseEvent) -> Message {
    if model.focus == Focus::CommandPalette {
        return Message::Noop;
    }

    if mouse.is_scroll() {
        let direction = match mouse.kind {
            MouseEventKind::ScrollUp => -1,
            MouseEventKind::ScrollDown => 1,
            _ => return Message::Noop,
        };
        if !should_handle_scroll(&mut model.last_list_scroll, direction) {
            return Message::Noop;
        }
        return match mouse.kind {
            MouseEventKind::ScrollUp => Message::ListUp,
            MouseEventKind::ScrollDown => Message::ListDown,
            _ => Message::Noop,
        };
    }

    if mouse.button != MouseButton::Left {
        return Message::Noop;
    }

    if !matches!(mouse.kind, MouseEventKind::Press | MouseEventKind::Release) {
        return Message::Noop;
    }

    let header_height = 1u32;
    let footer_height = 2u32;
    let height = u32::from(model.height);
    if height <= header_height + footer_height {
        return Message::Noop;
    }

    let list_start = header_height;
    let list_end = height.saturating_sub(footer_height);
    if mouse.y < list_start || mouse.y >= list_end {
        return Message::Noop;
    }

    let row = (mouse.y - list_start) as usize;
    let index = model.list_scroll + row;
    let reviews = model.filtered_reviews();
    if let Some(review) = reviews.get(index) {
        return Message::SelectReview(review.review_id.clone());
    }

    Message::Noop
}

fn map_review_detail_mouse(model: &mut Model, mouse: opentui::MouseEvent) -> Message {
    if model.focus == Focus::CommandPalette || model.focus == Focus::Commenting {
        return Message::Noop;
    }

    let sidebar_rect = match model.layout_mode {
        LayoutMode::Full | LayoutMode::Compact | LayoutMode::Overlay => {
            if !model.sidebar_visible {
                None
            } else {
                Some((
                    0u32,
                    0u32,
                    model.layout_mode.sidebar_width() as u32,
                    model.height as u32,
                ))
            }
        }
        LayoutMode::Single => {
            if !model.sidebar_visible || !matches!(model.focus, Focus::FileSidebar) {
                None
            } else {
                Some((0u32, 0u32, model.width as u32, model.height as u32))
            }
        }
    };

    if mouse.is_scroll() {
        let direction = match mouse.kind {
            MouseEventKind::ScrollUp => -1,
            MouseEventKind::ScrollDown => 1,
            _ => return Message::Noop,
        };
        if let Some((x, y, width, height)) = sidebar_rect {
            if mouse.x >= x
                && mouse.x < x.saturating_add(width)
                && mouse.y >= y
                && mouse.y < y.saturating_add(height)
            {
                if !should_handle_scroll(&mut model.last_sidebar_scroll, direction) {
                    return Message::Noop;
                }
                return match mouse.kind {
                    MouseEventKind::ScrollUp => Message::PrevFile,
                    MouseEventKind::ScrollDown => Message::NextFile,
                    _ => Message::Noop,
                };
            }
        }

        return match mouse.kind {
            MouseEventKind::ScrollUp => Message::ScrollUp,
            MouseEventKind::ScrollDown => Message::ScrollDown,
            _ => Message::Noop,
        };
    }

    if mouse.button != MouseButton::Left {
        return Message::Noop;
    }

    if !matches!(mouse.kind, MouseEventKind::Press | MouseEventKind::Release) {
        return Message::Noop;
    }

    let Some((sidebar_x, sidebar_y, sidebar_width, sidebar_height)) = sidebar_rect else {
        return Message::Noop;
    };

    if mouse.x < sidebar_x
        || mouse.x >= sidebar_x.saturating_add(sidebar_width)
        || mouse.y < sidebar_y
        || mouse.y >= sidebar_y.saturating_add(sidebar_height)
    {
        return Message::Noop;
    }

    let mut list_start = sidebar_y + 1;
    if model.current_review.is_some() {
        list_start = list_start.saturating_add(5);
    }
    let bottom = sidebar_y + sidebar_height.saturating_sub(1);
    if list_start >= bottom || mouse.y < list_start || mouse.y >= bottom {
        return Message::Noop;
    }

    let row = (mouse.y - list_start) as usize;
    let index = model.sidebar_scroll.saturating_add(row);
    let items = model.sidebar_items();
    if items.get(index).is_some() {
        return Message::ClickSidebarItem(index);
    }

    Message::Noop
}

fn should_handle_scroll(last: &mut Option<(Instant, i8)>, direction: i8) -> bool {
    const DEBOUNCE: Duration = Duration::from_millis(5);
    let now = Instant::now();
    if let Some((prev_at, prev_dir)) = last {
        if *prev_dir == direction && now.duration_since(*prev_at) < DEBOUNCE {
            return false;
        }
    }
    *last = Some((now, direction));
    true
}

fn map_review_detail_key(model: &Model, key: KeyCode, modifiers: KeyModifiers) -> Message {
    use botcrit_ui::Focus;

    if modifiers.contains(KeyModifiers::CTRL) {
        match key {
            KeyCode::Char('j') => return Message::ScrollTenDown,
            KeyCode::Char('k') => return Message::ScrollTenUp,
            _ => {}
        }
    }

    match model.focus {
        Focus::FileSidebar => match key {
            KeyCode::Char('q') => Message::Quit,
            KeyCode::Esc | KeyCode::Char('h') => Message::Back,
            KeyCode::Tab => Message::ToggleFocus,
            KeyCode::Char('j') | KeyCode::Down => Message::NextFile,
            KeyCode::Char('k') | KeyCode::Up => Message::PrevFile,
            KeyCode::Char('g') | KeyCode::Home => Message::SidebarTop,
            KeyCode::Char('G') | KeyCode::End => Message::SidebarBottom,
            KeyCode::Enter => Message::SidebarSelect,
            KeyCode::Char('l') => Message::ToggleFocus, // Move to diff pane
            KeyCode::Char('s') => Message::ToggleSidebar,
            _ => Message::Noop,
        },
        Focus::DiffPane => match key {
            KeyCode::Char('q') => Message::Quit,
            KeyCode::Esc => Message::Back,
            KeyCode::Tab => Message::ToggleFocus,
            KeyCode::Char('j') | KeyCode::Down => Message::ScrollDown,
            KeyCode::Char('k') | KeyCode::Up => Message::ScrollUp,
            KeyCode::Char('g') | KeyCode::Home => Message::ScrollTop,
            KeyCode::Char('G') | KeyCode::End => Message::ScrollBottom,
            KeyCode::Char('n') => Message::NextThread,
            KeyCode::Char('p') | KeyCode::Char('N') => Message::PrevThread,
            KeyCode::Char('v') => Message::ToggleDiffView, // Toggle unified/side-by-side
            KeyCode::Char('w') => Message::ToggleDiffWrap,
            KeyCode::Char('o') => Message::OpenFileInEditor,
            KeyCode::Char('c') => Message::EnterCommentMode,
            KeyCode::Char('u') => Message::ScrollHalfPageUp,
            KeyCode::Char('d') => Message::ScrollHalfPageDown,
            KeyCode::Char('b') => Message::PageUp,
            KeyCode::Char('f') => Message::PageDown,
            KeyCode::Char('h') => Message::ToggleFocus,
            KeyCode::Char('s') => Message::ToggleSidebar,
            KeyCode::Enter => {
                // Expand the current thread (if one is selected via n/p)
                if let Some(id) = &model.expanded_thread {
                    Message::ExpandThread(id.clone())
                } else {
                    // Select first thread
                    Message::NextThread
                }
            }
            KeyCode::PageUp => Message::PageUp,
            KeyCode::PageDown => Message::PageDown,
            KeyCode::Char('[') => Message::PrevFile,
            KeyCode::Char(']') => Message::NextFile,
            _ => Message::Noop,
        },
        Focus::ThreadExpanded => match key {
            KeyCode::Esc => Message::CollapseThread,
            KeyCode::Char('j') | KeyCode::Down => Message::ScrollDown,
            KeyCode::Char('k') | KeyCode::Up => Message::ScrollUp,
            KeyCode::Char('g') | KeyCode::Home => Message::ScrollTop,
            KeyCode::Char('G') | KeyCode::End => Message::ScrollBottom,
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if let Some(id) = &model.expanded_thread {
                    Message::ResolveThread(id.clone())
                } else {
                    Message::Noop
                }
            }
            _ => Message::Noop,
        },
        Focus::Commenting => match key {
            KeyCode::Esc => Message::CancelComment,
            KeyCode::Enter => Message::SaveComment,
            KeyCode::Char(c) => Message::CommentInput(c.to_string()),
            KeyCode::Backspace => Message::CommentInputBackspace,
            _ => Message::Noop,
        },
        _ => Message::Noop,
    }
}

fn map_command_palette_key(key: KeyCode) -> Message {
    match key {
        KeyCode::Esc => Message::HideCommandPalette,
        KeyCode::Up => Message::CommandPalettePrev,
        KeyCode::Down => Message::CommandPaletteNext,
        KeyCode::Enter => Message::CommandPaletteExecute,
        KeyCode::Char(c) => Message::CommandPaletteUpdateInput(c.to_string()),
        KeyCode::Backspace => Message::CommandPaletteInputBackspace,
        _ => Message::Noop,
    }
}

fn handle_data_loading(model: &mut Model, client: &dyn CritClient, repo_path: Option<&std::path::Path>) {
    // Load review details when entering detail screen
    if model.screen == Screen::ReviewDetail && model.current_review.is_none() {
        let reviews = model.filtered_reviews();
        if let Some(review) = reviews.get(model.list_index) {
            let review_id = review.review_id.clone();
            if let Ok(Some(data)) = client.load_review_data(&review_id) {
                model.current_review = Some(data.detail);
                model.threads = data.threads;
                model.all_comments = data.comments;
            }
        }
    }

    // Load diff or file content for all files in the review stream
    if model.screen == Screen::ReviewDetail {
        if let (Some(repo), Some(review)) = (repo_path, &model.current_review) {
            let files = model.files_with_threads();
            let from = &review.initial_commit;
            let to = review.final_commit.as_deref();

            for file in &files {
                if model.file_cache.contains_key(&file.path) {
                    continue;
                }

                let diff = vcs::get_file_diff(repo, &file.path, from, to);
                let mut file_content = None;
                let mut file_highlighted_lines = Vec::new();
                let highlighted_lines = if let Some(parsed) = &diff {
                    let diff_highlights =
                        compute_diff_highlights(parsed, &file.path, &model.highlighter);

                    // Check if any threads for this file will be orphaned
                    let file_threads: Vec<&botcrit_ui::db::ThreadSummary> = model
                        .threads
                        .iter()
                        .filter(|t| t.file_path == file.path)
                        .collect();
                    let anchors = botcrit_ui::view::map_threads_to_diff(parsed, &file_threads);
                    let anchored_ids: std::collections::HashSet<&str> =
                        anchors.iter().map(|a| a.thread_id.as_str()).collect();
                    let has_orphaned = file_threads
                        .iter()
                        .any(|t| !anchored_ids.contains(t.thread_id.as_str()));

                    if has_orphaned {
                        let commit = to.unwrap_or(from);
                        if let Some(lines) = vcs::get_file_content(repo, &file.path, commit) {
                            file_highlighted_lines =
                                compute_file_highlights(&lines, &file.path, &model.highlighter);
                            file_content = Some(botcrit_ui::model::FileContent { lines });
                        }
                    }

                    diff_highlights
                } else {
                    let commit = to.unwrap_or(from);
                    if let Some(lines) = vcs::get_file_content(repo, &file.path, commit) {
                        file_content = Some(botcrit_ui::model::FileContent { lines });
                    }
                    if let Some(content) = &file_content {
                        compute_file_highlights(&content.lines, &file.path, &model.highlighter)
                    } else {
                        Vec::new()
                    }
                };

                model.file_cache.insert(
                    file.path.clone(),
                    botcrit_ui::model::FileCacheEntry {
                        diff,
                        file_content,
                        highlighted_lines,
                        file_highlighted_lines,
                    },
                );
            }

            model.sync_active_file_cache();

            // Apply pending CLI navigation targets now that data is loaded
            apply_pending_navigation(model);
        }
    }

    ensure_default_expanded_thread(model);
}

fn apply_pending_navigation(model: &mut Model) {
    if model.pending_thread.is_none() && model.pending_file.is_none() {
        return;
    }

    // --thread implies the file it belongs to, so check it first
    if let Some(thread_id) = model.pending_thread.take() {
        if let Some(thread) = model.threads.iter().find(|t| t.thread_id == thread_id) {
            let thread_file = thread.file_path.clone();
            let files = model.files_with_threads();
            if let Some(idx) = files.iter().position(|f| f.path == thread_file) {
                model.file_index = idx;
                model.diff_scroll = file_scroll_offset(&nav_stream_layout(model), idx);
                model.sync_active_file_cache();
            }
            model.expanded_thread = Some(thread_id);
            // Clear pending_file since --thread takes precedence
            model.pending_file = None;
            model.needs_redraw = true;
            return;
        }
        // Thread not found — fall through to --file if set
    }

    if let Some(file_path) = model.pending_file.take() {
        let files = model.files_with_threads();
        if let Some(idx) = files.iter().position(|f| f.path == file_path) {
            model.file_index = idx;
            model.diff_scroll = file_scroll_offset(&nav_stream_layout(model), idx);
            model.sync_active_file_cache();
            model.needs_redraw = true;
        }
    }
}

/// Compute stream layout for navigation purposes (mirrors update.rs::stream_layout).
fn nav_stream_layout(model: &Model) -> botcrit_ui::stream::StreamLayout {
    const DIFF_MARGIN: u32 = 2;
    let total_width = model.width as u32;
    let pane_width = match model.layout_mode {
        LayoutMode::Full | LayoutMode::Compact | LayoutMode::Overlay => {
            if model.sidebar_visible {
                total_width.saturating_sub(model.layout_mode.sidebar_width() as u32)
            } else {
                total_width
            }
        }
        LayoutMode::Single => total_width,
    };
    let width = pane_width.saturating_sub(DIFF_MARGIN * 2);
    let files = model.files_with_threads();
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

fn handle_demo_data_loading(model: &mut Model) {
    use botcrit_ui::db::ReviewDetail;

    // In demo mode, repopulate threads/comments after SelectReview clears them
    if model.screen == Screen::ReviewDetail && model.threads.is_empty() {
        populate_demo_threads(model);
    }

    // In demo mode, populate current_review when entering detail view
    if model.screen == Screen::ReviewDetail && model.current_review.is_none() {
        let reviews = model.filtered_reviews();
        if let Some(review) = reviews.get(model.list_index) {
            model.current_review = Some(ReviewDetail {
                review_id: review.review_id.clone(),
                jj_change_id: "demo-change-id".to_string(),
                initial_commit: "abc123".to_string(),
                final_commit: None,
                title: review.title.clone(),
                description: Some("Demo review description".to_string()),
                author: review.author.clone(),
                created_at: "2025-01-27T12:00:00Z".to_string(),
                status: review.status.clone(),
                status_changed_at: None,
                status_changed_by: None,
                abandon_reason: None,
                thread_count: review.thread_count,
                open_thread_count: review.open_thread_count,
            });
        }
    }

    // Load diffs for all files in demo mode
    if model.screen == Screen::ReviewDetail {
        let files = model.files_with_threads();
        for file in &files {
            if model.file_cache.contains_key(&file.path) {
                continue;
            }
            let diff = get_demo_diff(&file.path);
            let highlighted_lines = if let Some(parsed) = &diff {
                compute_diff_highlights(parsed, &file.path, &model.highlighter)
            } else {
                Vec::new()
            };

            model.file_cache.insert(
                file.path.clone(),
                botcrit_ui::model::FileCacheEntry {
                    diff,
                    file_content: None,
                    highlighted_lines,
                    file_highlighted_lines: Vec::new(),
                },
            );
        }

        model.sync_active_file_cache();
    }

    ensure_default_expanded_thread(model);
}

fn load_demo_data(model: &mut Model) {
    use botcrit_ui::db::ReviewSummary;

    model.reviews = vec![
        ReviewSummary {
            review_id: "cr-1d3".to_string(),
            title: "Add user authentication".to_string(),
            author: "alice".to_string(),
            status: "open".to_string(),
            thread_count: 3,
            open_thread_count: 2,
        },
        ReviewSummary {
            review_id: "cr-2f8".to_string(),
            title: "Fix database connection pooling".to_string(),
            author: "bob".to_string(),
            status: "open".to_string(),
            thread_count: 1,
            open_thread_count: 1,
        },
        ReviewSummary {
            review_id: "cr-4a1".to_string(),
            title: "Update dependencies to latest versions".to_string(),
            author: "carol".to_string(),
            status: "open".to_string(),
            thread_count: 0,
            open_thread_count: 0,
        },
        ReviewSummary {
            review_id: "cr-0b2".to_string(),
            title: "Initial project setup".to_string(),
            author: "alice".to_string(),
            status: "merged".to_string(),
            thread_count: 2,
            open_thread_count: 0,
        },
        ReviewSummary {
            review_id: "cr-1c9".to_string(),
            title: "WIP: Experimental feature".to_string(),
            author: "bob".to_string(),
            status: "abandoned".to_string(),
            thread_count: 0,
            open_thread_count: 0,
        },
    ];

    populate_demo_threads(model);
}

fn populate_demo_threads(model: &mut Model) {
    use botcrit_ui::db::{Comment, ThreadSummary};

    model.threads = vec![
        ThreadSummary {
            thread_id: "th-001".to_string(),
            file_path: "src/auth.rs".to_string(),
            selection_start: 42,
            selection_end: Some(45),
            status: "open".to_string(),
            comment_count: 3,
        },
        ThreadSummary {
            thread_id: "th-002".to_string(),
            file_path: "src/auth.rs".to_string(),
            selection_start: 78,
            selection_end: None,
            status: "resolved".to_string(),
            comment_count: 2,
        },
        ThreadSummary {
            thread_id: "th-003".to_string(),
            file_path: "src/main.rs".to_string(),
            selection_start: 15,
            selection_end: None,
            status: "open".to_string(),
            comment_count: 1,
        },
    ];

    model.all_comments.insert(
        "th-001".to_string(),
        vec![
            Comment {
                comment_id: "cm-001a".to_string(),
                author: "bob".to_string(),
                body: "The hardcoded 24h expiry should come from config. \
                       What if we need shorter tokens for API clients?"
                    .to_string(),
                created_at: "2025-01-15T10:30:00Z".to_string(),
            },
            Comment {
                comment_id: "cm-001b".to_string(),
                author: "alice".to_string(),
                body: "Good catch — updated to read from config.token_expiry_hours. \
                       Defaults to 24h if unset."
                    .to_string(),
                created_at: "2025-01-15T11:05:00Z".to_string(),
            },
            Comment {
                comment_id: "cm-001c".to_string(),
                author: "bob".to_string(),
                body: "Looks good, thanks!".to_string(),
                created_at: "2025-01-15T11:20:00Z".to_string(),
            },
        ],
    );

    model.all_comments.insert(
        "th-002".to_string(),
        vec![
            Comment {
                comment_id: "cm-002a".to_string(),
                author: "carol".to_string(),
                body: "verify_password now returns Result instead of bool — \
                       nice, this removes the silent failure path."
                    .to_string(),
                created_at: "2025-01-15T14:00:00Z".to_string(),
            },
            Comment {
                comment_id: "cm-002b".to_string(),
                author: "alice".to_string(),
                body: "Exactly. The old unwrap_or(false) was masking bcrypt errors."
                    .to_string(),
                created_at: "2025-01-15T14:30:00Z".to_string(),
            },
        ],
    );

    model.all_comments.insert(
        "th-003".to_string(),
        vec![Comment {
            comment_id: "cm-003a".to_string(),
            author: "bob".to_string(),
            body: "Should we also add a shutdown hook for graceful cleanup?"
                .to_string(),
            created_at: "2025-01-16T09:00:00Z".to_string(),
        }],
    );
}

fn ensure_default_expanded_thread(model: &mut Model) {
    if model.expanded_thread.is_some() {
        return;
    }

    if let Some(thread) = model.threads_for_current_file().first() {
        model.expanded_thread = Some(thread.thread_id.clone());
        return;
    }

    if let Some(thread) = model.threads.first() {
        model.expanded_thread = Some(thread.thread_id.clone());
    }
}

/// Get demo diff content for a file path
fn get_demo_diff(file_path: &str) -> Option<botcrit_ui::diff::ParsedDiff> {
    use botcrit_ui::diff::ParsedDiff;

    let diff_content = match file_path {
        "src/auth.rs" => {
            r#"diff --git a/src/auth.rs b/src/auth.rs
index abc123..def456 100644
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -38,15 +38,18 @@ impl AuthService {
     pub fn new(config: &Config) -> Self {
         Self {
             secret: config.jwt_secret.clone(),
-            expiry: Duration::hours(24),
+            expiry: Duration::hours(config.token_expiry_hours),
         }
     }
 
-    pub fn authenticate(&self, username: &str, password: &str) -> Result<Token> {
+    /// Authenticate a user and return a JWT token
+    pub fn authenticate(&self, username: &str, password: &str) -> Result<Token, AuthError> {
+        // Validate input
+        if username.is_empty() || password.is_empty() {
+            return Err(AuthError::InvalidCredentials);
+        }
+
         let user = self.find_user(username)?;
-        if !verify_password(password, &user.password_hash) {
-            return Err(anyhow!("Invalid credentials"));
-        }
+        self.verify_password(password, &user.password_hash)?;
         
         self.generate_token(&user)
     }
@@ -72,12 +75,14 @@ impl AuthService {
         Ok(User { id, username, role })
     }
 
-    fn verify_password(&self, password: &str, hash: &str) -> bool {
-        bcrypt::verify(password, hash).unwrap_or(false)
+    fn verify_password(&self, password: &str, hash: &str) -> Result<(), AuthError> {
+        if bcrypt::verify(password, hash).unwrap_or(false) {
+            Ok(())
+        } else {
+            Err(AuthError::InvalidCredentials)
+        }
     }
 
     fn generate_token(&self, user: &User) -> Result<Token> {
         let claims = Claims {
             sub: user.id.to_string(),
"#
        }
        "src/main.rs" => {
            r#"diff --git a/src/main.rs b/src/main.rs
index 111222..333444 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,14 +10,18 @@ use config::Config;
 
 mod auth;
 mod config;
+mod error;
 mod handlers;
 
 fn main() -> Result<()> {
-    let config = Config::from_env()?;
+    // Load configuration from environment or file
+    let config = Config::load()?;
+    
+    // Initialize logging
     env_logger::init();
+    info!("Starting server with config: {:?}", config);
     
-    let auth = AuthService::new(&config);
-    let server = Server::new(config.port);
+    let app = App::new(config)?;
     
-    server.run(auth)?;
+    app.run()?;
     Ok(())
 }
"#
        }
        _ => return None,
    };

    Some(ParsedDiff::parse(diff_content))
}

/// Read from stdin with a timeout
fn read_with_timeout(buf: &mut [u8], _timeout: Duration) -> std::io::Result<usize> {
    use std::io::Read;
    // Note: This is a simplified version. In production, you'd use
    // poll/select or async I/O for proper timeout handling.
    // For now, we rely on the terminal being in raw mode with VMIN=0, VTIME=1
    std::io::stdin().read(buf)
}

/// Compute syntax highlighting for diff lines
fn compute_diff_highlights(
    diff: &botcrit_ui::diff::ParsedDiff,
    file_path: &str,
    highlighter: &botcrit_ui::Highlighter,
) -> Vec<Vec<botcrit_ui::HighlightSpan>> {
    let mut result = Vec::new();

    // Get a file highlighter to maintain state across lines
    let Some(mut file_hl) = highlighter.for_file(file_path) else {
        return result;
    };

    for hunk in &diff.hunks {
        // Hunk header - no highlighting needed
        result.push(Vec::new());

        for line in &hunk.lines {
            let spans = file_hl.highlight_line(&line.content);
            result.push(spans);
        }
    }

    result
}

/// Compute syntax highlighting for file content lines
fn compute_file_highlights(
    lines: &[String],
    file_path: &str,
    highlighter: &botcrit_ui::Highlighter,
) -> Vec<Vec<botcrit_ui::HighlightSpan>> {
    let Some(mut file_hl) = highlighter.for_file(file_path) else {
        return Vec::new();
    };

    lines
        .iter()
        .map(|line| file_hl.highlight_line(line))
        .collect()
}
