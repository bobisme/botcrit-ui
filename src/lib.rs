//! botcrit-ui - GitHub-style code review TUI for botcrit
//!
//! Uses Elm Architecture (Model/Message/Update/View) with `opentui_rust` rendering.

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::similar_names)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::ref_option)]
#![allow(clippy::if_then_some_else_none)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::question_mark_used)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::redundant_pub_crate)]
#![allow(clippy::redundant_closure_for_method_calls)]

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

// Re-exports
pub use cli_client::CliClient;
pub use db::CritClient;
pub use message::Message;
pub use model::{Focus, LayoutMode, Model, Screen};
pub use syntax::{HighlightSpan, Highlighter};
pub use theme::Theme;
pub use update::update;
pub use view::view;
