# botcrit-ui: GitHub-style Code Review TUI

A terminal-based code review interface for [botcrit](https://github.com/anomalyco/botcrit), built with [opentui_rust](https://github.com/anomalyco/opentui_rust).

## Overview

botcrit-ui provides a GitHub-like review experience in the terminal:
- Browse open/closed reviews
- View diffs with inline comment threads
- Navigate files with thread indicators
- Respond to and resolve comment threads

## Architecture

### Elm Architecture (TEA)

The application follows The Elm Architecture pattern:

```rust
/// Application state
struct Model {
    screen: Screen,
    reviews: Vec<ReviewSummary>,
    current_review: Option<ReviewDetail>,
    threads: Vec<ThreadSummary>,
    current_thread: Option<ThreadDetail>,
    
    // UI state
    list_scroll: usize,
    selected_file_idx: usize,
    diff_scroll: usize,
    focus: Focus,
    
    // Layout
    terminal_size: (u16, u16),
    layout_mode: LayoutMode,
    
    // Data source
    db: ProjectionDb,
}

/// All possible user actions and events
enum Message {
    // Navigation
    SelectReview(String),
    Back,
    
    // List navigation
    ListUp,
    ListDown,
    ListPageUp,
    ListPageDown,
    ListTop,
    ListBottom,
    
    // File sidebar
    NextFile,
    PrevFile,
    SelectFile(usize),
    
    // Diff/Thread navigation
    ScrollUp,
    ScrollDown,
    NextThread,
    PrevThread,
    ExpandThread(String),
    CollapseThread,
    
    // Actions
    ResolveThread(String),
    ReopenThread(String),
    AddComment { thread_id: String, body: String },
    
    // System
    Resize(u16, u16),
    Tick,
    Quit,
}

/// Pure state transition
fn update(model: &mut Model, msg: Message) -> Option<Command> {
    match msg {
        Message::SelectReview(id) => {
            model.current_review = model.db.get_review(&id).ok().flatten();
            model.threads = model.db.list_threads(&id, None, None).unwrap_or_default();
            model.screen = Screen::ReviewDetail;
            model.selected_file_idx = 0;
            model.diff_scroll = 0;
            None
        }
        Message::Back => {
            match model.screen {
                Screen::ReviewDetail => {
                    model.screen = Screen::ReviewList;
                    model.current_review = None;
                }
                _ => {}
            }
            None
        }
        // ... other handlers
    }
}

/// Render current state to buffer
fn view(model: &Model, buffer: &mut OptimizedBuffer) {
    match model.screen {
        Screen::ReviewList => view_review_list(model, buffer),
        Screen::ReviewDetail => view_review_detail(model, buffer),
    }
}
```

### Screens

```rust
enum Screen {
    ReviewList,    // Main list of reviews
    ReviewDetail,  // Diff view with file sidebar and inline threads
}

enum Focus {
    ReviewList,
    FileSidebar,
    DiffPane,
    ThreadExpanded,
}
```

## Data Model

### From botcrit library

```rust
// Query API
ProjectionDb::list_reviews(status, author) -> Vec<ReviewSummary>
ProjectionDb::get_review(id) -> Option<ReviewDetail>
ProjectionDb::list_threads(review_id, status, file) -> Vec<ThreadSummary>
ProjectionDb::get_thread(id) -> Option<ThreadDetail>
ProjectionDb::list_comments(thread_id) -> Vec<Comment>

// Key types
ReviewSummary { review_id, title, author, status, thread_count, open_thread_count }
ReviewDetail { ..., jj_change_id, description, reviewers, votes }
ThreadSummary { thread_id, file_path, selection_start, selection_end, status, comment_count }
ThreadDetail { ..., comments: Vec<Comment> }
Comment { comment_id, author, body, created_at }

// Status values
Review: "open" | "approved" | "merged" | "abandoned"
Thread: "open" | "resolved"
Vote: "lgtm" | "block"
```

### View Model (derived from botcrit types)

```rust
/// Files with thread counts for sidebar
struct FileEntry {
    path: String,
    open_threads: usize,
    resolved_threads: usize,
}

/// Parsed diff with thread anchoring
struct DiffView {
    file_path: String,
    hunks: Vec<DiffHunk>,
}

struct DiffHunk {
    header: String,  // @@ -1,5 +1,7 @@
    lines: Vec<DiffLine>,
}

struct DiffLine {
    kind: DiffLineKind,
    old_line: Option<u32>,
    new_line: Option<u32>,
    content: String,
    threads: Vec<ThreadSummary>,  // Threads anchored to this line
}

enum DiffLineKind {
    Context,
    Added,
    Removed,
}
```

## UI Layout

### Review List Screen

```
┌─ Reviews ─────────────────────────────────────────────────────────────────┐
│                                                                           │
│  OPEN                                                                     │
│  ▸ cr-1d3  Add user authentication       alice   3 threads (2 open)      │
│    cr-2f8  Fix database connection       bob     1 thread                │
│    cr-4a1  Update dependencies           carol   0 threads               │
│                                                                           │
│  CLOSED                                                                   │
│    cr-0b2  [merged] Initial setup        alice   2 threads (resolved)    │
│    cr-1c9  [abandoned] WIP feature       bob     0 threads               │
│                                                                           │
├───────────────────────────────────────────────────────────────────────────┤
│  j/k navigate  Enter select  q quit                                       │
└───────────────────────────────────────────────────────────────────────────┘
```

### Review Detail Screen

```
┌─ cr-1d3: Add user authentication ─────────────────────────────────────────┐
│ ┌─ Files ────────┐ ┌─ src/auth.rs ────────────────────────────────────────┤
│ │ ▸ src/auth.rs 2│ │  10   use crate::db::User;                          │
│ │   src/main.rs 1│ │  11                                                  │
│ │   Cargo.toml  0│ │  12 + pub fn authenticate(user: &str, pass: &str)   │
│ │                │ │  13 +     -> Result<Session> {                       │
│ │                │ │  14 +     let user = db.find_user(user)?;            │
│ │                │ │      ┌─────────────────────────────────────────────┐ │
│ │                │ │      │ alice: Should we hash the password here?    │ │
│ │                │ │      │ bob: Good point, I'll add bcrypt            │ │
│ │                │ │      │ [reply] [resolve]                           │ │
│ │                │ │      └─────────────────────────────────────────────┘ │
│ │                │ │  15 +     verify_password(&user, pass)?;             │
│ │                │ │  16 +     Ok(Session::new(user))                     │
│ │                │ │  17 + }                                              │
│ │                │ │  18                                                  │
├─┴────────────────┴─┴──────────────────────────────────────────────────────┤
│  Tab files  j/k scroll  n/p thread  Enter expand  Esc back  q quit        │
└───────────────────────────────────────────────────────────────────────────┘
```

### Responsive Breakpoints

```rust
enum LayoutMode {
    Full,      // >= 120 cols: sidebar + full diff
    Compact,   // 90-119 cols: narrow sidebar + diff
    Overlay,   // 70-89 cols: overlay sidebar (toggle)
    Single,    // < 70 cols: single pane (switch between views)
}

fn layout_mode(width: u16) -> LayoutMode {
    match width {
        w if w >= 120 => LayoutMode::Full,
        w if w >= 90 => LayoutMode::Compact,
        w if w >= 70 => LayoutMode::Overlay,
        _ => LayoutMode::Single,
    }
}

// Sidebar widths
const SIDEBAR_FULL: u16 = 20;
const SIDEBAR_COMPACT: u16 = 14;
```

## Diff Rendering

### Parsing Unified Diff

```rust
fn parse_unified_diff(diff: &str) -> Vec<DiffHunk> {
    // Parse git diff output:
    // @@ -start,count +start,count @@
    // context line
    // +added line
    // -removed line
}
```

### Thread Anchoring

Threads are anchored to line numbers in a specific commit. To display:

1. Parse the unified diff to build line number mappings
2. Map thread's `selection_start`/`selection_end` to diff line positions
3. Use botcrit's `calculate_drift()` for threads on older commits
4. Render thread bubbles inline after the anchored line

```rust
fn anchor_threads_to_diff(
    hunks: &mut [DiffHunk],
    threads: &[ThreadSummary],
    current_commit: &str,
    repo: &JjRepo,
) {
    for thread in threads {
        // Calculate drift from thread's commit to current
        let drift = calculate_drift(
            repo,
            &thread.file_path,
            thread.selection_start as u32,
            &thread.commit_hash,  // from ThreadDetail
            current_commit,
        );
        
        if let Some(current_line) = drift.current_line() {
            // Find the diff line and attach thread
            if let Some(line) = find_diff_line(hunks, current_line) {
                line.threads.push(thread.clone());
            }
        }
    }
}
```

### Diff Colors (Theme Tokens)

```rust
struct DiffTheme {
    // Text colors
    added: Rgba,           // Added line text
    removed: Rgba,         // Removed line text
    context: Rgba,         // Context line text
    hunk_header: Rgba,     // @@ header text
    
    // Highlight colors (for +/- signs)
    highlight_added: Rgba,
    highlight_removed: Rgba,
    
    // Backgrounds
    added_bg: Rgba,
    removed_bg: Rgba,
    context_bg: Rgba,
    
    // Line number area
    line_number: Rgba,
    added_line_number_bg: Rgba,
    removed_line_number_bg: Rgba,
}
```

### Diff View Modes

```rust
enum DiffViewMode {
    Unified,  // Single column, +/- prefixed
    Split,    // Side-by-side (when terminal >= 120 cols)
}

// Auto-select based on width
fn diff_view_mode(width: u16, user_pref: Option<DiffViewMode>) -> DiffViewMode {
    match user_pref {
        Some(pref) => pref,
        None if width >= 120 => DiffViewMode::Split,
        None => DiffViewMode::Unified,
    }
}
```

## Theme System

### Theme Schema

```rust
struct Theme {
    name: String,
    
    // Base colors
    background: Rgba,
    foreground: Rgba,
    
    // UI chrome
    border: Rgba,
    border_focused: Rgba,
    panel_bg: Rgba,
    
    // Selection/highlighting
    selection_bg: Rgba,
    selection_fg: Rgba,
    cursor: Rgba,
    
    // Semantic colors
    primary: Rgba,      // Accent color
    success: Rgba,      // Green (approved, resolved)
    warning: Rgba,      // Yellow (needs attention)
    error: Rgba,        // Red (blocked, failed)
    muted: Rgba,        // Dim text
    
    // Diff colors (see DiffTheme above)
    diff: DiffTheme,
    
    // Syntax highlighting (optional)
    syntax: Option<SyntaxTheme>,
}
```

### JSON Theme Format

```json
{
  "$schema": "https://botcrit.dev/theme.json",
  "name": "default-dark",
  "colors": {
    "background": "#1a1a2e",
    "foreground": "#e0e0e0",
    "border": "#3a3a4e",
    "borderFocused": "#5a5a7e",
    "panelBg": "#252538",
    
    "selectionBg": "#3a3a5e",
    "selectionFg": "#ffffff",
    "cursor": "#f0f0f0",
    
    "primary": "#7aa2f7",
    "success": "#9ece6a",
    "warning": "#e0af68",
    "error": "#f7768e",
    "muted": "#565f89",
    
    "diffAdded": "#9ece6a",
    "diffRemoved": "#f7768e",
    "diffContext": "#a9b1d6",
    "diffHunkHeader": "#565f89",
    "diffHighlightAdded": "#73daca",
    "diffHighlightRemoved": "#ff7a93",
    "diffAddedBg": "#1a2f1a",
    "diffRemovedBg": "#2f1a1a",
    "diffContextBg": "#1a1a2e",
    "diffLineNumber": "#565f89",
    "diffAddedLineNumberBg": "#152515",
    "diffRemovedLineNumberBg": "#251515"
  }
}
```

### Theme Loading

```rust
fn load_theme(path: &Path) -> Result<Theme> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json)
}

fn default_dark_theme() -> Theme { /* ... */ }
fn default_light_theme() -> Theme { /* ... */ }
```

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `q` | Quit |
| `Esc` | Back / Cancel |
| `?` | Show help |
| `Ctrl+L` | Redraw |

### Review List

| Key | Action |
|-----|--------|
| `j` / `Down` | Next review |
| `k` / `Up` | Previous review |
| `g` | Go to first |
| `G` | Go to last |
| `Enter` | Open review |
| `o` | Filter: open only |
| `a` | Filter: all |

### Review Detail

| Key | Action |
|-----|--------|
| `Tab` | Toggle focus (sidebar <-> diff) |
| `j` / `Down` | Scroll down / Next file |
| `k` / `Up` | Scroll up / Previous file |
| `n` | Next thread |
| `p` / `N` | Previous thread |
| `Enter` | Expand thread |
| `r` | Reply to thread |
| `R` | Resolve thread |
| `u` | Unresolve thread |
| `[` / `]` | Previous / Next file |
| `Ctrl+D` | Page down |
| `Ctrl+U` | Page up |

## Commands (Side Effects)

```rust
enum Command {
    /// Load reviews from database
    LoadReviews,
    
    /// Load review detail
    LoadReview(String),
    
    /// Load threads for a review
    LoadThreads(String),
    
    /// Resolve a thread
    ResolveThread { thread_id: String, reason: Option<String> },
    
    /// Reopen a thread
    ReopenThread { thread_id: String, reason: Option<String> },
    
    /// Add a comment
    AddComment { thread_id: String, body: String },
    
    /// Get diff for a file
    GetDiff { file_path: String, commit: String },
}

/// Execute command (may be async)
async fn execute(cmd: Command, db: &ProjectionDb) -> Message {
    match cmd {
        Command::LoadReviews => {
            let reviews = db.list_reviews(None, None)?;
            Message::ReviewsLoaded(reviews)
        }
        // ...
    }
}
```

## Rendering with opentui_rust

### Buffer Management

```rust
use opentui::{Renderer, OptimizedBuffer, Style, Rgba};
use opentui::buffer::{BoxStyle, ClipRect};

fn main() -> Result<()> {
    let mut renderer = Renderer::new(80, 24)?;
    let mut model = Model::new()?;
    
    loop {
        // Handle input -> Message
        if let Some(msg) = poll_input()? {
            if let Some(cmd) = update(&mut model, msg) {
                let result_msg = execute(cmd, &model.db).await;
                update(&mut model, result_msg);
            }
        }
        
        // Render
        let buffer = renderer.buffer();
        buffer.clear(model.theme.background);
        view(&model, buffer);
        renderer.present()?;
        
        if model.should_quit {
            break;
        }
    }
    
    Ok(())
}
```

### Drawing Components

```rust
fn draw_review_list(model: &Model, buffer: &mut OptimizedBuffer, area: Rect) {
    let theme = &model.theme;
    
    // Draw border
    buffer.draw_box(
        area.x, area.y, area.width, area.height,
        BoxStyle::rounded(Style::fg(theme.border)),
    );
    
    // Title
    buffer.draw_text(
        area.x + 2, area.y,
        " Reviews ",
        Style::fg(theme.foreground).with_bold(),
    );
    
    // List items with scissor clipping
    buffer.push_scissor(ClipRect::new(
        area.x + 1, area.y + 1,
        area.width - 2, area.height - 2,
    ));
    
    for (i, review) in model.reviews.iter().enumerate() {
        let y = area.y + 1 + i as i32;
        let is_selected = i == model.list_scroll;
        
        let style = if is_selected {
            Style::fg(theme.foreground).with_bg(theme.selection_bg)
        } else {
            Style::fg(theme.foreground)
        };
        
        buffer.draw_text(area.x + 2, y, &review.title, style);
    }
    
    buffer.pop_scissor();
}

fn draw_diff_line(
    buffer: &mut OptimizedBuffer,
    line: &DiffLine,
    y: i32,
    area: Rect,
    theme: &DiffTheme,
) {
    let (bg, fg, sign) = match line.kind {
        DiffLineKind::Added => (theme.added_bg, theme.added, "+"),
        DiffLineKind::Removed => (theme.removed_bg, theme.removed, "-"),
        DiffLineKind::Context => (theme.context_bg, theme.context, " "),
    };
    
    // Fill background
    buffer.fill_rect(area.x, y, area.width, 1, bg);
    
    // Line number gutter
    let ln_bg = match line.kind {
        DiffLineKind::Added => theme.added_line_number_bg,
        DiffLineKind::Removed => theme.removed_line_number_bg,
        DiffLineKind::Context => theme.context_bg,
    };
    buffer.fill_rect(area.x, y, 6, 1, ln_bg);
    
    if let Some(ln) = line.new_line.or(line.old_line) {
        buffer.draw_text(
            area.x, y,
            &format!("{:>5}", ln),
            Style::fg(theme.line_number),
        );
    }
    
    // Sign with highlight color
    let sign_color = match line.kind {
        DiffLineKind::Added => theme.highlight_added,
        DiffLineKind::Removed => theme.highlight_removed,
        DiffLineKind::Context => theme.context,
    };
    buffer.draw_text(area.x + 6, y, sign, Style::fg(sign_color));
    
    // Content
    buffer.draw_text(area.x + 8, y, &line.content, Style::fg(fg));
}
```

## File Structure

```
botcrit-ui/
├── Cargo.toml
├── src/
│   ├── main.rs           # Entry point, event loop
│   ├── lib.rs            # Library exports
│   ├── model.rs          # Model struct and state
│   ├── message.rs        # Message enum
│   ├── update.rs         # State transitions
│   ├── view/
│   │   ├── mod.rs        # View dispatch
│   │   ├── review_list.rs
│   │   ├── review_detail.rs
│   │   ├── diff.rs       # Diff rendering
│   │   ├── thread.rs     # Thread bubble rendering
│   │   └── components.rs # Reusable UI components
│   ├── command.rs        # Side effects
│   ├── diff/
│   │   ├── mod.rs
│   │   ├── parse.rs      # Unified diff parser
│   │   └── anchor.rs     # Thread anchoring
│   ├── theme/
│   │   ├── mod.rs
│   │   ├── schema.rs     # Theme types
│   │   └── default.rs    # Built-in themes
│   ├── input.rs          # Keyboard handling
│   └── layout.rs         # Responsive layout calculation
├── themes/
│   ├── default-dark.json
│   └── default-light.json
└── SPEC.md
```

## Dependencies

```toml
[dependencies]
botcrit = { path = "../botcrit" }
opentui = { git = "https://github.com/anomalyco/opentui_rust" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
crossterm = "0.27"  # For input handling
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
anyhow = "1.0"
```

## Future Enhancements

- [ ] Syntax highlighting for diff content
- [ ] Split diff view mode
- [ ] Mouse support (click on threads, scroll)
- [ ] Search within diff
- [ ] Review creation (new review from current change)
- [ ] Inline comment creation (select lines -> new thread)
- [ ] Configurable keybindings
- [ ] Multiple theme support with hot-reload
