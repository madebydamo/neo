//! `neo bootstrap DIR`: install Neo on a new server from this computer
//! (docs/design/bootstrap.md).
//!
//! A wizard asks what a first install needs (server, name, domain, logins,
//! disk, how the internet reaches it), writes a config repo into DIR with
//! `neo init`, optionally opens `neo web` on it, adds the tunnel on the
//! streamproxy edge when the server has no public address, and installs with
//! `neo restore --data none` (nixos-anywhere). Rerunning it in the same DIR
//! asks again with the earlier answers as defaults.

pub mod answers;
mod edge;
mod finish;
mod probe;
pub(crate) mod prompt;

use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use toml_edit::DocumentMut;

use self::answers::{
    check_domain, check_email, check_hostname, check_username, string, strings, Access, Answers,
    Tunnel,
};
use self::edge::{Edge, Existing};
use self::probe::Login;
use self::prompt::Prompt;
use super::init::{init_repo, InitSource};
use super::restore::{DataSource, RestoreArgs, DEFAULT_TARGET};
use crate::utils::backup::default_key;
use crate::utils::command::{git_cmd, has_staged_changes};
use crate::utils::fsutil::TempDir;
use crate::utils::settings::{self, bool_at};

#[derive(clap::Args, Debug, Clone)]
pub struct BootstrapArgs {
    /// Folder for the new server's config repo. Run again with the same folder
    /// to change answers or to retry; it is kept after the install.
    pub dir: PathBuf,
    /// The new server: root@IP[:PORT]. Any Linux with root SSH (a hoster's
    /// fresh image) or a booted NixOS installer.
    #[arg(long)]
    pub host: Option<String>,
    /// Your ssh-ed25519 private key: logs in as root there and its .pub becomes
    /// an admin key (default: ~/.ssh/id_ed25519).
    #[arg(long)]
    pub key: Option<PathBuf>,
    /// Server name (hostname).
    #[arg(long)]
    pub hostname: Option<String>,
    /// Domain; apps get <app>.<domain>.
    #[arg(long)]
    pub domain: Option<String>,
    /// Email for the HTTPS certificates.
    #[arg(long)]
    pub email: Option<String>,
    /// Disk to install onto (erased), e.g. /dev/disk/by-id/….
    #[arg(long)]
    pub disk: Option<String>,
    /// The internet reaches the server directly on 80/443 (true), or through
    /// a streamproxy edge (false).
    #[arg(long)]
    pub public: Option<bool>,
    /// Streamproxy edge, an existing Neo server with a public address:
    /// [homeserver@]HOST[:PORT].
    #[arg(long)]
    pub edge: Option<String>,
    /// Neo flake the server follows (neo-cli.server.neoInput), e.g.
    /// github:madebydamo/neo/dev.
    #[arg(long)]
    pub server_neo_input: Option<String>,
    /// Open neo web on this computer for the new repo before installing.
    #[arg(long)]
    pub web: bool,
    /// Ask nothing: flags, then earlier answers. Fails when one is missing
    /// (the web login password can only be typed).
    #[arg(long)]
    pub defaults: bool,
    /// Stop before the install (repo and edge only).
    #[arg(long)]
    pub no_install: bool,
    /// Neo flake ref to run on the installer (as `neo restore --neo`).
    #[arg(long)]
    pub neo: Option<String>,
}

/// Answers that are no settings, kept for the next run in .git.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct State {
    host: Option<String>,
    key: Option<String>,
    edge: Option<String>,
    web_user: Option<String>,
}

const STATE_FILE: &str = ".git/neo-bootstrap.json";

impl State {
    fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(STATE_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save(&self, dir: &Path) -> Result<()> {
        fs::write(dir.join(STATE_FILE), serde_json::to_string_pretty(self)?)
            .context("save the bootstrap answers")
    }
}

fn section(title: &str) {
    println!("\n── {title} ──");
}

fn non_empty(s: &str) -> Result<(), String> {
    if s.trim().is_empty() {
        Err("must not be empty".into())
    } else {
        Ok(())
    }
}

fn with_user(spec: &str, user: &str) -> String {
    if spec.contains('@') {
        spec.to_string()
    } else {
        format!("{user}@{spec}")
    }
}

/// The first user name of `user:hash` lines.
fn user_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| l.split_once(':').map(|(u, _)| u.to_string()))
        .collect()
}

pub fn bootstrap(
    args: &BootstrapArgs,
    main_doc: &DocumentMut,
    profile: &str,
    nix_cmd: &str,
    dry_run: bool,
) -> Result<()> {
    let p = Prompt {
        auto: args.defaults,
    };
    let dir = std::path::absolute(&args.dir).context("resolve the folder")?;
    let dir_s = dir.display().to_string();
    let has_repo = dir.join(".git").exists() && dir.join(settings::FILE).exists();
    if !has_repo
        && dir.is_dir()
        && fs::read_dir(&dir)?
            .flatten()
            .any(|e| !e.file_name().to_string_lossy().starts_with('.'))
    {
        bail!("{dir_s} is not empty and holds no config repo: pick a new folder");
    }
    let mut doc = if has_repo {
        settings::load(&dir.join(settings::FILE))?
    } else {
        DocumentMut::new()
    };
    let mut state = State::load(&dir);
    if has_repo {
        println!("Continuing in {dir_s}: Enter keeps each earlier answer.");
    } else {
        println!("A new config repo goes into {dir_s}. Enter takes the suggestion in [brackets].");
    }

    // 1. The server.
    section("1/5 The new server");
    let host = match &args.host {
        Some(h) => with_user(h, "root"),
        None => with_user(
            &p.text(
                "Server to install ([user@]IP[:port]; root, or a user with sudo such as homeserver on a Neo server)",
                state.host.as_deref(),
                non_empty,
            )?,
            "root",
        ),
    };
    let key = args
        .key
        .clone()
        .or_else(|| state.key.as_ref().map(PathBuf::from))
        .or_else(default_key)
        .context("pass --key with your ssh-ed25519 private key")?;
    let login = Login::new(&host, Some(&key), true);
    println!("Looking at {} …", login.user_host);
    let probe = probe::probe(&login)?;
    println!(
        "  {}{}",
        probe.os,
        if probe.installer {
            " (NixOS installer)"
        } else {
            ""
        }
    );
    for d in &probe.disks {
        println!("  disk {}", d.describe());
    }
    let public_ips = probe.public_addresses();
    if !probe.addresses.is_empty() {
        let list: Vec<String> = probe.addresses.iter().map(|a| a.to_string()).collect();
        println!("  addresses {}", list.join(", "));
    }
    state.host = Some(host.clone());
    state.key = Some(key.display().to_string());

    // 2. Name and domain.
    section("2/5 Name and domain");
    let hostname = match &args.hostname {
        Some(h) => h.clone(),
        None => p.text(
            "Server name",
            Some(&string(&doc, &["core", "hostname"]).unwrap_or_else(|| "homeserver".into())),
            check_hostname,
        )?,
    };
    check_hostname(&hostname).map_err(anyhow::Error::msg)?;
    let domain = match &args.domain {
        Some(d) => d.clone(),
        None => p.text(
            "Domain (apps become <app>.<domain>)",
            string(&doc, &["services", "swag", "domain"]).as_deref(),
            check_domain,
        )?,
    };
    check_domain(&domain).map_err(anyhow::Error::msg)?;
    let email = match &args.email {
        Some(e) => e.clone(),
        None => p.text(
            "Email for the HTTPS certificates (Let's Encrypt)",
            string(&doc, &["services", "swag", "email"]).as_deref(),
            check_email,
        )?,
    };

    // 3. Logins.
    section("3/5 Logins");
    let mut keys = strings(&doc, &["core", "ssh", "authorizedKeys"]);
    match finish::public_key_of(&key) {
        Ok(own) => {
            let short = |k: &str| k.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
            let known = keys.iter().any(|k| short(k) == short(&own));
            if !known
                && (keys.is_empty()
                    || p.yes_no(&format!("Add your key ({own}) to the SSH keys?"), true)?)
            {
                keys.push(own);
            }
        }
        Err(e) => println!("! {e:#}"),
    }
    if keys.is_empty() {
        bail!(
            "no SSH key for the new server: put your public key next to {} (.pub)",
            key.display()
        );
    }
    println!("SSH keys: {} (log in as homeserver@ or admin@)", keys.len());

    let existing_users = strings(&doc, &["services", "tinyauth", "users"]);
    let web_users = if !existing_users.is_empty()
        && p.yes_no(
            &format!(
                "Keep the web login {}?",
                user_names(&existing_users).join(", ")
            ),
            true,
        )? {
        existing_users
    } else {
        let default_user = state
            .web_user
            .clone()
            .or_else(|| env::var("USER").ok())
            .filter(|u| check_username(u).is_ok() && u != "root");
        let user = p.text(
            "Web login user (for the Neo web UI and your apps)",
            default_user.as_deref(),
            check_username,
        )?;
        let pw = p
            .password("Web login password", false)?
            .context("a web login password is needed")?;
        vec![finish::web_user_line(&user, &pw)?]
    };
    state.web_user = user_names(&web_users).into_iter().next();

    let has_linux_pw = string(&doc, &["core", "hashedLinuxPassword"]).is_some();
    let linux_password_hash =
        if has_linux_pw && p.yes_no("Keep the Linux password (console and sudo)?", true)? {
            None
        } else {
            match p.password(
                "Linux password for the console and sudo (empty: SSH keys only)",
                true,
            )? {
                Some(pw) => Some(finish::linux_password_hash(&pw)?),
                None if has_linux_pw => Some(String::new()),
                None => None,
            }
        };

    // 4. Disk.
    section("4/5 Disk (erased by the install)");
    let current_disk = string(&doc, &["disko", "mainDisk"]);
    let disk = match &args.disk {
        Some(d) => d.clone(),
        None if probe.disks.is_empty() => {
            p.text("Disk to install onto", current_disk.as_deref(), non_empty)?
        }
        None => {
            let options: Vec<String> = probe.disks.iter().map(|d| d.describe()).collect();
            let default = current_disk
                .as_deref()
                .and_then(|c| probe.disks.iter().position(|d| d.path == c))
                .unwrap_or(0);
            let i = p.choose("Disk to install onto", &options, default)?;
            probe.disks[i].path.clone()
        }
    };
    let additional_disks = ask_additional_disks(&p, &doc, &probe, &disk)?;
    let snapshots_before = bool_at(&doc, &["disko", "autoSnapshot", "enabled"]).unwrap_or(true);
    let auto_snapshot = Some(p.yes_no(
        "Automatic snapshots of the Neo data (every 15 minutes, kept up to 3 months)?",
        snapshots_before,
    )?);

    // 5. How the internet reaches it.
    section("5/5 Reachable from the internet");
    let tunnel_before = bool_at(&doc, &["services", "rathole", "enabled"]).unwrap_or(false);
    let public = match args.public {
        Some(b) => b,
        None => {
            let found = if public_ips.is_empty() {
                "no public address found on it".to_string()
            } else {
                let l: Vec<String> = public_ips.iter().map(|a| a.to_string()).collect();
                format!("public address {}", l.join(", "))
            };
            p.yes_no(
                &format!(
                    "Does the internet reach this server directly on ports 80 and 443 ({found})? No: through a streamproxy edge"
                ),
                !tunnel_before && !public_ips.is_empty(),
            )?
        }
    };
    let mut edge = None;
    let access = if public {
        Access::Public
    } else {
        let spec = match &args.edge {
            Some(e) => e.clone(),
            None => p.text(
                "Streamproxy edge ([homeserver@]host of a Neo server with a public address)",
                state.edge.as_deref(),
                non_empty,
            )?,
        };
        println!("Connecting to the edge …");
        let e = Edge::connect(&spec)?;
        state.edge = Some(spec);
        let entries = e.entries()?;
        if !entries.is_empty() {
            let l: Vec<String> = entries.iter().map(|(n, u)| format!("{n} ({u})")).collect();
            println!("  tunnels on the edge: {}", l.join(", "));
        }
        let default_name =
            string(&doc, &["services", "rathole", "name"]).unwrap_or_else(|| hostname.clone());
        let check_name = |n: &str| -> Result<(), String> {
            check_hostname(n)?;
            match entries.iter().find(|(name, _)| name == n) {
                Some((_, url)) if url != &domain => Err(format!(
                    "the edge uses {n} for {url} already; pick another name"
                )),
                _ => Ok(()),
            }
        };
        let name = p.text("Tunnel name on the edge", Some(&default_name), check_name)?;
        let token = match string(&doc, &["services", "rathole", "token"]) {
            Some(t) => t,
            None => edge::random_token()?,
        };
        // The SSH name can be private (ssh_config alias, NetBird): the tunnel
        // needs the edge's public address.
        let edge_public = e.public_addresses()?;
        let suggested = string(&doc, &["services", "rathole", "remoteAddr"])
            .or_else(|| {
                edge_public
                    .iter()
                    .find(|a| a.is_ipv4())
                    .or(edge_public.first())
                    .map(|a| a.to_string())
            })
            .unwrap_or_else(|| e.address().to_string());
        let remote_addr = p.text(
            "Public address of the edge (the tunnel connects there)",
            Some(&suggested),
            non_empty,
        )?;
        let tunnel = Tunnel {
            remote_addr,
            name,
            token,
        };
        edge = Some(e);
        Access::Edge(tunnel)
    };

    let a = Answers {
        hostname: hostname.clone(),
        domain,
        email,
        authorized_keys: keys,
        web_users,
        linux_password_hash,
        disk,
        additional_disks,
        auto_snapshot,
        access,
        server_neo_input: args.server_neo_input.clone(),
    };
    answers::apply(&mut doc, &a);
    if dry_run {
        println!("DRY-RUN: would write {dir_s}/settings.toml:\n{doc}");
        return Ok(());
    }

    // The repo.
    section("Config repo");
    let tmp = TempDir::new("neo-bootstrap")?;
    let source = tmp.join(settings::FILE);
    settings::save(&source, &mut doc)?;
    init_repo(
        &dir_s,
        main_doc,
        profile,
        false,
        nix_cmd,
        &InitSource {
            settings: &source,
            hardware: false,
        },
    )?;
    drop(tmp);
    state.save(&dir)?;

    if args.web
        || p.yes_no(
            "Open the Neo web UI on this computer to change more settings before installing?",
            false,
        )?
    {
        web_step(&dir)?;
    }
    let doc = settings::load(&dir.join(settings::FILE))?;
    let domain =
        string(&doc, &["services", "swag", "domain"]).context("services.swag.domain is empty")?;
    let tunnel = bool_at(&doc, &["services", "rathole", "enabled"]).unwrap_or(false);

    // The edge.
    if tunnel {
        section("Streamproxy edge");
        let e = match edge {
            Some(e) => e,
            None => Edge::connect(
                state
                    .edge
                    .as_deref()
                    .context("rathole is on but no edge is known: run again and answer No to the internet question")?,
            )?,
        };
        let name = string(&doc, &["services", "rathole", "name"])
            .context("services.rathole.name is empty")?;
        let token = string(&doc, &["services", "rathole", "token"])
            .context("services.rathole.token is empty")?;
        let push = match e.existing(&name, &domain)? {
            Existing::None => true,
            Existing::Same => p.yes_no(
                &format!("The edge already has the tunnel {name} for {domain}. Write its password again and activate the edge?"),
                false,
            )?,
            Existing::Other(url) => bail!("the edge uses the tunnel {name} for {url}: change the tunnel name and run again"),
        };
        if push {
            e.push(&name, &domain, &token)?;
            e.activate()?;
            println!("✓ The edge forwards {domain} and *.{domain} to the tunnel {name}");
        }
        edge = Some(e);
    }

    // DNS.
    section("DNS");
    let neo_host = format!(
        "{}.{domain}",
        string(&doc, &["services", "neo", "subdomain"]).unwrap_or_else(|| "neo".into())
    );
    let (expected, via) = match &edge {
        Some(e) if tunnel => {
            let mut ips = e.public_addresses()?;
            if let Some(addr) = string(&doc, &["services", "rathole", "remoteAddr"]) {
                ips.extend(finish::resolve(&addr));
            }
            ips.sort();
            ips.dedup();
            (ips, format!("the edge {}", e.address()))
        }
        _ => (public_ips.clone(), "this server".to_string()),
    };
    finish::check_dns(&[domain.clone(), neo_host.clone()], &expected, &via);

    if args.no_install {
        println!(
            "\nStopped before the install (--no-install). Run the same command again to install."
        );
        return Ok(());
    }
    section("Install");
    if !p.yes_no(
        &format!(
            "Install Neo on {} now (erases {})?",
            login.user_host, a.disk
        ),
        true,
    )? {
        println!("Not installed. Run the same command again when ready.");
        return Ok(());
    }
    // nixos-anywhere needs root (or sudo without a password): with another
    // user, start the installer through sudo here, then go on as root.
    let mut install_host = host.clone();
    if login.user() != "root" {
        let root = login.with_user("root");
        if !probe.installer {
            println!(
                "{} is no root login: the NixOS installer is started through sudo, which may ask for {}'s password.",
                login.user_host,
                login.user()
            );
            if !p.yes_no("Start it now? The running system on it stops.", true)? {
                println!("Not installed. Run the same command again when ready.");
                return Ok(());
            }
            probe::kexec_with_sudo(&login)?;
            println!("Waiting for the installer …");
            probe::wait_for_installer(&root, Duration::from_secs(600))?;
        }
        install_host = root.spec();
    }
    let restore = RestoreArgs {
        repo: Some(dir_s.clone()),
        from_backup: None,
        from_store: Vec::new(),
        client: None,
        store_via: Default::default(),
        store_routes: None,
        host: Some(install_host.clone()),
        disk: None,
        key: Some(key.clone()),
        data: DataSource::None,
        data_from: None,
        backup_key: None,
        keep_hardware: false,
        check: false,
        yes: false,
        reboot: true,
        target: PathBuf::from(DEFAULT_TARGET),
        config: "neo".into(),
        no_install: false,
        neo: args.neo.clone(),
        origin: Some(String::new()),
        shred_key: false,
    };
    super::restore::remote::run(&restore, &install_host)?;

    // Wait for the web UI.
    section("First boot");
    let admin = login.with_user("homeserver");
    println!("Waiting for {} to boot …", admin.user_host);
    if finish::wait_for_ssh(&admin, Duration::from_secs(15 * 60)) {
        println!("  ✓ SSH works");
    } else {
        println!("  ! no SSH login after 15 minutes; check the server's console");
    }
    println!("Waiting for the web UI (the first start pulls the containers and the certificate) …");
    let pin = expected.first().copied();
    let web_ok = finish::wait_for_web(&neo_host, pin, Duration::from_secs(20 * 60))?;

    let user = state.web_user.clone().unwrap_or_default();
    println!();
    if web_ok {
        println!("✓ {hostname} is up.");
    } else {
        println!("! {hostname} is installed, but https://{neo_host} does not answer yet.");
        println!(
            "  Look at it with: ssh {} journalctl -b -u docker-swag",
            admin.user_host
        );
    }
    println!("  Web UI:  https://{neo_host}  (log in as {user})");
    println!(
        "  SSH:     ssh {}   (ssh-keygen -R {} if ssh warns about a changed host key)",
        admin.user_host,
        login.host()
    );
    println!("  The server keeps its own copy of the config repo. Once it runs as it should,");
    println!("  you can delete {dir_s}.");
    println!("  Next: Settings → Secrets (encrypt), a git remote for the config, backups.");
    Ok(())
}

/// Every other disk of the server: unused, or erased into its own pool at a
/// mount point (`disko.additionalDisks`). None keeps the setting (no disks to ask about).
fn ask_additional_disks(
    p: &Prompt,
    doc: &DocumentMut,
    probe: &probe::Probe,
    main: &str,
) -> Result<Option<Vec<(String, String)>>> {
    let others: Vec<&probe::Disk> = probe.disks.iter().filter(|d| d.path != main).collect();
    if others.is_empty() {
        return Ok(None);
    }
    let current = answers::additional_disks(doc);
    let root = string(doc, &["core", "volumes", "root"]).unwrap_or_else(|| "/var/neo".into());
    let media = format!("{root}/DATA/Media");
    let mut out = Vec::new();
    for d in others {
        let now = current
            .iter()
            .find(|(dev, _)| *dev == d.path)
            .map(|(_, m)| m.clone());
        let options = vec![
            "Not used".to_string(),
            format!("Media files ({media})"),
            format!("All Neo data ({root}): apps, files and the config"),
            "Somewhere else".to_string(),
        ];
        let default = match now.as_deref() {
            None => 0,
            Some(m) if m == media => 1,
            Some(m) if m == root => 2,
            Some(_) => 3,
        };
        let i = p.choose(
            &format!("Other disk {} (erased when used)", d.describe()),
            &options,
            default,
        )?;
        let mount = match i {
            0 => continue,
            1 => media.clone(),
            2 => root.clone(),
            _ => p.text("Mount point", now.as_deref(), |m: &str| {
                if m.starts_with('/') && m.len() > 1 {
                    Ok(())
                } else {
                    Err("an absolute path like /var/neo/DATA/Backup".into())
                }
            })?,
        };
        out.push((d.path.clone(), mount));
    }
    Ok(Some(out))
}

/// `neo web` on this computer for the new repo until Enter; its saves are
/// committed afterwards.
fn web_step(dir: &Path) -> Result<()> {
    let exe = env::current_exe().context("find this neo")?;
    let port = env::var("ROCKET_PORT").unwrap_or_else(|_| "8000".into());
    let mut child = Command::new(exe)
        .arg("--config-path")
        .arg(dir)
        .args(["--profile", "local", "web"])
        .spawn()
        .context("start neo web")?;
    println!(
        "Neo web runs at http://127.0.0.1:{port} for {}.",
        dir.display()
    );
    println!("Change and Save what you like there. Do not press Activate: it would change this computer.");
    print!("Press Enter here when you are done … ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    // SAFETY: plain signal to our own child.
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let _ = child.wait();
    let d = dir.display().to_string();
    git_cmd(&d, &["add", "-A"])?;
    if has_staged_changes(&d)? {
        git_cmd(
            &d,
            &["commit", "-q", "-m", "bootstrap: settings from the web UI"],
        )?;
        println!("✓ Committed the changes from the web UI");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_names_from_lines() {
        assert_eq!(
            user_names(&["me:$2b$x".into(), "kid:$2b$y".into()]),
            ["me", "kid"]
        );
    }

    #[test]
    fn hosts_default_to_a_user() {
        assert_eq!(with_user("1.2.3.4", "root"), "root@1.2.3.4");
        assert_eq!(with_user("admin@1.2.3.4:22", "root"), "admin@1.2.3.4:22");
    }

    #[test]
    fn state_round_trip() {
        let t = TempDir::new("neo-bootstrap-state").unwrap();
        fs::create_dir_all(t.join(".git")).unwrap();
        let s = State {
            host: Some("root@1.2.3.4".into()),
            edge: Some("edge".into()),
            ..Default::default()
        };
        s.save(t.path()).unwrap();
        let l = State::load(t.path());
        assert_eq!(l.host.as_deref(), Some("root@1.2.3.4"));
        assert_eq!(l.edge.as_deref(), Some("edge"));
    }
}
