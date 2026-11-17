use toml_edit::DocumentMut;

use super::{insert_dotted, remove_dotted};

/// neo-cli.gitUserName/gitUserEmail → core.git.userName/userEmail (the machine
/// git identity). Custom so it runs after 003, which still moves the keys into
/// neo-cli. An existing core.git value wins.
pub(super) fn migrate(doc: &mut DocumentMut) {
    for (from, to) in [("gitUserName", "userName"), ("gitUserEmail", "userEmail")] {
        let Some(val) = remove_dotted(doc, &format!("neo-cli.{from}")) else {
            continue;
        };
        let taken = doc
            .get("core")
            .and_then(|t| t.get("git"))
            .and_then(|t| t.as_table_like())
            .is_some_and(|t| t.contains_key(to));
        if !taken {
            insert_dotted(doc, &format!("core.git.{to}"), val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_008_moves_git_identity_to_core() {
        let raw = r#"
[core.git]
userEmail = "kept@example.com"

[neo-cli]
gitUserName = "Ops"
gitUserEmail = "old@example.com"
defaultBranch = "master"
"#;
        let mut doc: DocumentMut = raw.parse().unwrap();
        assert!(apply_migrations(&mut doc));

        let git = doc
            .get("core")
            .and_then(|t| t.get("git"))
            .and_then(|t| t.as_table())
            .unwrap();
        assert_eq!(git.get("userName").and_then(|v| v.as_str()), Some("Ops"));
        assert_eq!(
            git.get("userEmail").and_then(|v| v.as_str()),
            Some("kept@example.com"),
            "an existing core.git value wins"
        );
        let cli = doc.get("neo-cli").and_then(|t| t.as_table()).unwrap();
        assert!(!cli.contains_key("gitUserName"));
        assert!(!cli.contains_key("gitUserEmail"));
        assert!(cli.contains_key("defaultBranch"));
    }
}
