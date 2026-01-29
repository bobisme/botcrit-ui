//! botcrit-ui - GitHub-style code review TUI
//!
//! Usage: crit-ui [path-to-crit-db]
//!
//! If no path is provided, looks for .crit/index.db in current directory.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use opentui::input::ParseError;
use opentui::{
    enable_raw_mode, terminal_size, Event, InputParser, KeyCode, KeyModifiers, Renderer,
    RendererOptions,
};

use botcrit_ui::config::{load_ui_config, save_ui_config};
use botcrit_ui::theme::{load_built_in_theme, load_theme_from_path};
use botcrit_ui::{update, vcs, view, Db, Highlighter, Message, Model, Screen, Theme};

fn main() -> Result<()> {
    let args = parse_args()?;
    let db_path = args.db_path;

    // Determine repo root from database path
    // Database is at <repo>/.crit/index.db, so repo is grandparent
    // Canonicalize first to handle relative paths like ".crit/index.db"
    let repo_path = db_path.as_ref().and_then(|p| {
        p.canonicalize().ok().and_then(|canonical| {
            canonical
                .parent() // .crit/
                .and_then(|crit| crit.parent()) // repo root
                .map(|p| p.to_path_buf())
        })
    });

    // Open database if provided
    let db = if let Some(path) = &db_path {
        Some(
            Db::open(path)
                .with_context(|| format!("Failed to open database: {}", path.display()))?,
        )
    } else {
        None
    };

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
    let (term_width, height) = terminal_size().unwrap_or((80, 24));
    let width = term_width.saturating_sub(2).max(1);

    // Create model
    let mut model = Model::new(width as u16, height as u16);
    model.theme = theme;
    if let Some(theme_name) = syntax_theme {
        model.highlighter = Highlighter::with_theme(&theme_name);
    } else if model.theme.name.to_lowercase().contains("light") {
        model.highlighter = Highlighter::with_theme("base16-ocean.light");
    }

    // Load initial data
    if let Some(db) = &db {
        model.reviews = db.list_reviews(None).unwrap_or_default();
    } else {
        // Demo data for testing without a database
        load_demo_data(&mut model);
    }

    // Enter raw mode for input handling
    let _raw_guard = enable_raw_mode().context("Failed to enable raw mode")?;

    // Initialize renderer
    let options = RendererOptions {
        use_alt_screen: true,
        hide_cursor: true,
        enable_mouse: false,
        query_capabilities: false,
    };
    let mut renderer = Renderer::new_with_options(width.into(), height.into(), options)
        .context("Failed to initialize renderer")?;
    let _wrap_guard = AutoWrapGuard::new().context("Failed to disable line wrap")?;
    renderer.set_background(model.theme.background);

    // Input parser
    let mut input = InputParser::new();

    // Main loop
    loop {
        // Detect external terminal resize even if no input events are received
        if let Ok((term_width, term_height)) = terminal_size() {
            let ui_width = term_width.saturating_sub(2).max(1);
            let term_width_u16 = ui_width as u16;
            let term_height_u16 = term_height as u16;
            if term_width_u16 != model.width || term_height_u16 != model.height {
                model.resize(term_width_u16, term_height_u16);
                model.needs_redraw = true;
                renderer
                    .resize(ui_width.into(), term_height.into())
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

        // Poll for input (with timeout for potential refresh)
        let mut buf = [0u8; 32];
        if let Ok(n) = read_with_timeout(&mut buf, Duration::from_millis(100)) {
            if n > 0 {
                let mut offset = 0usize;
                while offset < n {
                    match input.parse(&buf[offset..n]) {
                        Ok((event, consumed)) => {
                            offset = offset.saturating_add(consumed);
                            let msg = map_event_to_message(&model, event);
                            let resize = if let Message::Resize { width, height } = msg {
                                Some((width, height))
                            } else {
                                None
                            };
                            update(&mut model, msg);

                            if let Some((width, height)) = resize {
                                renderer
                                    .resize(width.into(), height.into())
                                    .context("Failed to resize renderer")?;
                                model.needs_redraw = true;
                            }

                            // Handle data loading after navigation
                            if let Some(db) = &db {
                                handle_data_loading(&mut model, db, repo_path.as_deref());
                            } else {
                                // Demo mode - simulate data loading
                                handle_demo_data_loading(&mut model);
                            }
                        }
                        Err(ParseError::Empty) | Err(ParseError::Incomplete) => break,
                        Err(_) => {
                            offset = offset.saturating_add(1);
                        }
                    }
                }
            }
        }
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

struct CliArgs {
    db_path: Option<PathBuf>,
    theme: Option<String>,
}

fn parse_args() -> Result<CliArgs> {
    let args: Vec<String> = std::env::args().collect();
    let mut db_path: Option<PathBuf> = None;
    let mut theme: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                println!("Usage: crit-ui [options] [path-to-crit-db]");
                println!();
                println!("Options:");
                println!("  --theme <name|path>   Load theme by name or JSON path");
                println!("  --db <path>      Path to .crit/index.db");
                println!();
                println!("Environment:");
                println!("  BOTCRIT_UI_THEME  Theme name or JSON path");
                println!();
                println!(
                    "If no DB path is provided, looks for .crit/index.db in current directory."
                );
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
            "--db" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--db requires a path");
                }
                db_path = Some(PathBuf::from(&args[i]));
            }
            arg if arg.starts_with('-') => {
                anyhow::bail!("Unknown option: {arg}");
            }
            arg => {
                if db_path.is_none() {
                    db_path = Some(PathBuf::from(arg));
                } else {
                    anyhow::bail!("Unexpected argument: {arg}");
                }
            }
        }
        i += 1;
    }

    // Try default location if no explicit DB path
    if db_path.is_none() {
        let default_path = PathBuf::from(".crit/index.db");
        if default_path.exists() {
            db_path = Some(default_path);
        }
    }

    Ok(CliArgs { db_path, theme })
}

fn map_event_to_message(model: &Model, event: Event) -> Message {
    match event {
        Event::Key(key) => {
            // Check for Ctrl+C to quit
            if key.modifiers.contains(KeyModifiers::CTRL) && key.code == KeyCode::Char('c') {
                return Message::Quit;
            }

            match model.screen {
                Screen::ReviewList => map_review_list_key(key.code, model),
                Screen::ReviewDetail => map_review_detail_key(model, key.code, key.modifiers),
            }
        }
        Event::Resize(resize) => Message::Resize {
            width: resize.width.saturating_sub(2).max(1),
            height: resize.height,
        },
        Event::Mouse(_) => Message::Noop,
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
            KeyCode::Tab | KeyCode::Char(' ') => Message::ToggleFocus,
            KeyCode::Char('j') | KeyCode::Down => Message::NextFile,
            KeyCode::Char('k') | KeyCode::Up => Message::PrevFile,
            KeyCode::Enter | KeyCode::Char('l') => Message::ToggleFocus, // Move to diff pane
            KeyCode::Char('s') => Message::ToggleSidebar,
            _ => Message::Noop,
        },
        Focus::DiffPane => match key {
            KeyCode::Char('q') => Message::Quit,
            KeyCode::Esc => Message::Back,
            KeyCode::Tab | KeyCode::Char(' ') => Message::ToggleFocus,
            KeyCode::Char('j') | KeyCode::Down => Message::ScrollDown,
            KeyCode::Char('k') | KeyCode::Up => Message::ScrollUp,
            KeyCode::Char('n') => Message::NextThread,
            KeyCode::Char('p') | KeyCode::Char('N') => Message::PrevThread,
            KeyCode::Char('v') => Message::ToggleDiffView, // Toggle unified/side-by-side
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
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if let Some(id) = &model.expanded_thread {
                    Message::ResolveThread(id.clone())
                } else {
                    Message::Noop
                }
            }
            _ => Message::Noop,
        },
        _ => Message::Noop,
    }
}

fn handle_data_loading(model: &mut Model, db: &Db, repo_path: Option<&std::path::Path>) {
    // Load review details when entering detail screen
    if model.screen == Screen::ReviewDetail && model.current_review.is_none() {
        let reviews = model.filtered_reviews();
        if let Some(review) = reviews.get(model.list_index) {
            let review_id = review.review_id.clone();
            if let Ok(Some(detail)) = db.get_review(&review_id) {
                model.current_review = Some(detail);
            }
            if let Ok(threads) = db.list_threads(&review_id, None, None) {
                model.threads = threads;
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
                let highlighted_lines = if let Some(parsed) = &diff {
                    compute_diff_highlights(parsed, &file.path, &model.highlighter)
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
                    },
                );
            }

            model.sync_active_file_cache();
        }
    }

    // Load comments when thread is expanded
    if let Some(thread_id) = &model.expanded_thread {
        if model.comments.is_empty()
            || model.current_thread.as_ref().map(|t| &t.thread_id) != Some(thread_id)
        {
            if let Ok(Some(thread)) = db.get_thread(thread_id) {
                model.current_thread = Some(thread);
            }
            if let Ok(comments) = db.list_comments(thread_id) {
                model.comments = comments;
            }
        }
    }
}

fn handle_demo_data_loading(model: &mut Model) {
    use botcrit_ui::db::ReviewDetail;

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
                },
            );
        }

        model.sync_active_file_cache();
    }
}

fn load_demo_data(model: &mut Model) {
    use botcrit_ui::db::{ReviewSummary, ThreadSummary};

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

    // Demo threads for when a review is selected
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
