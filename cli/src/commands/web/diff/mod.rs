//! Changes preview: git output parsing, semantic settings diff and HTML rendering.
pub mod flake_lock;
mod intraline;
pub mod model;
pub mod parse;
mod render;
pub mod settings_semantic;

pub use render::{render_changes, RenderOptions};
pub use settings_semantic::{diff_settings_toml, SettingsDiff};
