//! Message types for the Elm Architecture

/// All possible user actions and system events
#[derive(Debug, Clone)]
pub enum Message {
    // === Navigation ===
    /// Select a review from the list
    SelectReview(String),
    /// Go back to previous screen
    Back,

    // === List Navigation ===
    /// Move selection up in list
    ListUp,
    /// Move selection down in list
    ListDown,
    /// Page up in list
    ListPageUp,
    /// Page down in list
    ListPageDown,
    /// Go to first item
    ListTop,
    /// Go to last item
    ListBottom,

    // === File Sidebar ===
    /// Move down in sidebar tree
    NextFile,
    /// Move up in sidebar tree
    PrevFile,
    /// Jump to first sidebar item
    SidebarTop,
    /// Jump to last sidebar item
    SidebarBottom,
    /// Select file by index
    SelectFile(usize),
    /// Select sidebar item by row index (mouse click)
    ClickSidebarItem(usize),
    /// Activate current sidebar item (Enter)
    SidebarSelect,

    // === Diff/Content Pane ===
    /// Scroll content up
    ScrollUp,
    /// Scroll content down
    ScrollDown,
    /// Scroll to top
    ScrollTop,
    /// Scroll to bottom
    ScrollBottom,
    /// Scroll up by half a page
    ScrollHalfPageUp,
    /// Scroll down by half a page
    ScrollHalfPageDown,
    /// Scroll up by 10 lines
    ScrollTenUp,
    /// Scroll down by 10 lines
    ScrollTenDown,
    /// Page up in content
    PageUp,
    /// Page down in content
    PageDown,
    /// Jump to next thread
    NextThread,
    /// Jump to previous thread
    PrevThread,
    /// Expand a thread to show comments
    ExpandThread(String),
    /// Collapse expanded thread
    CollapseThread,

    // === Focus ===
    /// Toggle focus between panes
    ToggleFocus,

    // === Actions ===
    /// Resolve a thread
    ResolveThread(String),
    /// Reopen a resolved thread
    ReopenThread(String),

    // === Filter/View ===
    /// Filter to open reviews only
    FilterOpen,
    /// Filter to all reviews
    FilterAll,
    /// Toggle between unified and side-by-side diff view
    ToggleDiffView,
    /// Toggle file sidebar visibility
    ToggleSidebar,
    /// Toggle diff line wrapping
    ToggleDiffWrap,
    /// Open current file in editor
    OpenFileInEditor,

    // === Command Palette ===
    ShowCommandPalette,
    HideCommandPalette,
    CommandPaletteNext,
    CommandPalettePrev,
    CommandPaletteUpdateInput(String),
    CommandPaletteInputBackspace,
    CommandPaletteExecute,

    // === Commenting ===
    EnterCommentMode,
    CommentInput(String),
    CommentInputBackspace,
    SaveComment,
    CancelComment,

    // === Theme Selection ===
    ShowThemePicker,
    ApplyTheme(String),

    // === System ===
    /// Terminal resize event
    Resize {
        width: u16,
        height: u16,
    },
    /// Periodic tick for animations/refresh
    Tick,
    /// Request to quit
    Quit,
    /// No-op (ignore event)
    Noop,
}
