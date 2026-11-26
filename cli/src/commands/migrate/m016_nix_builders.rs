use toml_edit::{Array, DocumentMut, Item, Table, Value};

use super::{insert_dotted, remove_dotted};

/// Remote Nix builders went from one `core.nix.remoteBuild` table to
/// `core.nix.builders.<name>` (multi-machine with priority weights):
/// - If `remoteBuild` has a `host`, move it to `core.nix.builders.main`
///   (preserving `enabled`, defaulting to `false` if omitted since the old
///   default was `false` while the new per-builder default is `true`).
/// - Convert `system = "..."` to `systems = ["..."]`.
/// - Convert legacy SSH CLI `extraOptions` flags (`-p`, `-o`, `-i`, `-J`, `-4`,
///   `-6`, `-C`) into `port` and `sshOptions` (`ssh_config` key/value pairs).
/// - If `remoteBuild` had no `host`, drop the table cleanly.
pub(super) fn migrate(doc: &mut DocumentMut) {
    let Some(old) = remove_dotted(doc, "core.nix.remoteBuild") else {
        return;
    };
    let Some(mut tbl) = old.into_table().ok() else {
        return;
    };

    let has_host = tbl
        .get("host")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !has_host {
        return;
    }

    if !tbl.contains_key("enabled") {
        tbl.insert("enabled", Item::Value(Value::from(false)));
    }

    if let Some(sys) = tbl.remove("system") {
        if let Some(s) = sys.as_str() {
            let mut arr = Array::new();
            arr.push(s);
            tbl.insert("systems", Item::Value(Value::Array(arr)));
        }
    }

    if let Some(extra) = tbl.remove("extraOptions") {
        if let Some(arr) = extra.as_array() {
            let args: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
            let (port, ssh_opts) = parse_ssh_flags(&args);
            if let Some(p) = port {
                tbl.insert("port", Item::Value(Value::from(p)));
            }
            if !ssh_opts.is_empty() {
                let mut opts_tbl = Table::new();
                for (k, v) in ssh_opts {
                    opts_tbl.insert(&k, Item::Value(Value::from(v)));
                }
                tbl.insert("sshOptions", Item::Table(opts_tbl));
            }
        }
    }

    let mut target = Table::new();
    for (k, v) in tbl.iter() {
        target.insert(k, v.clone());
    }
    insert_dotted(doc, "core.nix.builders.main", Item::Table(target));
}

fn split_option(spec: &str) -> Option<(String, String)> {
    let s = spec.trim();
    let (k, v) = s
        .split_once('=')
        .or_else(|| s.split_once(char::is_whitespace))?;
    let k = k.trim();
    let v = v.trim();
    if k.is_empty() || v.is_empty() {
        return None;
    }
    Some((k.to_string(), v.to_string()))
}

fn parse_ssh_flags(args: &[&str]) -> (Option<i64>, Vec<(String, String)>) {
    let mut port = None;
    let mut opts = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        match arg {
            "-p" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(p) = val.parse::<i64>() {
                        port = Some(p);
                    }
                    i += 2;
                    continue;
                }
            }
            "-o" => {
                if let Some(val) = args.get(i + 1) {
                    if let Some(kv) = split_option(val) {
                        opts.push(kv);
                    }
                    i += 2;
                    continue;
                }
            }
            "-i" => {
                if let Some(val) = args.get(i + 1) {
                    opts.push(("IdentityFile".to_string(), (*val).to_string()));
                    i += 2;
                    continue;
                }
            }
            "-J" => {
                if let Some(val) = args.get(i + 1) {
                    opts.push(("ProxyJump".to_string(), (*val).to_string()));
                    i += 2;
                    continue;
                }
            }
            "-4" => opts.push(("AddressFamily".to_string(), "inet".to_string())),
            "-6" => opts.push(("AddressFamily".to_string(), "inet6".to_string())),
            "-C" => opts.push(("Compression".to_string(), "yes".to_string())),
            _ if arg.starts_with("-p") && arg.len() > 2 => {
                if let Ok(p) = arg[2..].parse::<i64>() {
                    port = Some(p);
                }
            }
            _ if arg.starts_with("-o") && arg.len() > 2 => {
                if let Some(kv) = split_option(&arg[2..]) {
                    opts.push(kv);
                }
            }
            _ if arg.starts_with("-i") && arg.len() > 2 => {
                opts.push(("IdentityFile".to_string(), arg[2..].to_string()));
            }
            _ if arg.starts_with("-J") && arg.len() > 2 => {
                opts.push(("ProxyJump".to_string(), arg[2..].to_string()));
            }
            _ => {}
        }
        i += 1;
    }
    (port, opts)
}

#[cfg(test)]
mod tests {
    use super::super::apply_migrations;
    use toml_edit::DocumentMut;

    #[test]
    fn migration_016_moves_enabled_remote_build_into_builders_main() {
        let mut doc: DocumentMut = r#"
[core.nix.remoteBuild]
enabled = true
host = "buildbox"
user = "damo"
maxJobs = 16
speedFactor = 4
system = "aarch64-linux"
extraOptions = ["-p", "2222", "-o", "StrictHostKeyChecking=accept-new"]
"#
        .parse()
        .unwrap();
        apply_migrations(&mut doc);
        let nix = &doc["core"]["nix"];
        assert!(nix.get("remoteBuild").is_none());
        let main = &nix["builders"]["main"];
        assert_eq!(main["enabled"].as_bool(), Some(true));
        assert_eq!(main["host"].as_str(), Some("buildbox"));
        assert_eq!(main["user"].as_str(), Some("damo"));
        assert_eq!(main["maxJobs"].as_integer(), Some(16));
        assert_eq!(main["speedFactor"].as_integer(), Some(4));
        assert_eq!(main["port"].as_integer(), Some(2222));
        assert!(main.get("system").is_none());
        assert!(main.get("extraOptions").is_none());
        let systems: Vec<&str> = main["systems"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(systems, vec!["aarch64-linux"]);
        assert_eq!(
            main["sshOptions"]["StrictHostKeyChecking"].as_str(),
            Some("accept-new")
        );
    }

    #[test]
    fn migration_016_keeps_disabled_when_enabled_was_omitted() {
        let mut doc: DocumentMut = r#"
[core.nix.remoteBuild]
host = "buildbox"
user = "damo"
"#
        .parse()
        .unwrap();
        apply_migrations(&mut doc);
        let main = &doc["core"]["nix"]["builders"]["main"];
        assert_eq!(main["enabled"].as_bool(), Some(false));
        assert_eq!(main["host"].as_str(), Some("buildbox"));
    }

    #[test]
    fn migration_016_drops_empty_or_hostless_remote_build() {
        let mut doc: DocumentMut = r#"
[core.nix.remoteBuild]
enabled = false
"#
        .parse()
        .unwrap();
        apply_migrations(&mut doc);
        let nix = &doc["core"]["nix"];
        assert!(nix.get("remoteBuild").is_none());
        assert!(nix.get("builders").is_none());
    }
}
