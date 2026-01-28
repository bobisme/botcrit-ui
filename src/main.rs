//! botcrit-ui - GitHub-style code review TUI
//!
//! Usage: crit-ui [path-to-crit-db]
//!
//! If no path is provided, looks for .crit/index.db in current directory.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use opentui::{
    enable_raw_mode, terminal_size, Event, InputParser, KeyCode, KeyModifiers, Renderer,
    RendererOptions,
};

use botcrit_ui::{update, vcs, view, Db, Message, Model, Screen};

fn main() -> Result<()> {
    let db_path = parse_args()?;

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

    // Get terminal size
    let (width, height) = terminal_size().unwrap_or((80, 24));

    // Create model
    let mut model = Model::new(width as u16, height as u16);

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

    // Input parser
    let mut input = InputParser::new();

    // Main loop
    loop {
        // Check if we need a full redraw
        if model.needs_redraw {
            renderer.invalidate();
            model.needs_redraw = false;
        }

        // Render
        view(&model, renderer.buffer());
        renderer.present().context("Failed to present frame")?;

        if model.should_quit {
            break;
        }

        // Poll for input (with timeout for potential refresh)
        let mut buf = [0u8; 32];
        if let Ok(n) = read_with_timeout(&mut buf, Duration::from_millis(100)) {
            if n > 0 {
                if let Ok((event, _bytes_consumed)) = input.parse(&buf[..n]) {
                    let msg = map_event_to_message(&model, event);
                    update(&mut model, msg);

                    // Handle data loading after navigation
                    if let Some(db) = &db {
                        handle_data_loading(&mut model, db, repo_path.as_deref());
                    } else {
                        // Demo mode - simulate data loading
                        handle_demo_data_loading(&mut model);
                    }
                }
            }
        }
    }

    Ok(())
}

fn parse_args() -> Result<Option<PathBuf>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        if args[1] == "--help" || args[1] == "-h" {
            println!("Usage: crit-ui [path-to-crit-db]");
            println!();
            println!("If no path is provided, looks for .crit/index.db in current directory.");
            println!("If that doesn't exist, runs in demo mode with sample data.");
            std::process::exit(0);
        }
        return Ok(Some(PathBuf::from(&args[1])));
    }

    // Try default location
    let default_path = PathBuf::from(".crit/index.db");
    if default_path.exists() {
        return Ok(Some(default_path));
    }

    // No database found, will use demo data
    Ok(None)
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
                Screen::ReviewDetail => map_review_detail_key(model, key.code),
            }
        }
        Event::Resize(resize) => Message::Resize {
            width: resize.width,
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

fn map_review_detail_key(model: &Model, key: KeyCode) -> Message {
    use botcrit_ui::Focus;

    match model.focus {
        Focus::FileSidebar => match key {
            KeyCode::Char('q') => Message::Quit,
            KeyCode::Esc | KeyCode::Char('h') => Message::Back,
            KeyCode::Tab | KeyCode::Char(' ') => Message::ToggleFocus,
            KeyCode::Char('j') | KeyCode::Down => Message::NextFile,
            KeyCode::Char('k') | KeyCode::Up => Message::PrevFile,
            KeyCode::Enter | KeyCode::Char('l') => Message::ToggleFocus, // Move to diff pane
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

    // Load diff or file content for the currently selected file
    if model.screen == Screen::ReviewDetail {
        if let (Some(repo), Some(review)) = (repo_path, &model.current_review) {
            let files = model.files_with_threads();
            if let Some(file) = files.get(model.file_index) {
                // Check if we need to load new content
                let needs_load =
                    model.current_diff.is_none() && model.current_file_content.is_none();

                if needs_load {
                    // Try to fetch diff from VCS
                    let from = &review.initial_commit;
                    let to = review.final_commit.as_deref();

                    model.current_diff = vcs::get_file_diff(repo, &file.path, from, to);
                    model.diff_scroll = 0;

                    // Compute syntax highlighting for the diff
                    if let Some(diff) = &model.current_diff {
                        model.highlighted_lines =
                            compute_diff_highlights(diff, &file.path, &model.highlighter);
                    } else {
                        model.highlighted_lines.clear();
                    }

                    // If no diff (file didn't change), fetch file content for context
                    if model.current_diff.is_none() {
                        // Use the final commit (or initial if no final) to show current state
                        let commit = to.unwrap_or(from);
                        if let Some(lines) = vcs::get_file_content(repo, &file.path, commit) {
                            model.current_file_content =
                                Some(botcrit_ui::model::FileContent { lines });
                            // Compute highlights for file content
                            model.highlighted_lines = compute_file_highlights(
                                &model.current_file_content.as_ref().unwrap().lines,
                                &file.path,
                                &model.highlighter,
                            );
                        }
                    }
                }
            }
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

    // Load diff for the currently selected file
    if model.screen == Screen::ReviewDetail {
        let files = model.files_with_threads();
        if let Some(file) = files.get(model.file_index) {
            // Check if we need to load a new diff
            let needs_load = model
                .current_diff
                .as_ref()
                .map_or(true, |d| d.file_b.as_deref() != Some(&file.path));

            if needs_load {
                model.current_diff = get_demo_diff(&file.path);
                model.diff_scroll = 0; // Reset scroll when changing files
            }
        }
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
