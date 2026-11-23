//! Step 4 of the local restore: check that everything is there, find the
//! first readable backup destination, evaluate the config and print the plan.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml_edit::DocumentMut;

use super::super::{DataSource, RestoreArgs};
use super::{Dirs, FromBackup, Staged, Unlocked};
use crate::utils::backup::store_ssh::{apply_routes, StoreRoute};
use crate::utils::backup::{self, Access, Format, Source};
use crate::utils::command::stdout_checked;
use crate::utils::profile::{resolve_config_path, PROFILE_SERVER};
use crate::utils::settings::str_at;

/// What the restore will do, decided before anything is erased.
pub(super) struct Plan {
    /// Target already mounted by hand (no Disko).
    pub mounted: bool,
    pub disko_on: bool,
    pub main_disk: String,
    /// `core.volumes.root` of the config.
    pub root: String,
    pub pool: String,
    /// Where the config repo lives on the restored machine.
    pub config_path: String,
    /// `config_path` relative to `root`; `None` when it lies outside.
    pub config_rel: Option<String>,
    /// Every backup destination, in the order they are tried.
    pub sources: Vec<Source>,
    pub access: Access,
    /// Index of the first readable destination in `sources`.
    pub first: Option<usize>,
    /// The data row of the printed plan.
    pub data_line: String,
    /// Built Disko script; `None` when the target is mounted by hand.
    pub disko_script: Option<String>,
}

impl Plan {
    /// Whether the data can come from `s`, with a line describing what is there.
    pub fn usable(&self, s: &Source, data_from: Option<&Path>) -> Result<String> {
        if s.format == Format::Zfs && (self.mounted || !self.disko_on) {
            bail!("a ZFS backup restores into the Disko layout only");
        }
        backup::check(&self.access, s, data_from)
    }

    pub fn print(&self, args: &RestoreArgs, doc: &DocumentMut, identity: bool) {
        println!();
        println!("Restore plan");
        println!(
            "  machine   {} (domain {})",
            backup::hostname(doc),
            str_at(doc, &["services", "swag", "domain"]).unwrap_or("-")
        );
        if self.mounted {
            println!(
                "  disk      already mounted at {} (no Disko)",
                args.target.display()
            );
        } else {
            println!("  disk      {} — ERASED", self.main_disk);
            // Disko formats every disk it describes, the extra pools too.
            for (dev, mount) in crate::commands::bootstrap::answers::additional_disks(doc) {
                println!("  disk      {dev} → {mount} — ERASED");
            }
        }
        println!(
            "  identity  {}",
            if identity {
                "old SSH host key (secrets decrypt)"
            } else {
                "new host key"
            }
        );
        println!("  data      {}", self.data_line);
        println!(
            "  install   {}{}",
            if args.no_install {
                "skipped"
            } else {
                "nixos-install"
            },
            if args.reboot { ", then reboot" } else { "" }
        );
        println!();
    }
}

/// `git+file:` / `path:` plugins that do not exist on this machine.
fn missing_local_plugins(doc: &DocumentMut) -> Vec<String> {
    crate::utils::plugins::plugins_from_doc(doc)
        .into_iter()
        .map(|p| p.url)
        .filter_map(|url| {
            let base = url.split(['?', '#']).next().unwrap_or(&url);
            let dir = ["git+file://", "git+file:", "path:"]
                .iter()
                .find_map(|p| base.strip_prefix(p))
                .map(str::to_string)?;
            Some((url, dir))
        })
        .filter(|(_, p)| !Path::new(p).exists())
        .map(|(u, _)| u)
        .collect()
}

/// What the config fetches from another host: the plugins and the locked
/// flake inputs, as (what, host, port).
fn input_hosts(doc: &DocumentMut, lock: &str) -> Vec<(String, String, u16)> {
    let mut found: Vec<(String, String, u16)> = Vec::new();
    let mut add = |what: String, url: &str| {
        if let Some((h, p)) = url_host(url) {
            if !found.iter().any(|(_, fh, fp)| *fh == h && *fp == p) {
                found.push((what, h, p));
            }
        }
    };
    for p in crate::utils::plugins::plugins_from_doc(doc) {
        add(
            format!("plugin {} (core.plugins.{}.url)", p.id, p.id),
            &p.url,
        );
    }
    let v: serde_json::Value = serde_json::from_str(lock).unwrap_or_default();
    if let Some(nodes) = v["nodes"].as_object() {
        for (name, n) in nodes {
            let l = &n["locked"];
            let url = match l["type"].as_str() {
                Some("git" | "tarball" | "file" | "mercurial") => {
                    l["url"].as_str().map(str::to_string)
                }
                Some("github" | "gitlab" | "sourcehut") => {
                    l["host"].as_str().map(|h| format!("https://{h}/"))
                }
                _ => None,
            };
            if let Some(u) = url {
                add(format!("flake input {name}"), &u);
            }
        }
    }
    found
}

/// `scheme://[user@]host[:port]/…` → (host, port); None for local paths and
/// shorthands such as `github:owner/repo`.
fn url_host(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.rsplit('+').next().unwrap_or(scheme);
    let default = match scheme {
        "https" => 443,
        "http" => 80,
        "ssh" => 22,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    let hostport = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || hostport.starts_with('[') => match p.parse() {
            Ok(p) => (h, p),
            Err(_) => (hostport, default),
        },
        _ => (hostport, default),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// Inputs on a host that does not answer from here: they may run on the
/// machine being restored (an edge's own Git server or cache). Said, not
/// routed around (docs/design/backup-store-ssh.md §6).
fn unreachable_inputs(doc: &DocumentMut, staging: &Path) -> Vec<String> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let lock = std::fs::read_to_string(staging.join("flake.lock")).unwrap_or_default();
    let handles: Vec<_> = input_hosts(doc, &lock)
        .into_iter()
        .map(|(what, host, port)| {
            std::thread::spawn(move || {
                let addrs: Vec<_> = match (host.as_str(), port).to_socket_addrs() {
                    Ok(a) => a.collect(),
                    Err(_) => return Some(format!("{what}: {host} does not resolve")),
                };
                let ok = addrs
                    .iter()
                    .any(|a| TcpStream::connect_timeout(a, Duration::from_secs(5)).is_ok());
                (!ok).then(|| format!("{what}: {host}:{port} does not answer"))
            })
        })
        .collect();
    handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect()
}

pub(super) fn preflight(
    args: &RestoreArgs,
    dirs: &Dirs,
    staged: &Staged,
    unlocked: &Unlocked,
    from_backup: Option<&FromBackup>,
    admin_keys: &[PathBuf],
    routes: &[StoreRoute],
) -> Result<Plan> {
    let doc = &staged.doc;
    let (mounted, disko_on) = (staged.mounted, staged.disko_on);
    let missing = missing_local_plugins(doc);
    if !missing.is_empty() {
        bail!(
            "local plugins are not on this machine: {}. Put them into git, or copy them to the same path here first",
            missing.join(", ")
        );
    }
    let unreachable = unreachable_inputs(doc, &dirs.staging);
    for u in &unreachable {
        println!("! {u} from here.");
    }
    if !unreachable.is_empty() {
        println!("  If it runs on the machine you restore, point it elsewhere first and commit: a plugin at a copy (core.plugins.<id>.url = \"git+file:///path\"), a flake input with `nix flake lock --override-input <name> <url>`. docs/RESTORE.md → Restoring your edge.");
    }
    let main_disk = str_at(doc, &["disko", "mainDisk"])
        .unwrap_or("/dev/vda")
        .to_string();
    if !mounted && !Path::new(&main_disk).exists() {
        bail!("disko.mainDisk {main_disk} does not exist here; pick one from `ls -l /dev/disk/by-id` and pass --disk");
    }
    let root = backup::volumes_root(doc);
    let pool = str_at(doc, &["disko", "poolName"])
        .unwrap_or("zroot")
        .to_string();
    let config_path = resolve_config_path(doc, PROFILE_SERVER);
    let config_rel = config_path
        .strip_prefix(&format!("{root}/"))
        .map(str::to_string);

    // Every backup destination, in the order they are tried.
    let with_data = args.data == DataSource::Backup;
    // A store reached over SSH (--from-store) goes in front of its HTTPS way.
    let sources: Vec<Source> = match (from_backup, with_data) {
        (_, false) => Vec::new(),
        (Some(fb), true) if fb.only => vec![fb.source.clone()],
        (_, true) => apply_routes(Source::from_settings(doc)?, routes, &dirs.work)?,
    };
    for r in routes {
        let used = sources.iter().any(|s| {
            s.store
                .as_ref()
                .is_some_and(|st| st.over_ssh && st.url == r.url)
        });
        if with_data && !used {
            println!(
                "! No backup destination of this config uses the store {} (storeUrl): --from-store does not change where the data comes from",
                r.url
            );
        }
    }
    if with_data && sources.is_empty() && args.data_from.is_none() {
        bail!("the backup is off in this config: restore without data (--data none), or point --data-from at a copy");
    }
    let mut identities = Vec::new();
    if let Some(k) = &args.backup_key {
        identities.push(k.clone());
    }
    if unlocked.identity && unlocked.host_key.exists() {
        identities.push(unlocked.host_key.clone());
    }
    identities.extend(admin_keys.iter().cloned());
    let access = Access {
        identities,
        known_hosts: dirs.work.join("known_hosts"),
        work: dirs.work.join("backup"),
    };
    let mut plan = Plan {
        mounted,
        disko_on,
        main_disk,
        root,
        pool,
        config_path,
        config_rel,
        sources,
        access,
        first: None,
        data_line: String::new(),
        disko_script: None,
    };

    // The first destination that can be read; the ones after it are the
    // fallbacks if copying from it fails.
    let mut first = None;
    let mut unreadable = Vec::new();
    for (i, s) in plan.sources.iter().enumerate() {
        match plan.usable(s, args.data_from.as_deref()) {
            Ok(line) => {
                first = Some((i, line));
                break;
            }
            Err(e) => {
                println!("✗ cannot read the backup {}: {e:#}", s.describe());
                unreadable.push(format!("{}: {e:#}", s.describe()));
            }
        }
    }
    plan.data_line = match (&first, plan.sources.is_empty()) {
        (_, true) => "none (empty data)".into(),
        (None, false) => bail!(
            "no backup destination can be read: {}",
            unreadable.join("; ")
        ),
        (Some((i, line)), false) => {
            let rest = plan.sources.len() - i - 1;
            format!(
                "{} — {line}{}",
                plan.sources[*i].describe(),
                match rest {
                    0 => String::new(),
                    n => format!(" (then {n} more if it fails)"),
                }
            )
        }
    };
    plan.first = first.map(|(i, _)| i);
    if !plan.sources.is_empty() && plan.config_rel.is_none() {
        bail!(
            "the config repo ({}) is not inside {}",
            plan.config_path,
            plan.root
        );
    }

    let flake = format!(
        "{}#nixosConfigurations.{}",
        dirs.staging.display(),
        args.config
    );
    plan.disko_script = if mounted {
        None
    } else {
        println!("Evaluating the configuration (first run builds a lot) …");
        Some(
            // Build progress stays on the terminal (first runs take long).
            stdout_checked(
                Command::new("nix")
                    .args(["build", "--no-link", "--print-out-paths"])
                    .arg(format!("{flake}.config.system.build.diskoScript"))
                    .env("NIX_CONFIG", &unlocked.nix_config)
                    .stderr(Stdio::inherit()),
                "nix build diskoScript",
            )
            .context("the configuration does not evaluate")?
            .trim()
            .to_string(),
        )
    };
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_of_inputs() {
        assert_eq!(
            url_host("git+ssh://git@git.example.com:2222/me/p.git"),
            Some(("git.example.com".into(), 2222))
        );
        assert_eq!(
            url_host("git+https://gitea.dev.example.ch/a/b?ref=main"),
            Some(("gitea.dev.example.ch".into(), 443))
        );
        assert_eq!(url_host("https://[::1]:8443/x"), Some(("::1".into(), 8443)));
        assert_eq!(url_host("github:a/b"), None);
        assert_eq!(url_host("git+file:///srv/p"), None);
        let doc: DocumentMut =
            "[core.plugins.tmr]\nurl = \"git+https://gitea.example.ch/syntelis/tmr\"\n"
                .parse()
                .unwrap();
        let lock = r#"{"nodes":{"root":{},"neo":{"locked":{"type":"github","owner":"m","repo":"neo"}},
            "plugin-tmr":{"locked":{"type":"git","url":"https://gitea.example.ch/syntelis/tmr"}},
            "cache":{"locked":{"type":"tarball","url":"http://cache.lan:8080/x.tar.gz"}}}}"#;
        let hosts: Vec<(String, u16)> = input_hosts(&doc, lock)
            .into_iter()
            .map(|(_, h, p)| (h, p))
            .collect();
        assert_eq!(
            hosts,
            [("gitea.example.ch".into(), 443), ("cache.lan".into(), 8080)]
        );
    }

    #[test]
    fn finds_missing_local_plugins() {
        let doc: DocumentMut = "[core]\nplugins = [\"github:a/b\", \"git+file:/nonexistent/x\", \"path:/\", \"git+file:///nope?ref=main\"]\n"
            .parse()
            .unwrap();
        assert_eq!(
            missing_local_plugins(&doc),
            vec!["git+file:/nonexistent/x", "git+file:///nope?ref=main"]
        );
    }
}
