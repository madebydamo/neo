use super::escape::escape_html;

/// Spinner + truncated message (in-progress).
pub fn status_pulling(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="inline-flex items-center gap-1 text-info max-w-full"><span class="loading loading-spinner loading-xs flex-shrink-0"></span><span class="truncate">{}</span></span>"#,
        escape_html(msg)
    );
    (inner, title)
}

/// Success checkmark + message.
pub fn status_ok(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-success truncate">✓ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

/// Error mark + message.
pub fn status_err(msg: &str) -> (String, String) {
    let title = msg.to_string();
    let inner = format!(
        r#"<span class="text-error truncate">✗ {}</span>"#,
        escape_html(msg)
    );
    (inner, title)
}

/// Generic OOB status slot: `<div id="{prefix}-{key}" … hx-swap-oob>`.
pub fn status_slot_oob(prefix: &str, key: &str, classes: &str, inner: &str, title: &str) -> String {
    use super::escape::escape_attr;
    format!(
        r#"<div id="{prefix}-{key}" class="{classes}" title="{title}" hx-swap-oob="true">{inner}</div>"#,
        prefix = escape_html(prefix),
        key = escape_html(key),
        classes = classes,
        title = escape_attr(title),
        inner = inner,
    )
}
