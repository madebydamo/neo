/// Escape text for HTML body/content contexts (`&`, `<`, `>`, `"`, `'`).
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape a string for embedding inside a double-quoted HTML attribute.
pub fn escape_attr(s: &str) -> String {
    escape_html(s)
}

/// Escape a value for embedding inside a double-quoted Nix string literal.
pub fn escape_nix_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('$', "\\$")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_quotes() {
        assert_eq!(
            escape_html(r#"a&b<"c">'d"#),
            "a&amp;b&lt;&quot;c&quot;&gt;&#39;d"
        );
    }
}
