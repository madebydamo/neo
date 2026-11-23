//! A backup store reached over its server's normal SSH instead of its HTTPS
//! address (docs/design/backup-store-ssh.md §4): the operator's own login to
//! the store server carries the bytes to the store's sshd on 127.0.0.1, where
//! the store's keys and its gate decide as over HTTPS.
//!
//! - [`fetch`]: `ssh <login> neo backup-store list --json`.
//! - [`my_keys`] / [`opens`]: which of your keys opens which client.
//! - [`StoreRoute`]: how this machine reaches the store's sshd ([`Connect`]),
//!   turned into a [`Location::Ssh`], so the readers do not change.
//! - [`resolve_chain`]: the login's route from `ssh -G`, for a new machine
//!   that has none of the operator's ssh_config (§4.4, direct).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::source::{normalize_store_url, Format, Location, Source, StoreRef};
use crate::commands::backup_store::list::StoreListing;
use crate::commands::backup_store::{ClientReport, KeyInfo};

/// How this machine reaches the store's sshd (127.0.0.1:<sshPort> on the
/// store server).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "via", rename_all = "camelCase")]
pub enum Connect {
    /// `ProxyJump=<login>`: the login works from here (your computer, or a
    /// machine with your ssh_config).
    Jump { login: String },
    /// One master connection per client in `dir` (`<client>.sock`), opened by
    /// the operator's session on the new machine (§4.4 direct).
    Master { dir: PathBuf },
    /// 127.0.0.1:<port> here leads to the store's sshd (§4.4 relay).
    Port { port: u16 },
}

/// The client and folder picked when no config repo is given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Picked {
    pub client: String,
    /// Backup folder in the client's home ("." = the home itself).
    pub folder: String,
    /// files | encrypted
    pub format: String,
}

/// One store and the way to its sshd.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreRoute {
    /// The store's HTTPS address: matches `storeUrl` of the destinations.
    pub url: String,
    /// The store server's hostname (messages only).
    pub host: String,
    pub ssh_port: u16,
    /// `ssh-ed25519 AAAA…`, read through the authenticated login, so the
    /// inner hop is pinned without trust on first use.
    pub host_key: String,
    pub connect: Connect,
    #[serde(default)]
    pub picked: Option<Picked>,
}

impl StoreRoute {
    pub fn new(listing: &StoreListing, connect: Connect) -> Result<Self> {
        if listing.host_key.split_whitespace().count() < 2 {
            bail!("the store on {} sent no host key", listing.host);
        }
        Ok(Self {
            url: normalize_store_url(&listing.url),
            host: listing.host.clone(),
            ssh_port: listing.ssh_port,
            host_key: listing.host_key.clone(),
            connect,
            picked: None,
        })
    }

    /// The name the store's host key is pinned under: its HTTPS host.
    pub fn host_alias(&self) -> String {
        let authority = self.url.trim_start_matches("https://");
        authority.split(':').next().unwrap_or(authority).to_string()
    }

    /// The SSH location of `client`'s account. Writes the pinned host key to
    /// `<work>/known_hosts.store`.
    pub fn location(&self, client: &str, work: &Path) -> Result<Location> {
        fs::create_dir_all(work)?;
        let known = work.join("known_hosts.store");
        let line = format!("{} {}\n", self.host_alias(), self.host_key);
        let mut text = fs::read_to_string(&known).unwrap_or_default();
        if !text.contains(&line) {
            text.push_str(&line);
            fs::write(&known, text)?;
        }
        Ok(Location::Ssh {
            dest: format!("backup-{client}@127.0.0.1"),
            options: self.ssh_options(client, &known),
        })
    }

    fn ssh_options(&self, client: &str, known: &Path) -> Vec<String> {
        let mut o: Vec<String> = Vec::new();
        let mut opt = |v: String| {
            o.push("-o".into());
            o.push(v);
        };
        opt(format!("HostKeyAlias={}", self.host_alias()));
        opt(format!("UserKnownHostsFile={}", known.display()));
        opt("StrictHostKeyChecking=yes".into());
        opt("ServerAliveInterval=30".into());
        opt("ServerAliveCountMax=10".into());
        let port = match &self.connect {
            Connect::Jump { login } => {
                opt(format!("ProxyJump={login}"));
                self.ssh_port
            }
            Connect::Master { dir } => {
                opt(format!(
                    "ControlPath={}",
                    master_socket(dir, client).display()
                ));
                opt("ControlMaster=no".into());
                self.ssh_port
            }
            Connect::Port { port } => *port,
        };
        o.extend(["-p".to_string(), port.to_string()]);
        o
    }

    /// The same destination, read over SSH.
    pub fn variant(&self, source: &Source, store: &StoreRef, work: &Path) -> Result<Source> {
        Ok(Source {
            location: self.location(&store.client, work)?,
            store: Some(StoreRef {
                over_ssh: true,
                ..store.clone()
            }),
            ..source.clone()
        })
    }

    /// The picked client's backup (no config repo given).
    pub fn picked_source(&self, work: &Path) -> Result<Option<Source>> {
        let Some(p) = &self.picked else {
            return Ok(None);
        };
        let format = match p.format.as_str() {
            "encrypted" => Format::Encrypted,
            "files" => Format::Files,
            _ => bail!(
                "{} sends ZFS snapshots: give its config repo (neo restore <repo> --from-store …)",
                p.client
            ),
        };
        Ok(Some(Source {
            name: String::new(),
            location: self.location(&p.client, work)?,
            format,
            remote_path: match p.folder.trim_matches('/') {
                "." => String::new(),
                f => f.to_string(),
            },
            folder: "neo".into(),
            dataset: String::new(),
            ssh_key: String::new(),
            store: Some(StoreRef {
                url: self.url.clone(),
                client: p.client.clone(),
                over_ssh: true,
            }),
        }))
    }
}

/// The master connection of one client's account (§4.4 direct).
pub fn master_socket(dir: &Path, client: &str) -> PathBuf {
    dir.join(format!("{client}.sock"))
}

/// The SSH way in front of each store destination these routes reach; the
/// picked backup first when no destination matches it.
pub fn apply_routes(
    sources: Vec<Source>,
    routes: &[StoreRoute],
    work: &Path,
) -> Result<Vec<Source>> {
    let mut out = super::source::with_store_ssh(sources, |s, r| {
        routes
            .iter()
            .find(|route| route.url == r.url)
            .map(|route| route.variant(s, r, work))
            .transpose()
    })?;
    for route in routes {
        if let Some(p) = route.picked_source(work)? {
            let known = out.iter().any(|s| {
                s.store.as_ref().is_some_and(|r| {
                    r.over_ssh && r.url == route.url && r.client == p.store.as_ref().unwrap().client
                }) && s.remote_path == p.remote_path
            });
            if !known {
                out.insert(0, p);
            }
        }
    }
    Ok(out)
}

/// Read the routes `neo restore --host` wrote for the new machine.
pub fn read_routes(path: &Path) -> Result<Vec<StoreRoute>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// `ssh <login> neo backup-store list --json`, with the errors explained.
pub fn fetch(login: &str) -> Result<StoreListing> {
    println!("Asking {login} for its backup store …");
    let out = Command::new("ssh")
        .arg(login)
        .args(["neo", "backup-store", "list", "--json"])
        .stdin(Stdio::inherit())
        .stderr(Stdio::piped())
        .output()
        .context("run ssh")?;
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    match out.status.code() {
        Some(0) => {}
        Some(255) => bail!(
            "could not log in to {login}: {err}. Neo uses your normal SSH; check that `ssh {login}` works."
        ),
        Some(127) => bail!("{login} has no neo command: it runs no Neo backup store"),
        _ if err.contains("unrecognized subcommand") || err.contains("invalid value") => bail!(
            "{login} runs a Neo older than the way in over SSH (no `neo backup-store list`); update it, or use the store's web address"
        ),
        _ if err.contains("is the backup store on?") => {
            bail!("{login} runs no Neo backup store (the backup store is off there)")
        }
        _ => bail!("`neo backup-store list` failed on {login}: {err}"),
    }
    serde_json::from_slice(&out.stdout)
        .with_context(|| format!("{login} sent something that is no store listing"))
}

/// `ssh+store://<client>@<login>/<folder>`: (client, login, folder).
pub fn parse_spec(spec: &str) -> Result<Option<(String, String, String)>> {
    let Some(rest) = spec.strip_prefix("ssh+store://") else {
        return Ok(None);
    };
    let (authority, folder) = rest.split_once('/').unwrap_or((rest, ""));
    let (client, login) = authority
        .split_once('@')
        .filter(|(c, l)| !c.is_empty() && !l.is_empty())
        .with_context(|| {
            format!("{spec}: write ssh+store://<server name>@<your login to the store>/<folder>")
        })?;
    // `user@host:port` is no ssh destination; the URI form is.
    let login = match login.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
            format!("ssh://{h}:{p}")
        }
        _ => login.to_string(),
    };
    let folder = match folder.trim_matches('/') {
        "" => ".".to_string(),
        f => f.to_string(),
    };
    Ok(Some((client.to_string(), login, folder)))
}

/// One of your keys.
#[derive(Debug, Clone, PartialEq)]
pub struct MyKey {
    pub fingerprint: String,
    /// Where it is: a file, or "your SSH agent".
    pub label: String,
    /// The private key file, when there is one (an encrypted backup opens
    /// only with the file).
    pub private: Option<PathBuf>,
}

/// Your keys: `--key`, `~/.ssh/*.pub` and the agent's (`ssh-add -l`).
pub fn my_keys(extra: &[PathBuf]) -> Vec<MyKey> {
    let mut keys: Vec<MyKey> = Vec::new();
    let mut add = |k: MyKey| match keys.iter_mut().find(|m| m.fingerprint == k.fingerprint) {
        Some(m) if m.private.is_none() && k.private.is_some() => *m = k,
        Some(_) => {}
        None => keys.push(k),
    };
    for k in extra {
        if let Some(fp) = fingerprint_of(k) {
            add(MyKey {
                fingerprint: fp,
                label: k.display().to_string(),
                private: Some(k.clone()),
            });
        }
    }
    if let Some(home) = env::var_os("HOME") {
        let dir = PathBuf::from(home).join(".ssh");
        let mut pubs: Vec<PathBuf> = fs::read_dir(&dir)
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "pub"))
                    .collect()
            })
            .unwrap_or_default();
        pubs.sort();
        for p in pubs {
            if let Some(fp) = fingerprint_of(&p) {
                let private = p.with_extension("");
                add(MyKey {
                    fingerprint: fp,
                    label: tilde(&private),
                    private: private.exists().then_some(private),
                });
            }
        }
    }
    if let Ok(out) = Command::new("ssh-add")
        .args(["-l", "-E", "sha256"])
        .stderr(Stdio::null())
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(fp) = line
                .split_whitespace()
                .nth(1)
                .filter(|f| f.starts_with("SHA256:"))
            {
                add(MyKey {
                    fingerprint: fp.to_string(),
                    label: "a key in your SSH agent".into(),
                    private: None,
                });
            }
        }
    }
    keys
}

fn tilde(p: &Path) -> String {
    match env::var("HOME") {
        Ok(h) if !h.is_empty() => match p.strip_prefix(&h) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => p.display().to_string(),
        },
        _ => p.display().to_string(),
    }
}

fn fingerprint_of(key: &Path) -> Option<String> {
    let out = Command::new("ssh-keygen")
        .args(["-l", "-E", "sha256", "-f"])
        .arg(key)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

/// Whether one of your keys restores a client.
#[derive(Debug, Clone, PartialEq)]
pub enum Opens {
    /// It logs in and opens the backup.
    Yes(MyKey),
    /// It logs in, but the encrypted backup is not encrypted to it.
    LoginOnly(MyKey),
    No,
}

pub fn opens(c: &ClientReport, mine: &[MyKey]) -> Opens {
    let roles_known = c.keys.iter().any(|k| !k.role.is_empty());
    let find = |want: &dyn Fn(&KeyInfo) -> bool| {
        // A key with its private file first: age needs the file.
        let mut hits: Vec<&MyKey> = mine
            .iter()
            .filter(|m| {
                c.keys
                    .iter()
                    .any(|k| k.fingerprint == m.fingerprint && want(k))
            })
            .collect();
        hits.sort_by_key(|m| m.private.is_none());
        hits.first().map(|m| (*m).clone())
    };
    if roles_known {
        if let Some(m) = find(&|k| k.role == "unlock") {
            return Opens::Yes(m);
        }
        if let Some(m) = find(&|k| k.role == "access") {
            return Opens::LoginOnly(m);
        }
        return Opens::No;
    }
    match find(&|_| true) {
        Some(m) => Opens::Yes(m),
        None => Opens::No,
    }
}

/// "you can restore this with …" / "none of your keys can open it; …".
pub fn explain(c: &ClientReport, opens: &Opens) -> String {
    match opens {
        Opens::Yes(m) if m.private.is_none() => format!(
            "{} ({}) opens it: pass its private key file with --key",
            m.label, m.fingerprint
        ),
        Opens::Yes(m) => format!("you can restore this with {}", m.label),
        Opens::LoginOnly(m) => format!(
            "{} logs in but cannot open it: add it to System → SSH on {}; it counts after its next backup",
            m.label, c.name
        ),
        Opens::No => {
            let accepted: Vec<String> = c
                .keys
                .iter()
                .filter(|k| k.role != "server")
                .map(|k| format!("{} ({})", k.label, k.fingerprint))
                .collect();
            if accepted.is_empty() {
                "none of your keys can open it, and it accepts no admin key yet".into()
            } else {
                format!(
                    "none of your keys can open it; it accepts: {}",
                    accepted.join(", ")
                )
            }
        }
    }
}

/// The store's clients, one block each, for people.
pub fn show(listing: &StoreListing, mine: &[MyKey]) {
    println!(
        "{}",
        crate::commands::backup_store::list::text(listing, crate::commands::backup_store::now())
            .trim_end()
    );
    println!();
    for c in &listing.clients {
        println!("  {}: {}", c.name, explain(c, &opens(c, mine)));
    }
    println!();
}

/// `--client`, or the only client your keys open, or ask. Returns the
/// client and its backup folder.
pub fn pick(
    listing: &StoreListing,
    mine: &[MyKey],
    wanted: Option<&str>,
    ask: bool,
) -> Result<Picked> {
    let usable = |c: &ClientReport| c.format == "files" || c.format == "encrypted";
    let client = match wanted {
        Some(w) => listing
            .client(w)
            .with_context(|| format!("the store on {} holds no client {w}", listing.host))?,
        None => {
            let mut order: Vec<&ClientReport> =
                listing.clients.iter().filter(|c| usable(c)).collect();
            // Those you can open first.
            order.sort_by_key(|c| !matches!(opens(c, mine), Opens::Yes(_)));
            let openable = order
                .iter()
                .filter(|c| matches!(opens(c, mine), Opens::Yes(_)))
                .count();
            match (order.as_slice(), openable) {
                ([], _) => bail!(
                    "the store on {} holds no backup with files to read (ZFS backups need the config repo: neo restore <repo> --from-store …)",
                    listing.host
                ),
                ([one], _) => one,
                (many, 1) if !ask => many[0],
                (many, _) if ask => {
                    let options: Vec<String> = many
                        .iter()
                        .map(|c| format!("{} — {}", c.name, explain(c, &opens(c, mine))))
                        .collect();
                    let i = crate::commands::bootstrap::prompt::Prompt { auto: false }
                        .choose("Which server do you restore", &options, 0)?;
                    many[i]
                }
                (many, _) => bail!(
                    "the store holds {}: pass --client <name>",
                    many.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
                ),
            }
        }
    };
    if !usable(client) {
        bail!(
            "{} sends ZFS snapshots: give its config repo (neo restore <repo> --from-store …)",
            client.name
        );
    }
    let folder = match client.folders.as_slice() {
        [] => bail!("{} holds no backup yet", client.name),
        [one] => one.clone(),
        many if ask => {
            let i = crate::commands::bootstrap::prompt::Prompt { auto: false }.choose(
                &format!("Which backup folder of {}", client.name),
                many,
                0,
            )?;
            many[i].clone()
        }
        many => bail!(
            "{} has more than one backup folder ({}): use --from-backup ssh+store://{}@<login>/<folder>",
            client.name,
            many.join(", "),
            client.name
        ),
    };
    Ok(Picked {
        client: client.name.clone(),
        folder,
        format: client.format.clone(),
    })
}

// ── The login's route, for a new machine (§4.4 direct) ──────────────────

/// One hop as `ssh -G` resolves it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hop {
    pub user: String,
    pub hostname: String,
    pub port: u16,
    /// The name its key has in your known_hosts.
    pub known_as: String,
}

/// What `ssh -G` says about one destination.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SshG {
    pub hop: Hop,
    pub proxy_jump: Option<String>,
    pub proxy_command: Option<String>,
    pub known_hosts_files: Vec<String>,
}

pub fn parse_ssh_g(text: &str) -> SshG {
    let mut g = SshG::default();
    let mut alias = None;
    for line in text.lines() {
        let Some((k, v)) = line.trim().split_once(' ') else {
            continue;
        };
        let v = v.trim();
        match k {
            "user" => g.hop.user = v.into(),
            "hostname" => g.hop.hostname = v.into(),
            "port" => g.hop.port = v.parse().unwrap_or(22),
            "proxyjump" if v != "none" => g.proxy_jump = Some(v.into()),
            "proxycommand" if v != "none" => g.proxy_command = Some(v.into()),
            "hostkeyalias" if v != "none" => alias = Some(v.to_string()),
            "userknownhostsfile" | "globalknownhostsfile" => g
                .known_hosts_files
                .extend(v.split_whitespace().map(str::to_string)),
            _ => {}
        }
    }
    if g.hop.port == 0 {
        g.hop.port = 22;
    }
    g.hop.known_as = alias.unwrap_or_else(|| {
        if g.hop.port == 22 {
            g.hop.hostname.clone()
        } else {
            format!("[{}]:{}", g.hop.hostname, g.hop.port)
        }
    });
    g
}

/// A ProxyJump entry as an ssh destination: `user@host:port` → `ssh://…`.
fn jump_destination(entry: &str) -> String {
    if entry.starts_with("ssh://") {
        return entry.to_string();
    }
    match entry.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && p.bytes().all(|b| b.is_ascii_digit()) => {
            format!("ssh://{h}:{p}")
        }
        _ => entry.to_string(),
    }
}

/// The hops from the first jump host to the login itself, each resolved
/// with `ssh_g` (normally `ssh -G`).
pub fn resolve_chain(
    login: &str,
    ssh_g: &dyn Fn(&str) -> Result<SshG>,
) -> Result<(Vec<Hop>, Vec<String>)> {
    fn chain(
        dest: &str,
        ssh_g: &dyn Fn(&str) -> Result<SshG>,
        depth: usize,
        files: &mut Vec<String>,
    ) -> Result<Vec<Hop>> {
        if depth > 5 {
            bail!("{dest}: more than 5 jump hosts");
        }
        let g = ssh_g(dest)?;
        if let Some(c) = &g.proxy_command {
            bail!("{dest} is reached through a ProxyCommand ({c}), which the new machine does not have");
        }
        for f in &g.known_hosts_files {
            if !files.contains(f) {
                files.push(f.clone());
            }
        }
        let Some(jumps) = &g.proxy_jump else {
            return Ok(vec![g.hop]);
        };
        // `-J a,b dest`: a with its own config, b through a, dest through b.
        let entries: Vec<String> = jumps.split(',').map(jump_destination).collect();
        let mut hops = chain(&entries[0], ssh_g, depth + 1, files)?;
        for e in &entries[1..] {
            let j = ssh_g(e)?;
            hops.push(j.hop);
        }
        hops.push(g.hop);
        Ok(hops)
    }
    let mut files = Vec::new();
    let hops = chain(login, ssh_g, 0, &mut files)?;
    Ok((hops, files))
}

/// `ssh -G <dest>` on this computer.
pub fn ssh_g(dest: &str) -> Result<SshG> {
    let out = Command::new("ssh")
        .args(["-G", dest])
        .stdin(Stdio::null())
        .output()
        .context("ssh -G")?;
    if !out.status.success() {
        bail!(
            "ssh -G {dest}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(parse_ssh_g(&String::from_utf8_lossy(&out.stdout)))
}

/// Name of hop `i` in the new machine's ssh_config.
pub fn hop_name(i: usize) -> String {
    format!("neo-hop-{i}")
}

/// The new machine's ssh_config for these hops: resolved values only, keys
/// pinned to `known_hosts`, the admin key besides the forwarded agent.
pub fn hops_config(hops: &[Hop], known_hosts: &Path, admin_key: &Path) -> String {
    let mut s = String::new();
    for (i, h) in hops.iter().enumerate() {
        s.push_str(&format!(
            "Host {name}\n  HostName {}\n  User {}\n  Port {}\n  HostKeyAlias {name}\n",
            h.hostname,
            h.user,
            h.port,
            name = hop_name(i)
        ));
        if i > 0 {
            s.push_str(&format!("  ProxyJump {}\n", hop_name(i - 1)));
        }
    }
    s.push_str(&format!(
        "Host *\n  UserKnownHostsFile {}\n  StrictHostKeyChecking yes\n  BatchMode yes\n  ConnectTimeout 20\n  IdentityFile {}\n  ServerAliveInterval 30\n  ServerAliveCountMax 10\n",
        known_hosts.display(),
        admin_key.display()
    ));
    s
}

/// Your known_hosts lines for each hop, renamed to its `neo-hop-<i>`. Fails
/// for a hop you never logged in to.
pub fn hops_known_hosts(hops: &[Hop], files: &[String]) -> Result<String> {
    let home = env::var("HOME").unwrap_or_default();
    let files: Vec<String> = files
        .iter()
        .map(|f| match f.strip_prefix("~/") {
            Some(rest) => format!("{home}/{rest}"),
            None => f.clone(),
        })
        .filter(|f| Path::new(f).exists())
        .collect();
    let mut out = String::new();
    for (i, h) in hops.iter().enumerate() {
        let mut found = Vec::new();
        for f in &files {
            let o = Command::new("ssh-keygen")
                .args(["-F", &h.known_as, "-f", f])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .context("ssh-keygen -F")?;
            found.extend(rename_known_hosts(
                &String::from_utf8_lossy(&o.stdout),
                &hop_name(i),
            ));
        }
        if found.is_empty() {
            bail!(
                "host key of {} not in your known_hosts (log in to it once from this computer)",
                h.known_as
            );
        }
        for l in found {
            out.push_str(&l);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Lines of `ssh-keygen -F` output under another host name (markers such as
/// @revoked are dropped, a revoked key must not become trusted).
pub fn rename_known_hosts(found: &str, name: &str) -> Vec<String> {
    found
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('@'))
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let _host = w.next()?;
            let (t, k) = (w.next()?, w.next()?);
            Some(format!("{name} {t} {k}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing() -> StoreListing {
        serde_json::from_str(
            r#"{"host":"brugg","url":"https://backup.example.com/","sshPort":18023,"hostKey":"ssh-ed25519 AAAAHOST","generated":1,
               "clients":[
                 {"name":"meilen","format":"encrypted","configured":true,"estimated":false,"folders":["backup/meilen"],
                  "keys":[{"label":"root@meilen","fingerprint":"SHA256:srv","role":"server"},{"label":"me","fingerprint":"SHA256:me","role":"unlock"},{"label":"old","fingerprint":"SHA256:old","role":"access"}]},
                 {"name":"gossau","format":"zfs","configured":true,"estimated":false,"folders":[],"keys":[{"label":"me","fingerprint":"SHA256:me","role":""}]},
                 {"name":"uster","format":"files","configured":true,"estimated":false,"folders":[".","old"],"keys":[{"label":"x","fingerprint":"SHA256:x","role":""}]}
               ]}"#,
        )
        .unwrap()
    }

    fn key(fp: &str, file: bool) -> MyKey {
        MyKey {
            fingerprint: fp.into(),
            label: format!("~/.ssh/{fp}"),
            private: file.then(|| PathBuf::from(format!("/k/{fp}"))),
        }
    }

    #[test]
    fn specs() {
        assert_eq!(
            parse_spec("ssh+store://meilen@homeserver@brugg/backup/meilen/").unwrap(),
            Some((
                "meilen".into(),
                "homeserver@brugg".into(),
                "backup/meilen".into()
            ))
        );
        assert_eq!(
            parse_spec("ssh+store://meilen@homeserver@localhost:2222").unwrap(),
            Some((
                "meilen".into(),
                "ssh://homeserver@localhost:2222".into(),
                ".".into()
            ))
        );
        assert_eq!(
            parse_spec("ssh+store://m@brugg-alias/x")
                .unwrap()
                .unwrap()
                .1,
            "brugg-alias"
        );
        assert!(parse_spec("ssh+store://brugg/x").is_err());
        assert_eq!(parse_spec("https://m@b/x").unwrap(), None);
    }

    #[test]
    fn route_options_pin_the_store_and_jump_through_the_login() {
        let r = StoreRoute::new(
            &listing(),
            Connect::Jump {
                login: "homeserver@brugg".into(),
            },
        )
        .unwrap();
        assert_eq!(r.url, "https://backup.example.com");
        assert_eq!(r.host_alias(), "backup.example.com");
        let dir = crate::utils::fsutil::TempDir::new("neo-store-ssh-test").unwrap();
        let Location::Ssh { dest, options } = r.location("meilen", dir.path()).unwrap() else {
            panic!()
        };
        assert_eq!(dest, "backup-meilen@127.0.0.1");
        let o = options.join(" ");
        for want in [
            "ProxyJump=homeserver@brugg",
            "HostKeyAlias=backup.example.com",
            "StrictHostKeyChecking=yes",
            "-p 18023",
        ] {
            assert!(o.contains(want), "{want}: {o}");
        }
        let known = fs::read_to_string(dir.join("known_hosts.store")).unwrap();
        assert_eq!(known, "backup.example.com ssh-ed25519 AAAAHOST\n");
        // Twice: one line.
        r.location("uster", dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("known_hosts.store")).unwrap(),
            known
        );

        let m = StoreRoute {
            connect: Connect::Master {
                dir: "/root/neo-restore".into(),
            },
            ..r.clone()
        };
        let o = m.ssh_options("meilen", Path::new("/k")).join(" ");
        assert!(
            o.contains("ControlPath=/root/neo-restore/meilen.sock"),
            "{o}"
        );
        assert!(
            o.contains("ControlMaster=no") && !o.contains("ProxyJump"),
            "{o}"
        );
        let p = StoreRoute {
            connect: Connect::Port { port: 40123 },
            ..r
        };
        assert!(p
            .ssh_options("meilen", Path::new("/k"))
            .join(" ")
            .ends_with("-p 40123"));
    }

    #[test]
    fn routes_survive_the_file() {
        let r = StoreRoute {
            picked: Some(Picked {
                client: "meilen".into(),
                folder: ".".into(),
                format: "encrypted".into(),
            }),
            ..StoreRoute::new(&listing(), Connect::Port { port: 1 }).unwrap()
        };
        let text = serde_json::to_string(&vec![r.clone()]).unwrap();
        let back: Vec<StoreRoute> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, vec![r]);
    }

    #[test]
    fn store_destinations_get_the_ssh_way_first() {
        let doc: toml_edit::DocumentMut = concat!(
            "[core]\nhostname = \"meilen\"\n",
            "[services.backup]\nenabled = true\n",
            "[services.backup.destinations.brugg]\ntarget = \"store\"\nformat = \"encrypted\"\nstoreUrl = \"https://backup.example.com/\"\nremotePath = \"backup/meilen\"\npriority = 10\n",
            "[services.backup.destinations.other]\ntarget = \"store\"\nformat = \"encrypted\"\nstoreUrl = \"https://backup.elsewhere.com\"\npriority = 20\n",
            "[services.backup.destinations.usb]\ntarget = \"disk\"\nremotePath = \"/mnt/usb\"\npriority = 30\n",
        )
        .parse()
        .unwrap();
        let sources = Source::from_settings(&doc).unwrap();
        let route = StoreRoute::new(
            &listing(),
            Connect::Jump {
                login: "homeserver@brugg".into(),
            },
        )
        .unwrap();
        let dir = crate::utils::fsutil::TempDir::new("neo-store-ssh-test").unwrap();
        let all = apply_routes(sources, std::slice::from_ref(&route), dir.path()).unwrap();
        let names: Vec<(&str, bool)> = all
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.store.as_ref().is_some_and(|r| r.over_ssh),
                )
            })
            .collect();
        assert_eq!(
            names,
            [
                ("brugg", true),
                ("brugg", false),
                ("other", false),
                ("usb", false)
            ]
        );
        assert_eq!(all[0].remote_path, "backup/meilen");
        assert!(
            all[0].describe().contains("over SSH"),
            "{}",
            all[0].describe()
        );

        // A picked backup that no destination names comes first.
        let picked = StoreRoute {
            picked: Some(Picked {
                client: "meilen".into(),
                folder: "elsewhere".into(),
                format: "encrypted".into(),
            }),
            ..route.clone()
        };
        let again = apply_routes(
            Source::from_settings(&doc).unwrap(),
            std::slice::from_ref(&picked),
            dir.path(),
        )
        .unwrap();
        assert_eq!(again.len(), 5);
        assert_eq!(again[0].remote_path, "elsewhere");
        // The same folder as the destination: not twice.
        let same = StoreRoute {
            picked: Some(Picked {
                folder: "backup/meilen".into(),
                ..picked.picked.clone().unwrap()
            }),
            ..route
        };
        assert_eq!(
            apply_routes(Source::from_settings(&doc).unwrap(), &[same], dir.path())
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn keys_are_matched_by_role() {
        let l = listing();
        let meilen = l.client("meilen").unwrap();
        assert_eq!(
            opens(meilen, &[key("SHA256:me", true)]),
            Opens::Yes(key("SHA256:me", true))
        );
        // The file wins over the agent's copy.
        assert_eq!(
            opens(meilen, &[key("SHA256:me", false), key("SHA256:me", true)]),
            Opens::Yes(key("SHA256:me", true))
        );
        assert!(matches!(
            opens(meilen, &[key("SHA256:old", true)]),
            Opens::LoginOnly(_)
        ));
        assert_eq!(opens(meilen, &[key("SHA256:srv", true)]), Opens::No);
        let e = explain(meilen, &Opens::No);
        assert!(
            e.contains("me (SHA256:me)") && !e.contains("root@meilen"),
            "{e}"
        );
        assert!(explain(meilen, &Opens::Yes(key("SHA256:me", false))).contains("--key"));
        // Roles unknown (files, zfs): any listed key.
        let uster = l.client("uster").unwrap();
        assert!(matches!(
            opens(uster, &[key("SHA256:x", false)]),
            Opens::Yes(_)
        ));
    }

    #[test]
    fn picking() {
        let l = listing();
        let mine = [key("SHA256:me", true)];
        let p = pick(&l, &mine, Some("meilen"), false).unwrap();
        assert_eq!(
            p,
            Picked {
                client: "meilen".into(),
                folder: "backup/meilen".into(),
                format: "encrypted".into()
            }
        );
        // Only meilen opens: taken without asking.
        assert_eq!(pick(&l, &mine, None, false).unwrap().client, "meilen");
        assert!(pick(&l, &mine, Some("gossau"), false)
            .unwrap_err()
            .to_string()
            .contains("config repo"));
        assert!(pick(&l, &mine, Some("uster"), false)
            .unwrap_err()
            .to_string()
            .contains("more than one backup folder"));
        assert!(pick(&l, &[], None, false)
            .unwrap_err()
            .to_string()
            .contains("--client"));
    }

    #[test]
    fn ssh_g_output() {
        let g = parse_ssh_g(
            "user homeserver\nhostname 192.168.1.20\nport 2222\nproxyjump jump@gw:2200,inner\nuserknownhostsfile ~/.ssh/known_hosts ~/.ssh/known_hosts2\nglobalknownhostsfile /etc/ssh/ssh_known_hosts\n",
        );
        assert_eq!(
            g.hop,
            Hop {
                user: "homeserver".into(),
                hostname: "192.168.1.20".into(),
                port: 2222,
                known_as: "[192.168.1.20]:2222".into()
            }
        );
        assert_eq!(g.proxy_jump.as_deref(), Some("jump@gw:2200,inner"));
        assert_eq!(g.known_hosts_files.len(), 3);
        let a = parse_ssh_g("user u\nhostname h\nport 22\nhostkeyalias brugg\n");
        assert_eq!(a.hop.known_as, "brugg");
    }

    #[test]
    fn chains_resolve_nested_jumps() {
        let fake = |dest: &str| -> Result<SshG> {
            Ok(match dest {
                "brugg" => parse_ssh_g(
                    "user homeserver\nhostname 10.0.0.2\nport 22\nproxyjump ssh://jump@gw:2200,inner\nuserknownhostsfile ~/.ssh/known_hosts\n",
                ),
                "ssh://jump@gw:2200" => parse_ssh_g(
                    "user jump\nhostname gw.example\nport 2200\nproxyjump edge\n",
                ),
                "edge" => parse_ssh_g("user e\nhostname edge.example\nport 22\n"),
                "inner" => parse_ssh_g(
                    "user i\nhostname inner.lan\nport 22\nproxyjump ignored\n",
                ),
                "ts" => parse_ssh_g(
                    "user u\nhostname x\nport 22\nproxycommand tailscale nc %h %p\n",
                ),
                other => bail!("unknown {other}"),
            })
        };
        let (hops, files) = resolve_chain("brugg", &fake).unwrap();
        let names: Vec<&str> = hops.iter().map(|h| h.hostname.as_str()).collect();
        assert_eq!(
            names,
            ["edge.example", "gw.example", "inner.lan", "10.0.0.2"]
        );
        assert_eq!(files, ["~/.ssh/known_hosts"]);
        assert!(resolve_chain("ts", &fake)
            .unwrap_err()
            .to_string()
            .contains("ProxyCommand"));

        let conf = hops_config(
            &hops,
            Path::new("/r/known_hosts"),
            Path::new("/r/admin-key"),
        );
        assert!(conf.contains("Host neo-hop-3\n  HostName 10.0.0.2\n  User homeserver\n  Port 22\n  HostKeyAlias neo-hop-3\n  ProxyJump neo-hop-2\n"), "{conf}");
        assert!(!conf.contains("Host neo-hop-0\n  HostName edge.example\n  User e\n  Port 22\n  HostKeyAlias neo-hop-0\n  ProxyJump"));
        assert!(conf.contains("StrictHostKeyChecking yes"));
    }

    #[test]
    fn known_hosts_lines_are_renamed() {
        let found = "# Host brugg found: line 3\n|1|abc=|def= ssh-ed25519 AAAAK1\nbrugg,10.0.0.2 ecdsa-sha2-nistp256 AAAAK2 comment\n@revoked brugg ssh-rsa AAAAK3\n";
        assert_eq!(
            rename_known_hosts(found, "neo-hop-0"),
            [
                "neo-hop-0 ssh-ed25519 AAAAK1",
                "neo-hop-0 ecdsa-sha2-nistp256 AAAAK2"
            ]
        );
    }
}
