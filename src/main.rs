//! botcrit-ui - GitHub-style code review TUI
//!
//! Usage: crit-ui [path-to-crit-db]
//!
//! If no path is provided, looks for .crit/index.db in current directory.

#![allow(clippy::too_many_lines)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::literal_string_with_formatting_args)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use opentui::input::ParseError;
use opentui::{
    enable_raw_mode, terminal_size, Event, InputParser, KeyCode, Renderer,
    RendererOptions,
};

use botcrit_ui::config::{load_ui_config, save_ui_config};
use botcrit_ui::input::map_event_to_message;
use botcrit_ui::model::{CommentRequest, DiffViewMode, EditorRequest};
use botcrit_ui::stream::{
    compute_stream_layout, file_scroll_offset, StreamLayoutParams, SIDE_BY_SIDE_MIN_WIDTH,
};
use botcrit_ui::theme::{load_built_in_theme, load_theme_from_path};
use botcrit_ui::{
    update, view, CliClient, CritClient, Focus, Highlighter, LayoutMode, Message, Model,
    Screen, Theme,
};

fn main() -> Result<()> {
    let args = parse_args()?;

    // Build client: --path or auto-detect .crit/ → CliClient, else demo
    let client: Option<Box<dyn CritClient>> = args
        .repo_path
        .as_ref()
        .map(|repo| -> Box<dyn CritClient> { Box::new(CliClient::new(repo)) });

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
    let mut model = Model::new(width, height, config);
    model.theme = theme;
    if let Some(theme_name) = syntax_theme {
        model.highlighter = Highlighter::with_theme(&theme_name);
    } else if model.theme.name.to_lowercase().contains("light") {
        model.highlighter = Highlighter::with_theme("base16-ocean.light");
    }

    apply_default_diff_view(&mut model);

    // Store repo path for display in header
    model.repo_path = repo_path.as_ref().map(|p| p.display().to_string());

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
            let term_width_u16 = term_width;
            let term_height_u16 = term_height;
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
                                    &mut EventContext {
                                        renderer: &mut renderer,
                                        raw_guard: &mut raw_guard,
                                        wrap_guard: &mut wrap_guard,
                                        cursor_guard: &mut cursor_guard,
                                        client: &client,
                                        repo_path: repo_path.as_deref(),
                                        options,
                                    },
                                )?;
                            }
                            Err(ParseError::Empty | ParseError::Incomplete) => {
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
                                    &mut EventContext {
                                        renderer: &mut renderer,
                                        raw_guard: &mut raw_guard,
                                        wrap_guard: &mut wrap_guard,
                                        cursor_guard: &mut cursor_guard,
                                        client: &client,
                                        repo_path: repo_path.as_deref(),
                                        options,
                                    },
                                )?;
                            }
                            Err(ParseError::Empty | ParseError::Incomplete) => {
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
                    &mut EventContext {
                        renderer: &mut renderer,
                        raw_guard: &mut raw_guard,
                        wrap_guard: &mut wrap_guard,
                        cursor_guard: &mut cursor_guard,
                        client: &client,
                        repo_path: repo_path.as_deref(),
                        options,
                    },
                )?;
            }
        } else if pending_esc {
            // Read error/timeout — resolve pending escape
            pending_esc = false;
            let esc_event = Event::Key(opentui::KeyEvent::key(KeyCode::Esc));
            process_event(
                &esc_event,
                &mut model,
                &mut EventContext {
                    renderer: &mut renderer,
                    raw_guard: &mut raw_guard,
                    wrap_guard: &mut wrap_guard,
                    cursor_guard: &mut cursor_guard,
                    client: &client,
                    repo_path: repo_path.as_deref(),
                    options,
                },
            )?;
        }
    }

    Ok(())
}

struct EventContext<'a> {
    renderer: &'a mut Renderer,
    raw_guard: &'a mut Option<opentui::RawModeGuard>,
    wrap_guard: &'a mut Option<AutoWrapGuard>,
    cursor_guard: &'a mut Option<CursorGuard>,
    client: &'a Option<Box<dyn CritClient>>,
    repo_path: Option<&'a Path>,
    options: RendererOptions,
}

fn process_event(event: &Event, model: &mut Model, ctx: &mut EventContext<'_>) -> Result<()> {
    let _ = ctx.client; // reserved for future use
    let msg = map_event_to_message(model, event);
    let resize = if let Message::Resize { width, height } = &msg {
        Some((*width, *height))
    } else {
        None
    };
    update(model, msg);

    if let Some((width, height)) = resize {
        ctx.renderer
            .resize(width.into(), height.into())
            .context("Failed to resize renderer")?;
        model.needs_redraw = true;
    }

    if let Some(request) = model.pending_editor_request.take() {
        let (prev_width, prev_height) = ctx.renderer.size();
        let prev_width = prev_width as u16;
        let prev_height = prev_height as u16;
        drop(std::mem::replace(
            ctx.renderer,
            Renderer::new_with_options(1, 1, ctx.options).unwrap(),
        ));
        ctx.raw_guard.take();
        ctx.wrap_guard.take();
        ctx.cursor_guard.take();

        let _ = open_file_in_editor(ctx.repo_path, request);

        *ctx.raw_guard = Some(enable_raw_mode().context("Failed to enable raw mode")?);
        let (width, height) = terminal_size().unwrap_or((prev_width, prev_height));
        *ctx.renderer = Renderer::new_with_options(width.into(), height.into(), ctx.options)
            .context("Failed to initialize renderer")?;
        ctx.renderer.set_background(model.theme.background);
        *ctx.wrap_guard = Some(AutoWrapGuard::new().context("Failed to disable line wrap")?);
        *ctx.cursor_guard = Some(CursorGuard::new().context("Failed to hide cursor")?);
        model.resize(width, height);
        model.needs_redraw = true;
        ctx.renderer.invalidate();
    }

    if let Some(request) = model.pending_comment_request.take() {
        let (prev_width, prev_height) = ctx.renderer.size();
        let prev_width = prev_width as u16;
        let prev_height = prev_height as u16;
        drop(std::mem::replace(
            ctx.renderer,
            Renderer::new_with_options(1, 1, ctx.options).unwrap(),
        ));
        ctx.raw_guard.take();
        ctx.wrap_guard.take();
        ctx.cursor_guard.take();

        let comment_result = run_comment_editor(ctx.repo_path, &request);

        // Persist the comment if editor returned content
        if let Ok(Some(body)) = &comment_result {
            if let Some(client) = ctx.client.as_ref() {
                let persist_result = persist_comment(client.as_ref(), ctx.repo_path, &request, body);
                if persist_result.is_ok() {
                    // Refresh review data to show the new comment
                    reload_review_data(model, client.as_ref(), ctx.repo_path);
                }
            }
        }

        *ctx.raw_guard = Some(enable_raw_mode().context("Failed to enable raw mode")?);
        let (width, height) = terminal_size().unwrap_or((prev_width, prev_height));
        *ctx.renderer = Renderer::new_with_options(width.into(), height.into(), ctx.options)
            .context("Failed to initialize renderer")?;
        ctx.renderer.set_background(model.theme.background);
        *ctx.wrap_guard = Some(AutoWrapGuard::new().context("Failed to disable line wrap")?);
        *ctx.cursor_guard = Some(CursorGuard::new().context("Failed to hide cursor")?);
        model.resize(width, height);
        model.needs_redraw = true;
        ctx.renderer.invalidate();
    }

    // Handle inline editor submission (no TUI teardown needed)
    if let Some(submission) = model.pending_comment_submission.take() {
        if let Some(client) = ctx.client.as_ref() {
            let persist_result =
                persist_comment(client.as_ref(), ctx.repo_path, &submission.request, &submission.body);
            match persist_result {
                Ok(()) => reload_review_data(model, client.as_ref(), ctx.repo_path),
                Err(e) => {
                    model.flash_message = Some(format!("Comment failed: {e}"));
                }
            }
        }
        model.needs_redraw = true;
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
        cmd.arg(format!("+{line}"));
    }
    cmd.arg(file_path);
    let _ = cmd.status();
    Ok(())
}

/// Open $EDITOR with a temp file for writing a comment.
/// Returns `Ok(Some(body))` if the user wrote content, `Ok(None)` if cancelled.
fn run_comment_editor(_repo_path: Option<&Path>, request: &CommentRequest) -> Result<Option<String>> {
    use std::io::Read;

    let dir = std::env::temp_dir();
    let tmp_path = dir.join(format!("crit-comment-{}.md", std::process::id()));

    // Build the temp file with context
    {
        let mut f = std::fs::File::create(&tmp_path)
            .context("Failed to create temp file for comment")?;

        // Write context as comments (lines starting with # are stripped later)
        writeln!(f, "# File: {}", request.file_path)?;
        let line_range = match request.end_line {
            Some(end) if end != request.start_line => format!("{}-{}", request.start_line, end),
            _ => request.start_line.to_string(),
        };
        writeln!(f, "# Lines: {line_range}")?;
        if let Some(thread_id) = &request.thread_id {
            writeln!(f, "# Thread: {thread_id}")?;
        }
        if !request.existing_comments.is_empty() {
            writeln!(f, "#")?;
            writeln!(f, "# Existing comments:")?;
            for c in &request.existing_comments {
                writeln!(f, "# {}: {}", c.author, c.body)?;
            }
        }
        writeln!(f, "#")?;
        writeln!(f, "# Write your comment below. Lines starting with # are ignored.")?;
        writeln!(f, "# Save and exit to submit. Leave empty to cancel.")?;
        writeln!(f)?;
        f.flush()?;
    }

    // Determine editor
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor).arg(&tmp_path).status();

    // Read the result
    let body = if let Ok(exit) = status {
        if exit.success() {
            let mut content = String::new();
            std::fs::File::open(&tmp_path)
                .and_then(|mut f| f.read_to_string(&mut content))
                .context("Failed to read temp file after editor")?;

            // Strip comment lines and trim
            let body: String = content
                .lines()
                .filter(|line| !line.starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();

            if body.is_empty() { None } else { Some(body) }
        } else {
            None // Editor exited non-zero → cancel
        }
    } else {
        None
    };

    // Clean up
    let _ = std::fs::remove_file(&tmp_path);

    Ok(body)
}

/// Persist a comment via the crit CLI.
fn persist_comment(
    client: &dyn CritClient,
    _repo_path: Option<&Path>,
    request: &CommentRequest,
    body: &str,
) -> Result<()> {
    if let Some(thread_id) = &request.thread_id {
        client.reply(thread_id, body)?;
    } else {
        client.comment(
            &request.review_id,
            &request.file_path,
            request.start_line,
            request.end_line,
            body,
        )?;
    }
    Ok(())
}

/// Build file cache entries from data returned by crit (no VCS calls needed).
fn populate_file_cache(
    model: &mut Model,
    files: Vec<botcrit_ui::db::FileData>,
) {
    use botcrit_ui::diff::ParsedDiff;

    model.file_cache.clear();

    for file_data in files {
        let diff = file_data.diff.as_deref().map(ParsedDiff::parse);

        let file_content = file_data.content.map(|c| botcrit_ui::model::FileContent {
            lines: c.lines,
            start_line: c.start_line,
        });

        let highlighted_lines = if let Some(parsed) = &diff {
            compute_diff_highlights(parsed, &file_data.path, &model.highlighter)
        } else if let Some(content) = &file_content {
            compute_file_highlights(&content.lines, &file_data.path, &model.highlighter)
        } else {
            Vec::new()
        };

        let file_highlighted_lines = if diff.is_some() {
            if let Some(content) = &file_content {
                compute_file_highlights(&content.lines, &file_data.path, &model.highlighter)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        model.file_cache.insert(
            file_data.path,
            botcrit_ui::model::FileCacheEntry {
                diff,
                file_content,
                highlighted_lines,
                file_highlighted_lines,
            },
        );
    }

    model.sync_active_file_cache();
}

/// Reload review data after a comment is persisted.
fn reload_review_data(model: &mut Model, client: &dyn CritClient, _repo_path: Option<&Path>) {
    let Some(review) = &model.current_review else {
        return;
    };
    let review_id = review.review_id.clone();
    if let Ok(Some(data)) = client.load_review_data(&review_id) {
        model.current_review = Some(data.detail);
        model.threads = data.threads;
        model.all_comments = data.comments;
        populate_file_cache(model, data.files);
    }
}

fn handle_data_loading(model: &mut Model, client: &dyn CritClient, _repo_path: Option<&std::path::Path>) {
    // Load review details when entering detail screen
    if model.screen == Screen::ReviewDetail && model.current_review.is_none() {
        let reviews = model.filtered_reviews();
        if let Some(review) = reviews.get(model.list_index) {
            let review_id = review.review_id.clone();
            if let Ok(Some(data)) = client.load_review_data(&review_id) {
                model.current_review = Some(data.detail);
                model.threads = data.threads;
                model.all_comments = data.comments;
                populate_file_cache(model, data.files);
            }
        }
    }

    // If we're on the detail screen and file cache is empty but we have review data,
    // the cache was already populated by load_review_data above (or a previous call).
    if model.screen == Screen::ReviewDetail && model.current_review.is_some() {
        model.sync_active_file_cache();
        apply_pending_navigation(model);
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

/// Compute stream layout for navigation purposes (mirrors `update.rs::stream_layout`).
fn nav_stream_layout(model: &Model) -> botcrit_ui::stream::StreamLayout {
    const DIFF_MARGIN: u32 = 2;
    let total_width = u32::from(model.width);
    let pane_width = match model.layout_mode {
        LayoutMode::Full | LayoutMode::Compact | LayoutMode::Overlay => {
            if model.sidebar_visible {
                total_width.saturating_sub(u32::from(model.layout_mode.sidebar_width()))
            } else {
                total_width
            }
        }
        LayoutMode::Single => total_width,
    };
    let width = pane_width.saturating_sub(DIFF_MARGIN * 2);
    let files = model.files_with_threads();
    let description = model
        .current_review
        .as_ref()
        .and_then(|r| r.description.as_deref());
    compute_stream_layout(&StreamLayoutParams {
        files: &files,
        file_cache: &model.file_cache,
        threads: &model.threads,
        all_comments: &model.all_comments,
        view_mode: model.diff_view_mode,
        wrap: model.diff_wrap,
        content_width: width,
        description,
    })
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
            reviewers: vec!["security-reviewer".to_string()],
        },
        ReviewSummary {
            review_id: "cr-2f8".to_string(),
            title: "Fix database connection pooling".to_string(),
            author: "bob".to_string(),
            status: "open".to_string(),
            thread_count: 1,
            open_thread_count: 1,
            reviewers: Vec::new(),
        },
        ReviewSummary {
            review_id: "cr-4a1".to_string(),
            title: "Update dependencies to latest versions".to_string(),
            author: "carol".to_string(),
            status: "open".to_string(),
            thread_count: 0,
            open_thread_count: 0,
            reviewers: Vec::new(),
        },
        ReviewSummary {
            review_id: "cr-0b2".to_string(),
            title: "Initial project setup".to_string(),
            author: "alice".to_string(),
            status: "merged".to_string(),
            thread_count: 2,
            open_thread_count: 0,
            reviewers: vec!["api-reviewer".to_string(), "security-reviewer".to_string()],
        },
        ReviewSummary {
            review_id: "cr-1c9".to_string(),
            title: "WIP: Experimental feature".to_string(),
            author: "bob".to_string(),
            status: "abandoned".to_string(),
            thread_count: 0,
            open_thread_count: 0,
            reviewers: Vec::new(),
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
