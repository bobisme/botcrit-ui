//! Application state model

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::command::CommandSpec;
use crate::config::UiConfig;
use crate::db::{Comment, ReviewDetail, ReviewSummary, ThreadDetail, ThreadSummary};
use crate::diff::ParsedDiff;
use crate::syntax::{HighlightSpan, Highlighter};
use crate::theme::Theme;

/// File content for displaying context when no diff is available
#[derive(Debug, Clone)]
pub struct FileContent {
    pub lines: Vec<String>,
}

/// Cached data for a file in the review stream
pub struct FileCacheEntry {
    pub diff: Option<ParsedDiff>,
    pub file_content: Option<FileContent>,
    pub highlighted_lines: Vec<Vec<HighlightSpan>>,
    /// Syntax highlights indexed by file line number (for orphaned thread context).
    /// Only populated when both `diff` and `file_content` are present.
    pub file_highlighted_lines: Vec<Vec<HighlightSpan>>,
}

/// Current screen/view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    ReviewList,
    ReviewDetail,
}

/// Which pane has focus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    ReviewList,
    FileSidebar,
    DiffPane,
    ThreadExpanded,
    CommandPalette,
    Commenting,
}

/// What the command palette is showing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaletteMode {
    #[default]
    Commands,
    Themes,
}

/// Responsive layout mode based on terminal width
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// >= 120 cols: full sidebar + diff
    Full,
    /// 90-119 cols: compact sidebar + diff
    Compact,
    /// 70-89 cols: overlay sidebar (toggleable)
    Overlay,
    /// < 70 cols: single pane mode
    Single,
}

/// Diff view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffViewMode {
    /// Traditional unified diff (default)
    #[default]
    Unified,
    /// Side-by-side diff (old left, new right)
    SideBySide,
}

#[derive(Debug, Clone)]
pub struct EditorRequest {
    pub file_path: String,
    pub line: Option<u32>,
}

impl LayoutMode {
    /// Determine layout mode from terminal width
    #[must_use]
    pub const fn from_width(width: u16) -> Self {
        match width {
            w if w >= 130 => Self::Full,
            w if w >= 100 => Self::Compact,
            w if w >= 80 => Self::Overlay,
            _ => Self::Single,
        }
    }

    /// Get sidebar width for this layout mode
    #[must_use]
    pub const fn sidebar_width(self) -> u16 {
        match self {
            Self::Full => 28,
            Self::Compact => 24,
            Self::Overlay => 22,
            Self::Single => 0,
        }
    }
}

/// Filter for review list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewFilter {
    #[default]
    All,
    Open,
    Closed,
}

/// Application state
#[allow(clippy::struct_excessive_bools)] // TUI state inherently needs many boolean flags
pub struct Model {
    // === Screen state ===
    pub screen: Screen,
    pub focus: Focus,
    pub previous_focus: Option<Focus>,

    // === Data ===
    pub reviews: Vec<ReviewSummary>,
    pub current_review: Option<ReviewDetail>,
    pub threads: Vec<ThreadSummary>,
    pub current_thread: Option<ThreadDetail>,
    pub all_comments: HashMap<String, Vec<Comment>>,
    /// Parsed diff for the currently selected file
    pub current_diff: Option<ParsedDiff>,
    /// File content for context when no diff available
    pub current_file_content: Option<FileContent>,
    /// Cache for all files in the review stream
    pub file_cache: HashMap<String, FileCacheEntry>,
    /// Syntax highlighter
    pub highlighter: Highlighter,
    /// Cached highlighted lines for current diff (indexed by display line)
    pub highlighted_lines: Vec<Vec<HighlightSpan>>,

    // === UI state ===
    /// Selected index in review list
    pub list_index: usize,
    /// Scroll offset in review list
    pub list_scroll: usize,
    /// Selected file index in sidebar
    pub file_index: usize,
    /// Selected index in the flat sidebar tree
    pub sidebar_index: usize,
    /// Scroll offset for sidebar tree
    pub sidebar_scroll: usize,
    /// Files whose thread children are collapsed
    pub collapsed_files: HashSet<String>,
    /// Scroll offset in diff pane
    pub diff_scroll: usize,
    /// Currently expanded thread ID
    pub expanded_thread: Option<String>,
    /// Review list filter
    pub filter: ReviewFilter,
    /// Show sidebar in overlay mode
    pub sidebar_visible: bool,
    /// Diff view mode (unified or side-by-side)
    pub diff_view_mode: DiffViewMode,
    /// Wrap diff lines when enabled
    pub diff_wrap: bool,
    /// Pending editor launch request
    pub pending_editor_request: Option<EditorRequest>,

    // === Command Palette ===
    pub command_palette_input: String,
    pub command_palette_selection: usize,
    pub command_palette_commands: Vec<CommandSpec>,
    pub command_palette_mode: PaletteMode,

    // === Commenting State ===
    pub comment_input: String,
    pub comment_target_line: Option<u32>,

    // === Layout ===
    pub width: u16,
    pub height: u16,
    pub layout_mode: LayoutMode,

    // === Theme ===
    pub theme: Theme,
    /// Theme name before opening the picker (for revert on Esc)
    pub pre_palette_theme: Option<String>,
    pub config: UiConfig,

    // === Render-computed data ===
    /// Thread positions captured during rendering (`thread_id` → `stream_row`)
    pub thread_positions: RefCell<HashMap<String, usize>>,

    // === Review list search ===
    pub search_input: String,
    pub search_active: bool,

    // === Repo path for display ===
    pub repo_path: Option<String>,

    // === Control ===
    pub should_quit: bool,
    /// Flag indicating the view needs a full redraw
    pub needs_redraw: bool,

    // === Input state ===
    pub last_list_scroll: Option<(Instant, i8)>,
    pub last_sidebar_scroll: Option<(Instant, i8)>,

    // === Pending CLI navigation targets ===
    pub pending_review: Option<String>,
    pub pending_file: Option<String>,
    pub pending_thread: Option<String>,
}

impl Model {
    /// Create a new model
    #[must_use]
    pub fn new(width: u16, height: u16, config: UiConfig) -> Self {
        Self {
            screen: Screen::default(),
            focus: Focus::default(),
            previous_focus: None,
            reviews: Vec::new(),
            current_review: None,
            threads: Vec::new(),
            current_thread: None,
            all_comments: HashMap::new(),
            current_diff: None,
            current_file_content: None,
            file_cache: HashMap::new(),
            highlighter: Highlighter::new(),
            highlighted_lines: Vec::new(),
            list_index: 0,
            list_scroll: 0,
            file_index: 0,
            sidebar_index: 0,
            sidebar_scroll: 0,
            collapsed_files: HashSet::new(),
            diff_scroll: 0,
            expanded_thread: None,
            filter: ReviewFilter::default(),
            sidebar_visible: true,
            diff_view_mode: DiffViewMode::default(),
            diff_wrap: true,
            pending_editor_request: None,
            command_palette_input: String::new(),
            command_palette_selection: 0,
            command_palette_commands: Vec::new(),
            command_palette_mode: PaletteMode::default(),
            comment_input: String::new(),
            comment_target_line: None,
            width,
            height,
            layout_mode: LayoutMode::from_width(width),
            theme: Theme::default(),
            pre_palette_theme: None,
            config,
            thread_positions: RefCell::new(HashMap::new()),
            search_input: String::new(),
            search_active: false,
            repo_path: None,
            should_quit: false,
            needs_redraw: true,
            last_list_scroll: None,
            last_sidebar_scroll: None,
            pending_review: None,
            pending_file: None,
            pending_thread: None,
        }
    }

    /// Get filtered reviews based on current filter and search query
    #[must_use]
    pub fn filtered_reviews(&self) -> Vec<&ReviewSummary> {
        let status_filtered: Vec<&ReviewSummary> = match self.filter {
            ReviewFilter::All => self.reviews.iter().collect(),
            ReviewFilter::Open => self.reviews.iter().filter(|r| r.status == "open").collect(),
            ReviewFilter::Closed => self
                .reviews
                .iter()
                .filter(|r| r.status != "open")
                .collect(),
        };
        if self.search_input.is_empty() {
            return status_filtered;
        }
        let query = self.search_input.to_lowercase();
        status_filtered
            .into_iter()
            .filter(|r| {
                r.title.to_lowercase().contains(&query)
                    || r.review_id.to_lowercase().contains(&query)
                    || r.author.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Get unique files from threads for the sidebar
    #[must_use]
    pub fn files_with_threads(&self) -> Vec<FileEntry> {
        use std::collections::HashMap;

        let mut files: HashMap<String, (usize, usize)> = HashMap::new();

        for thread in &self.threads {
            let entry = files.entry(thread.file_path.clone()).or_insert((0, 0));
            if thread.status == "open" {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }

        let mut result: Vec<_> = files
            .into_iter()
            .map(|(path, (open, resolved))| FileEntry {
                path,
                open_threads: open,
                resolved_threads: resolved,
            })
            .collect();

        result.sort_by(|a, b| a.path.cmp(&b.path));
        result
    }

    /// Get threads for the currently selected file
    #[must_use]
    pub fn threads_for_current_file(&self) -> Vec<&ThreadSummary> {
        let files = self.files_with_threads();
        let Some(file) = files.get(self.file_index) else {
            return Vec::new();
        };

        self.threads
            .iter()
            .filter(|t| t.file_path == file.path)
            .collect()
    }

    /// Get threads that are visible in the current diff (all threads for the file)
    #[must_use]
    pub fn visible_threads_for_current_file(&self) -> Vec<&ThreadSummary> {
        self.threads_for_current_file()
    }

    /// Build a flat list of sidebar items: files with their threads as children
    #[must_use]
    pub fn sidebar_items(&self) -> Vec<SidebarItem> {
        let files = self.files_with_threads();
        let mut items = Vec::new();

        for (file_idx, file) in files.iter().enumerate() {
            let collapsed = self.collapsed_files.contains(&file.path);
            items.push(SidebarItem::File {
                entry: file.clone(),
                file_idx,
                collapsed,
            });
            if !collapsed {
                // Add threads belonging to this file, sorted by their
                // position in the diff stream so the sidebar order matches
                // what the user sees in the diff pane.  Fall back to
                // selection_start for threads not yet positioned.
                let positions = self.thread_positions.borrow();
                let mut file_threads: Vec<&ThreadSummary> = self
                    .threads
                    .iter()
                    .filter(|t| t.file_path == file.path)
                    .collect();
                file_threads.sort_by_key(|t| {
                    positions
                        .get(&t.thread_id)
                        .copied()
                        .unwrap_or(usize::MAX)
                });

                for thread in file_threads {
                    items.push(SidebarItem::Thread {
                        thread_id: thread.thread_id.clone(),
                        status: thread.status.clone(),
                        comment_count: thread.comment_count,
                        file_idx,
                    });
                }
            }
        }

        items
    }

    /// Handle terminal resize
    pub const fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.layout_mode = LayoutMode::from_width(width);
    }

    /// Get the visible height for the review list (accounting for chrome)
    #[must_use]
    pub const fn list_visible_height(&self) -> usize {
        // Account for header block (5) + search bar (2) + help bar (2)
        // Each item is 2 lines tall
        let available = self.height.saturating_sub(9) as usize;
        available / 2
    }

    /// Sync current file fields from the file cache
    pub fn sync_active_file_cache(&mut self) {
        let files = self.files_with_threads();
        let Some(file) = files.get(self.file_index) else {
            self.current_diff = None;
            self.current_file_content = None;
            self.highlighted_lines.clear();
            return;
        };

        if let Some(entry) = self.file_cache.get(&file.path) {
            self.current_diff = entry.diff.clone();
            self.current_file_content = entry.file_content.clone();
            self.highlighted_lines = entry.highlighted_lines.clone();
        } else {
            self.current_diff = None;
            self.current_file_content = None;
            self.highlighted_lines.clear();
        }
    }
}

/// File entry for sidebar display
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub open_threads: usize,
    pub resolved_threads: usize,
}

/// An item in the sidebar tree (file or thread)
#[derive(Debug, Clone)]
pub enum SidebarItem {
    File {
        entry: FileEntry,
        /// Index into `files_with_threads()` for selection matching
        file_idx: usize,
        /// Whether this file's threads are collapsed
        collapsed: bool,
    },
    Thread {
        thread_id: String,
        status: String,
        comment_count: i64,
        /// Parent file index for selection matching
        file_idx: usize,
    },
}
