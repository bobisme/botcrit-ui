//! Theme system for botcrit-ui
//!
//! Provides color tokens for consistent styling across the UI.

use opentui::Rgba;
use serde::{Deserialize, Serialize};

/// Diff-specific color tokens
#[derive(Debug, Clone)]
pub struct DiffTheme {
    /// Text color for added lines
    pub added: Rgba,
    /// Text color for removed lines
    pub removed: Rgba,
    /// Text color for context lines
    pub context: Rgba,
    /// Color for @@ hunk headers
    pub hunk_header: Rgba,

    /// Bright highlight for + signs
    pub highlight_added: Rgba,
    /// Bright highlight for - signs
    pub highlight_removed: Rgba,

    /// Background for added lines
    pub added_bg: Rgba,
    /// Background for removed lines
    pub removed_bg: Rgba,
    /// Background for context lines
    pub context_bg: Rgba,

    /// Line number text color
    pub line_number: Rgba,
    /// Line number bg for added lines
    pub added_line_number_bg: Rgba,
    /// Line number bg for removed lines
    pub removed_line_number_bg: Rgba,
}

/// Complete theme definition
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,

    // Base colors
    pub background: Rgba,
    pub foreground: Rgba,

    // UI chrome
    pub border: Rgba,
    pub border_focused: Rgba,
    pub panel_bg: Rgba,

    // Selection/highlighting
    pub selection_bg: Rgba,
    pub selection_fg: Rgba,
    pub cursor: Rgba,

    // Semantic colors
    pub primary: Rgba,
    pub success: Rgba,
    pub warning: Rgba,
    pub error: Rgba,
    pub muted: Rgba,

    // Diff colors
    pub diff: DiffTheme,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Default dark theme (Tokyo Night inspired)
    #[must_use]
    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),

            background: Rgba::from_hex("#1a1b26").unwrap_or(Rgba::BLACK),
            foreground: Rgba::from_hex("#c0caf5").unwrap_or(Rgba::WHITE),

            border: Rgba::from_hex("#3b4261").unwrap_or(Rgba::WHITE),
            border_focused: Rgba::from_hex("#7aa2f7").unwrap_or(Rgba::WHITE),
            panel_bg: Rgba::from_hex("#24283b").unwrap_or(Rgba::BLACK),

            selection_bg: Rgba::from_hex("#33467c").unwrap_or(Rgba::WHITE),
            selection_fg: Rgba::from_hex("#c0caf5").unwrap_or(Rgba::WHITE),
            cursor: Rgba::from_hex("#c0caf5").unwrap_or(Rgba::WHITE),

            primary: Rgba::from_hex("#7aa2f7").unwrap_or(Rgba::BLUE),
            success: Rgba::from_hex("#9ece6a").unwrap_or(Rgba::GREEN),
            warning: Rgba::from_hex("#e0af68").unwrap_or(Rgba::WHITE),
            error: Rgba::from_hex("#f7768e").unwrap_or(Rgba::RED),
            muted: Rgba::from_hex("#565f89").unwrap_or(Rgba::WHITE),

            diff: DiffTheme {
                added: Rgba::from_hex("#9ece6a").unwrap_or(Rgba::GREEN),
                removed: Rgba::from_hex("#f7768e").unwrap_or(Rgba::RED),
                context: Rgba::from_hex("#a9b1d6").unwrap_or(Rgba::WHITE),
                hunk_header: Rgba::from_hex("#565f89").unwrap_or(Rgba::WHITE),

                highlight_added: Rgba::from_hex("#73daca").unwrap_or(Rgba::GREEN),
                highlight_removed: Rgba::from_hex("#ff7a93").unwrap_or(Rgba::RED),

                added_bg: Rgba::from_hex("#1a2f1a").unwrap_or(Rgba::BLACK),
                removed_bg: Rgba::from_hex("#2f1a1a").unwrap_or(Rgba::BLACK),
                context_bg: Rgba::from_hex("#1a1b26").unwrap_or(Rgba::BLACK),

                line_number: Rgba::from_hex("#565f89").unwrap_or(Rgba::WHITE),
                added_line_number_bg: Rgba::from_hex("#152515").unwrap_or(Rgba::BLACK),
                removed_line_number_bg: Rgba::from_hex("#251515").unwrap_or(Rgba::BLACK),
            },
        }
    }

    /// Light theme variant
    #[must_use]
    pub fn light() -> Self {
        Self {
            name: "light".to_string(),

            background: Rgba::from_hex("#d5d6db").unwrap_or(Rgba::WHITE),
            foreground: Rgba::from_hex("#343b58").unwrap_or(Rgba::BLACK),

            border: Rgba::from_hex("#9699a3").unwrap_or(Rgba::BLACK),
            border_focused: Rgba::from_hex("#34548a").unwrap_or(Rgba::BLACK),
            panel_bg: Rgba::from_hex("#cbccd1").unwrap_or(Rgba::WHITE),

            selection_bg: Rgba::from_hex("#99a7df").unwrap_or(Rgba::BLACK),
            selection_fg: Rgba::from_hex("#343b58").unwrap_or(Rgba::BLACK),
            cursor: Rgba::from_hex("#343b58").unwrap_or(Rgba::BLACK),

            primary: Rgba::from_hex("#34548a").unwrap_or(Rgba::BLUE),
            success: Rgba::from_hex("#485e30").unwrap_or(Rgba::GREEN),
            warning: Rgba::from_hex("#8f5e15").unwrap_or(Rgba::WHITE),
            error: Rgba::from_hex("#8c4351").unwrap_or(Rgba::RED),
            muted: Rgba::from_hex("#6a6f87").unwrap_or(Rgba::BLACK),

            diff: DiffTheme {
                added: Rgba::from_hex("#485e30").unwrap_or(Rgba::GREEN),
                removed: Rgba::from_hex("#8c4351").unwrap_or(Rgba::RED),
                context: Rgba::from_hex("#343b58").unwrap_or(Rgba::BLACK),
                hunk_header: Rgba::from_hex("#6a6f87").unwrap_or(Rgba::BLACK),

                highlight_added: Rgba::from_hex("#33635c").unwrap_or(Rgba::GREEN),
                highlight_removed: Rgba::from_hex("#a8323e").unwrap_or(Rgba::RED),

                added_bg: Rgba::from_hex("#c5dcc5").unwrap_or(Rgba::WHITE),
                removed_bg: Rgba::from_hex("#dcc5c5").unwrap_or(Rgba::WHITE),
                context_bg: Rgba::from_hex("#d5d6db").unwrap_or(Rgba::WHITE),

                line_number: Rgba::from_hex("#6a6f87").unwrap_or(Rgba::BLACK),
                added_line_number_bg: Rgba::from_hex("#b5ccb5").unwrap_or(Rgba::WHITE),
                removed_line_number_bg: Rgba::from_hex("#ccb5b5").unwrap_or(Rgba::WHITE),
            },
        }
    }
}

/// JSON-serializable theme format for loading from files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub colors: ThemeColors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColors {
    pub background: String,
    pub foreground: String,
    pub border: String,
    pub border_focused: String,
    pub panel_bg: String,
    pub selection_bg: String,
    pub selection_fg: String,
    pub cursor: String,
    pub primary: String,
    pub success: String,
    pub warning: String,
    pub error: String,
    pub muted: String,

    // Diff colors
    pub diff_added: String,
    pub diff_removed: String,
    pub diff_context: String,
    pub diff_hunk_header: String,
    pub diff_highlight_added: String,
    pub diff_highlight_removed: String,
    pub diff_added_bg: String,
    pub diff_removed_bg: String,
    pub diff_context_bg: String,
    pub diff_line_number: String,
    pub diff_added_line_number_bg: String,
    pub diff_removed_line_number_bg: String,
}

impl TryFrom<ThemeFile> for Theme {
    type Error = anyhow::Error;

    fn try_from(file: ThemeFile) -> Result<Self, Self::Error> {
        let c = &file.colors;
        Ok(Self {
            name: file.name,
            background: parse_color(&c.background)?,
            foreground: parse_color(&c.foreground)?,
            border: parse_color(&c.border)?,
            border_focused: parse_color(&c.border_focused)?,
            panel_bg: parse_color(&c.panel_bg)?,
            selection_bg: parse_color(&c.selection_bg)?,
            selection_fg: parse_color(&c.selection_fg)?,
            cursor: parse_color(&c.cursor)?,
            primary: parse_color(&c.primary)?,
            success: parse_color(&c.success)?,
            warning: parse_color(&c.warning)?,
            error: parse_color(&c.error)?,
            muted: parse_color(&c.muted)?,
            diff: DiffTheme {
                added: parse_color(&c.diff_added)?,
                removed: parse_color(&c.diff_removed)?,
                context: parse_color(&c.diff_context)?,
                hunk_header: parse_color(&c.diff_hunk_header)?,
                highlight_added: parse_color(&c.diff_highlight_added)?,
                highlight_removed: parse_color(&c.diff_highlight_removed)?,
                added_bg: parse_color(&c.diff_added_bg)?,
                removed_bg: parse_color(&c.diff_removed_bg)?,
                context_bg: parse_color(&c.diff_context_bg)?,
                line_number: parse_color(&c.diff_line_number)?,
                added_line_number_bg: parse_color(&c.diff_added_line_number_bg)?,
                removed_line_number_bg: parse_color(&c.diff_removed_line_number_bg)?,
            },
        })
    }
}

fn parse_color(hex: &str) -> anyhow::Result<Rgba> {
    Rgba::from_hex(hex).ok_or_else(|| anyhow::anyhow!("Invalid hex color: {hex}"))
}
