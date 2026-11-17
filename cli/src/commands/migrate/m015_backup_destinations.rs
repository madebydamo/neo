use toml_edit::DocumentMut;

use super::move_dotted;

/// The backup went from one destination to `services.backup.destinations.<name>`
/// (docs/design/backup-restore.md §2): the existing one becomes `main`.
pub(super) fn migrate(doc: &mut DocumentMut) {
    for key in [
        "target",
        "format",
        "host",
        "user",
        "sshKey",
        "extraOptions",
        "storeUrl",
        "storeClient",
        "remotePath",
        "dataset",
        "keep",
    ] {
        move_dotted(
            doc,
            &format!("services.backup.{key}"),
            &format!("services.backup.destinations.main.{key}"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_015_moves_the_backup_into_main() {
        let mut doc: DocumentMut = "[services.backup]\nenabled = true\nschedule = \"03:00:00\"\nhost = \"b\"\nformat = \"encrypted\"\nextraOptions = [\"-p\", \"23\"]\nremotePath = \"dev\"\n"
            .parse()
            .unwrap();
        apply_migrations(&mut doc);
        let b = &doc["services"]["backup"];
        assert_eq!(b["enabled"].as_bool(), Some(true));
        assert_eq!(b["schedule"].as_str(), Some("03:00:00"));
        assert!(b.get("host").is_none() && b.get("format").is_none());
        let main = &b["destinations"]["main"];
        assert_eq!(main["host"].as_str(), Some("b"));
        assert_eq!(main["format"].as_str(), Some("encrypted"));
        assert_eq!(main["remotePath"].as_str(), Some("dev"));
        assert_eq!(main["extraOptions"].as_array().map(|a| a.len()), Some(2));
    }
}
