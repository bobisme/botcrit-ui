//! botcrit-ui - GitHub-style code review TUI for botcrit
//!
//! Uses Elm Architecture (Model/Message/Update/View) with opentui_rust rendering.

#![allow(dead_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

pub mod db;
pub mod diff;
pub mod message;
pub mod model;
pub mod syntax;
pub mod theme;
pub mod update;
pub mod vcs;
pub mod view;

// Re-exports
pub use db::Db;
pub use message::Message;
pub use model::{Focus, LayoutMode, Model, Screen};
pub use syntax::{HighlightSpan, Highlighter};
pub use theme::Theme;
pub use update::update;
pub use view::view;
