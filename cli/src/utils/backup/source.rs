//! The backup destinations as they are read from settings.toml or named on
//! the command line: the same keys the backup service writes with
//! (nix/services/backup, design docs/design/backup-restore.md §2), used here
//! in the other direction.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

use crate::utils::profile::DEFAULT_VOLUMES_ROOT;
use crate::utils::settings::{bool_at, item_at, str_at};

#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    Files,
    /// restic repository + neo-backup.age.
    Encrypted,
    Zfs,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    /// `user@host` and the ssh options from `extraOptions`.
    Ssh { dest: String, options: Vec<String> },
    /// A disk attached to the machine being restored.
    Disk,
}

#[derive(Debug, Clone)]
pub struct Source {
    /// The destination's name in the settings; empty for `--from-backup`.
    pub name: String,
    pub location: Location,
    pub format: Format,
    /// files: folder holding `<basename of sourceDir>/`; encrypted: folder
    /// holding neo-backup.age and the repository (remotePath).
    pub remote_path: String,
    /// files: basename of sourceDir (`neo` for /var/neo).
    pub folder: String,
    /// zfs: dataset holding the backup snapshots.
    pub dataset: String,
    /// sshKey from the settings (the host key by default).
    pub ssh_key: String,
    /// A backup store destination: which store and which of its clients.
    pub store: Option<StoreRef>,
}

/// A destination in a backup store (`target = "store"`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoreRef {
    /// The store's address, `https://host[:port]` without a trailing `/`.
    pub url: String,
    pub client: String,
    /// Reached over the store server's normal SSH, not over `url`
    /// (docs/design/backup-store-ssh.md §4.3).
    pub over_ssh: bool,
}

/// A store address as both sides write it: no trailing `/`.
pub fn normalize_store_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// neo.core.volumes.root (default /var/neo).
pub fn volumes_root(doc: &DocumentMut) -> String {
    str_at(doc, &["core", "volumes", "root"])
        .unwrap_or(DEFAULT_VOLUMES_ROOT)
        .trim_end_matches('/')
        .to_string()
}

pub fn hostname(doc: &DocumentMut) -> String {
    str_at(doc, &["core", "hostname"])
        .unwrap_or("homeserver")
        .to_string()
}

fn backup_enabled(doc: &DocumentMut) -> bool {
    bool_at(doc, &["services", "backup", "enabled"]).unwrap_or(false)
}

/// The enabled tables of `services.backup.destinations`. Configs from before
/// `destinations` (migration 015) are one destination named `main`.
fn enabled_destinations(doc: &DocumentMut) -> Vec<(&str, &toml_edit::Item)> {
    let Some(b) = item_at(doc, &["services", "backup"]) else {
        return Vec::new();
    };
    match b.get("destinations").and_then(|d| d.as_table_like()) {
        Some(dests) => dests
            .iter()
            .filter(|(_, d)| d.get("enabled").and_then(|v| v.as_bool()) != Some(false))
            .collect(),
        None => vec![("main", b)],
    }
}

impl Source {
    /// The enabled backup destinations in settings.toml, in the order a restore
    /// tries them: by `priority`, then name (the backup's own run order,
    /// nix/services/backup). Empty when backup is off.
    pub fn from_settings(doc: &DocumentMut) -> Result<Vec<Source>> {
        let Some(b) = item_at(doc, &["services", "backup"]) else {
            return Ok(Vec::new());
        };
        if !backup_enabled(doc) {
            return Ok(Vec::new());
        }
        let root = volumes_root(doc);
        let source_dir = b
            .get("sourceDir")
            .and_then(|v| v.as_str())
            .unwrap_or(&root)
            .to_string();
        let mut found = Vec::new();
        for (name, d) in enabled_destinations(doc) {
            let priority = d
                .get("priority")
                .and_then(|v| v.as_integer())
                .unwrap_or(100);
            found.push((priority, Self::destination(doc, name, d, &source_dir)?));
        }
        found.sort_by(|(pa, a), (pb, b)| pa.cmp(pb).then_with(|| a.name.cmp(&b.name)));
        Ok(found.into_iter().map(|(_, s)| s).collect())
    }

    /// One destination table (`services.backup.destinations.<name>`).
    fn destination(
        doc: &DocumentMut,
        name: &str,
        b: &toml_edit::Item,
        source_dir: &str,
    ) -> Result<Source> {
        let at = format!("services.backup.destinations.{name}");
        let get = |k: &str| b.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let format = match get("format").as_deref().unwrap_or("files") {
            "files" => Format::Files,
            "encrypted" => Format::Encrypted,
            "zfs" => Format::Zfs,
            other => bail!("{at}.format = {other:?} is not files, encrypted or zfs"),
        };
        let mut store = None;
        let location = match get("target").as_deref().unwrap_or("ssh") {
            "ssh" => {
                let host = get("host")
                    .filter(|h| !h.is_empty())
                    .with_context(|| format!("{at}.host is not set"))?;
                let user = get("user").unwrap_or_else(|| "root".into());
                let options = b
                    .get("extraOptions")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                Location::Ssh {
                    dest: format!("{user}@{host}"),
                    options,
                }
            }
            "disk" => Location::Disk,
            "store" => {
                let url = get("storeUrl")
                    .filter(|u| !u.is_empty())
                    .with_context(|| format!("{at}.storeUrl is not set"))?;
                let client = get("storeClient").unwrap_or_else(|| hostname(doc));
                let location = store_location(&url, &client)?;
                store = Some(StoreRef {
                    url: normalize_store_url(&url),
                    client,
                    over_ssh: false,
                });
                location
            }
            other => bail!("{at}.target = {other:?} is not ssh, disk or store"),
        };
        let root = volumes_root(doc);
        let folder = Path::new(source_dir)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "neo".into());
        if format != Format::Zfs && source_dir.trim_end_matches('/') != root {
            bail!(
                "the backup copies {source_dir}, not all of {root}: restore it by hand (docs/RESTORE.md)"
            );
        }
        Ok(Source {
            name: name.to_string(),
            location,
            format,
            remote_path: get("remotePath").unwrap_or_else(|| hostname(doc)),
            folder,
            dataset: get("dataset").unwrap_or_default(),
            ssh_key: get("sshKey").unwrap_or_else(|| "/etc/ssh/ssh_host_ed25519_key".into()),
            store,
        })
    }

    /// A backup named on the command line (`--from-backup`): `user@host:folder`
    /// (relative to the home folder), `ssh://user@host[:port]/abs/folder`,
    /// `https://client@store[:port]/folder` (a backup store), or a local
    /// folder. The format is found with [`detect`].
    pub fn from_location(spec: &str) -> Result<Source> {
        let mut store = None;
        let (location, remote_path) = if let Some(rest) = spec.strip_prefix("https://") {
            let (auth, path) = rest.split_once('/').unwrap_or((rest, ""));
            let (client, host) = auth.split_once('@').with_context(|| {
                format!("{spec}: name the server in the store: https://<name>@store/folder")
            })?;
            let url = format!("https://{host}");
            let location = store_location(&url, client)?;
            store = Some(StoreRef {
                url,
                client: client.to_string(),
                over_ssh: false,
            });
            (location, path.to_string())
        } else if spec.starts_with("ssh+store://") {
            bail!("{spec}: a store over SSH is read with `neo restore --from-store`, or through Source::from_spec");
        } else if spec.starts_with("http://") {
            bail!("{spec}: a backup store is only reachable over https://");
        } else if let Some(rest) = spec.strip_prefix("ssh://") {
            let (auth, path) = rest.split_once('/').unwrap_or((rest, ""));
            let (dest, options) = match auth.rsplit_once(':') {
                Some((d, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
                    (d.to_string(), vec!["-p".to_string(), p.to_string()])
                }
                _ => (auth.to_string(), Vec::new()),
            };
            (Location::Ssh { dest, options }, format!("/{path}"))
        } else if spec.starts_with('/') || spec.starts_with('.') {
            let p = fs::canonicalize(spec).with_context(|| format!("{spec} does not exist"))?;
            (Location::Disk, p.display().to_string())
        } else if let Some((dest, path)) = spec.split_once(':') {
            (
                Location::Ssh {
                    dest: dest.to_string(),
                    options: Vec::new(),
                },
                path.to_string(),
            )
        } else {
            bail!("{spec}: give the backup as user@host:folder, ssh://user@host:port/folder, https://name@store/folder or a local folder");
        };
        let remote_path = match remote_path.trim_end_matches('/') {
            "" if remote_path.starts_with('/') => "/".to_string(),
            p => p.to_string(),
        };
        Ok(Source {
            name: String::new(),
            location,
            format: Format::Files,
            remote_path,
            folder: "neo".into(),
            dataset: String::new(),
            ssh_key: String::new(),
            store,
        })
    }

    pub fn describe(&self) -> String {
        let at = match (&self.location, &self.store) {
            (_, Some(s)) if s.over_ssh => format!(
                "the store {} over SSH (account backup-{})",
                s.url.trim_start_matches("https://"),
                s.client
            ),
            (Location::Ssh { dest, .. }, _) => dest.clone(),
            (Location::Disk, _) => "a disk on this machine".into(),
        };
        let what = match self.format {
            Format::Files => format!("files in {}/{} on {at}", self.remote_path, self.folder),
            Format::Encrypted => format!(
                "encrypted backup in {} on {at}",
                folder_or_home(&self.remote_path)
            ),
            Format::Zfs => format!("ZFS snapshots of {} on {at}", self.dataset),
        };
        if self.name.is_empty() {
            what
        } else {
            format!("{} ({what})", self.name)
        }
    }
}

/// Each store destination reachable over SSH as well (`--from-store`) gets
/// that way in front of its HTTPS way; the order of the others stays
/// (docs/design/backup-store-ssh.md §4.2). `over_ssh(store, client)` builds
/// the SSH way of a store destination, or None for a store it does not know.
pub fn with_store_ssh(
    sources: Vec<Source>,
    mut over_ssh: impl FnMut(&Source, &StoreRef) -> Result<Option<Source>>,
) -> Result<Vec<Source>> {
    let mut out = Vec::with_capacity(sources.len());
    for s in sources {
        if let Some(r) = s.store.clone().filter(|r| !r.over_ssh) {
            if let Some(ssh) = over_ssh(&s, &r)? {
                out.push(ssh);
            }
        }
        out.push(s);
    }
    Ok(out)
}

/// A backup store: SSH to the client's account, inside a WebSocket on `url`
/// (nix/services/backup-store).
fn store_location(url: &str, client: &str) -> Result<Location> {
    let authority = url
        .strip_prefix("https://")
        .with_context(|| format!("{url}: a backup store is only reachable over https://"))?
        .trim_end_matches('/');
    let host = authority.split(':').next().unwrap_or_default();
    if host.is_empty() || client.is_empty() || authority.contains('/') {
        bail!("{url} is not a backup store address like https://backup.example.com");
    }
    Ok(Location::Ssh {
        dest: format!("backup-{client}@{host}"),
        options: vec![
            "-o".into(),
            format!("ProxyCommand=websocat --binary -E - wss://{authority}/"),
            "-o".into(),
            "ServerAliveInterval=30".into(),
            "-o".into(),
            "ServerAliveCountMax=10".into(),
        ],
    })
}

fn folder_or_home(p: &str) -> &str {
    if p.is_empty() {
        "the home folder"
    } else {
        p
    }
}

/// `path` inside `dir`, as sftp and rsync read it ("" = the SSH user's home).
pub(super) fn join_remote(dir: &str, path: &str) -> String {
    match dir.trim_end_matches('/') {
        "" if dir.starts_with('/') => format!("/{path}"),
        "" => path.to_string(),
        d => format!("{d}/{path}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::backup::encrypted::HEADER;
    use crate::utils::backup::restore::files_origin;

    fn doc(s: &str) -> DocumentMut {
        s.parse().unwrap()
    }

    #[test]
    fn off_without_backup() {
        assert!(Source::from_settings(&doc("")).unwrap().is_empty());
        assert!(
            Source::from_settings(&doc("[services.backup]\nhost = \"x\"\n"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn ssh_files_defaults() {
        let d = doc(
            "[core]\nhostname = \"myhome\"\n[services.backup]\nenabled = true\nhost = \"b.example\"\nuser = \"zh1\"\nextraOptions = [\"-p\", \"23\"]\n",
        );
        let s = Source::from_settings(&d).unwrap().remove(0);
        assert_eq!(s.format, Format::Files);
        assert_eq!(s.remote_path, "myhome");
        assert_eq!(s.folder, "neo");
        assert_eq!(s.ssh_key, "/etc/ssh/ssh_host_ed25519_key");
        assert_eq!(
            s.location,
            Location::Ssh {
                dest: "zh1@b.example".into(),
                options: vec!["-p".into(), "23".into()]
            }
        );
        assert_eq!(files_origin(&s, None).unwrap(), "zh1@b.example:myhome/neo/");
    }

    #[test]
    fn disk_zfs() {
        let d = doc(
            "[services.backup]\nenabled = true\ntarget = \"disk\"\nformat = \"zfs\"\ndataset = \"usb/neo\"\n",
        );
        let s = Source::from_settings(&d).unwrap().remove(0);
        assert_eq!(s.location, Location::Disk);
        assert_eq!(s.format, Format::Zfs);
        assert_eq!(s.dataset, "usb/neo");
    }

    #[test]
    fn encrypted_from_settings() {
        let d = doc("[services.backup]\nenabled = true\nformat = \"encrypted\"\nhost = \"b\"\nremotePath = \"bk/home\"\n");
        let s = Source::from_settings(&d).unwrap().remove(0);
        assert_eq!(s.format, Format::Encrypted);
        assert_eq!(
            join_remote(&s.remote_path, HEADER),
            "bk/home/neo-backup.age"
        );
        assert!(s
            .describe()
            .starts_with("main (encrypted backup in bk/home"));
    }

    #[test]
    fn locations() {
        let s = Source::from_location("zh1@b.example:backups/home/").unwrap();
        assert_eq!(
            s.location,
            Location::Ssh {
                dest: "zh1@b.example".into(),
                options: vec![]
            }
        );
        assert_eq!(s.remote_path, "backups/home");
        let s = Source::from_location("ssh://root@10.0.0.2:2222/tank/bk").unwrap();
        assert_eq!(
            s.location,
            Location::Ssh {
                dest: "root@10.0.0.2".into(),
                options: vec!["-p".into(), "2222".into()]
            }
        );
        assert_eq!(s.remote_path, "/tank/bk");
        let s = Source::from_location("ssh://u@h").unwrap();
        assert_eq!(s.remote_path, "/");
        assert_eq!(join_remote(&s.remote_path, HEADER), "/neo-backup.age");
        let s = Source::from_location("u@h:").unwrap();
        assert_eq!(join_remote(&s.remote_path, HEADER), "neo-backup.age");
        assert_eq!(Source::from_location("/").unwrap().location, Location::Disk);
        assert!(Source::from_location("justaname").is_err());
    }

    #[test]
    fn store() {
        let d = doc("[core]\nhostname = \"myhome\"\n[services.backup]\nenabled = true\ntarget = \"store\"\nformat = \"encrypted\"\nstoreUrl = \"https://backup.example.com/\"\n");
        let s = Source::from_settings(&d).unwrap().remove(0);
        let Location::Ssh { dest, options } = &s.location else {
            panic!();
        };
        assert_eq!(dest, "backup-myhome@backup.example.com");
        assert_eq!(
            options[1],
            "ProxyCommand=websocat --binary -E - wss://backup.example.com/"
        );

        let s = Source::from_location("https://myhome@backup.example.com:8443/dev/").unwrap();
        let Location::Ssh { dest, options } = &s.location else {
            panic!();
        };
        assert_eq!(dest, "backup-myhome@backup.example.com");
        assert!(options[1].ends_with("wss://backup.example.com:8443/"));
        assert_eq!(s.remote_path, "dev");
        assert_eq!(
            join_remote(
                &Source::from_location("https://m@b.example")
                    .unwrap()
                    .remote_path,
                HEADER
            ),
            HEADER
        );

        assert!(Source::from_location("https://b.example/dev").is_err());
        assert!(Source::from_location("http://m@b.example/dev").is_err());
        assert!(store_location("http://b.example", "m").is_err());
    }

    #[test]
    fn destinations_in_restore_order() {
        let d = doc(concat!(
            "[core]\nhostname = \"myhome\"\n",
            "[services.backup]\nenabled = true\n",
            "[services.backup.destinations.usb]\ntarget = \"disk\"\nremotePath = \"/mnt/usb\"\n",
            "[services.backup.destinations.friend-a]\nhost = \"b.example\"\nremotePath = \"disk1/neo\"\nformat = \"encrypted\"\npriority = 10\n",
            "[services.backup.destinations.friend-b]\nhost = \"b.example\"\nremotePath = \"disk2/neo\"\nformat = \"encrypted\"\npriority = 10\n",
            "[services.backup.destinations.old]\nenabled = false\nhost = \"gone.example\"\n",
        ));
        let all = Source::from_settings(&d).unwrap();
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["friend-a", "friend-b", "usb"]);
        assert_eq!(all[0].remote_path, "disk1/neo");
        assert_eq!(all[1].remote_path, "disk2/neo");
        assert_eq!(all[2].location, Location::Disk);
        assert!(all[0]
            .describe()
            .starts_with("friend-a (encrypted backup in disk1/neo"));
        // The flat keys of configs from before are one destination.
        let legacy = doc("[services.backup]\nenabled = true\nhost = \"h\"\n");
        assert_eq!(Source::from_settings(&legacy).unwrap()[0].name, "main");
    }

    #[test]
    fn partial_files_backup_is_refused() {
        let d = doc(
            "[services.backup]\nenabled = true\nhost = \"h\"\nsourceDir = \"/var/neo/DATA/Media\"\n",
        );
        assert!(Source::from_settings(&d).is_err());
    }
}
