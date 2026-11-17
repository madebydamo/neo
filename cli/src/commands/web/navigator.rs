//! Navigator sidebar data.
//!
//! On a deployed host the NixOS module writes the running generation's
//! navigator JSON at build time (`nix/services/neo/navigator.nix`) and points
//! `NEO_NAVIGATOR_PATH` at it, so the dashboard needs no nix eval. Without the
//! file (`neo web` in development) the caller falls back to the evaluator.
use std::path::Path;

use anyhow::{Context, Result};

use super::icons::IconSet;
use super::types::NavigatorContext;

const NAVIGATOR_PATH_ENV: &str = "NEO_NAVIGATOR_PATH";

/// Navigator from the build-time file, or `None` when unset or unreadable.
pub fn from_build_file(icons: &IconSet) -> Option<NavigatorContext> {
    let path = std::env::var(NAVIGATOR_PATH_ENV).ok()?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    match read(Path::new(path)) {
        Ok(mut ctx) => {
            finish(&mut ctx, icons);
            Some(ctx)
        }
        Err(e) => {
            eprintln!("web: navigator file unusable, falling back to nix eval: {e:#}");
            None
        }
    }
}

fn read(path: &Path) -> Result<NavigatorContext> {
    let body = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("parse {}", path.display()))
}

/// Fill per-service domain and initials and publish file icons.
pub fn finish(ctx: &mut NavigatorContext, icons: &IconSet) {
    let dom = ctx.domain.clone();
    for s in &mut ctx.services {
        s.domain = dom.clone();
        icons.link(&mut s.icon);
        if s.initials.is_empty() {
            s.initials = s.name.chars().take(2).collect::<String>().to_uppercase();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_build_file_and_fills_services() {
        let dir = std::env::temp_dir().join(format!("neo-navigator-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("navigator.json");
        std::fs::write(
            &file,
            r#"{"domain":"example.org","hostname":"box","theme":"dark",
                "services":[{"name":"jellyfin","subdomain":"tv","icon":"https://x/i.svg",
                "rank":null,"iframeCompatible":false,"units":["docker-jellyfin"],"timers":[]}]}"#,
        )
        .unwrap();

        let mut ctx = read(&file).unwrap();
        finish(&mut ctx, &IconSet::new(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(ctx.theme, "dark");
        assert_eq!(ctx.hostname, "box");
        let s = &ctx.services[0];
        assert_eq!(s.domain.as_deref(), Some("example.org"));
        assert_eq!(s.initials, "JE");
        assert_eq!(s.icon.as_deref(), Some("https://x/i.svg"));
        assert!(!s.iframe_compatible);
        assert_eq!(s.units, vec!["docker-jellyfin"]);
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(read(Path::new("/nonexistent/neo-navigator.json")).is_err());
    }
}
