//! What differs per page that shows the chat drawer.
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::refs::RefItem;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The page-specific half of the chat.
pub trait ChatHost: Send + Sync {
    /// The chat context for a `data-neo-agent` value (`general`,
    /// `service:<name>`, …), normalized, or None when this page has no such
    /// context.
    fn context(&self, raw: &str) -> Option<String>;
    /// Whether a past chat of this context is listed under Chats here.
    fn owns(&self, context: &str) -> bool;
    /// Display label and icon per context (`service:pihole` → Pi-hole).
    fn labels(&self) -> BoxFut<'_, HashMap<String, (String, Option<String>)>>;
    /// Instructions sent with the first message of a chat of `context`.
    fn instructions<'a>(&'a self, context: &'a str) -> BoxFut<'a, String>;
    /// Candidates for the `@` popup.
    fn refs(&self) -> BoxFut<'_, Result<Arc<Vec<RefItem>>, String>>;
    /// The `<neo-references>` block for a message's `@tokens`, if any resolve.
    fn ref_block<'a>(&'a self, message: &'a str) -> BoxFut<'a, Option<String>>;
    /// Assistant Markdown → HTML.
    fn render<'a>(&'a self, text: &'a str) -> BoxFut<'a, String>;
}

pub type Host = Arc<dyn ChatHost>;
