//! botcrit-ui - GitHub-style code review TUI for botcrit
//!
//! Uses Elm Architecture (Model/Message/Update/View) with `opentui_rust` rendering.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]

pub mod cli_client;
pub mod command;
pub mod config;
pub mod db;
pub mod diff;
pub mod input;
pub mod layout;
pub mod message;
pub mod model;
pub mod stream;
pub mod syntax;
pub mod text;
pub mod theme;
pub mod update;
pub mod vcs;
pub mod view;

pub use cli_client::CliClient;
pub use db::CritClient;
pub use message::Message;
pub use model::{Focus, LayoutMode, Model, Screen};
pub use syntax::{HighlightSpan, Highlighter};
pub use theme::Theme;
pub use update::update;
pub use view::view;
