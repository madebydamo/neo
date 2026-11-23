//! `--host` with `--from-store`: how the new machine reaches the store's sshd
//! (docs/design/backup-store-ssh.md §4.4). The new machine has none of your
//! aliases, keys or known_hosts.
//!
//! - direct: your login's route resolved here (`ssh -G`), your known_hosts
//!   lines for its hosts, and your agent over `ssh -A` for one master
//!   connection per client, which the restore unit reuses.
//! - relay: `-L` from this computer to the store over your login, `-R` from
//!   the new machine to that port. Needs this computer until the data is copied.

use anyhow::{bail, Context, Result};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use super::local::WORK_DIR;
use super::remote::{Target, ADMIN_KEY, BACKUP_KEY};
use super::StoreVia;
use crate::utils::backup::store_ssh::{
    self, hop_name, hops_config, hops_known_hosts, master_socket, resolve_chain, Connect,
    StoreRoute,
};
use crate::utils::command::{shell_quote, shell_words};

/// On the new machine: the routes `neo restore` there reads (`--store-routes`).
pub(super) const ROUTES_FILE: &str = "store-routes.json";
/// The pinned store host keys, where the local mode writes them too.
const STORE_KNOWN_HOSTS: &str = "known_hosts.store";

/// The open ways; the relays stop when this is dropped, the masters with
/// [`StorePaths::close`].
#[derive(Default)]
pub(super) struct StorePaths {
    relays: Vec<Child>,
    /// Master sockets on the new machine.
    masters: Vec<String>,
    /// For the plan: how each store is reached.
    pub lines: Vec<String>,
}

impl StorePaths {
    /// Whether every relay still runs.
    pub fn relays_alive(&mut self) -> bool {
        self.relays
            .iter_mut()
            .all(|c| matches!(c.try_wait(), Ok(None)))
    }

    pub fn has_relay(&self) -> bool {
        !self.relays.is_empty()
    }

    pub fn close(&mut self, target: &Target) {
        for sock in self.masters.drain(..) {
            target.ssh_ok(
                true,
                &format!(
                    "ssh -S {} -O exit neo-store 2>/dev/null",
                    shell_quote(&sock)
                ),
            );
        }
        for c in &mut self.relays {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.relays.clear();
    }
}

impl Drop for StorePaths {
    fn drop(&mut self) {
        for c in &mut self.relays {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// The keys on the new machine for the store's accounts.
fn key_args(has_backup_key: bool) -> Vec<String> {
    let mut v = vec!["-i".to_string(), format!("{WORK_DIR}/{ADMIN_KEY}")];
    if has_backup_key {
        v.extend(["-i".to_string(), format!("{WORK_DIR}/{BACKUP_KEY}")]);
    }
    v
}

/// Open a way to each store for the clients that use it; returns the routes
/// as the new machine sees them.
pub(super) fn open(
    target: &Target,
    routes: &[StoreRoute],
    clients: &[Vec<String>],
    via: StoreVia,
    has_backup_key: bool,
) -> Result<(Vec<StoreRoute>, StorePaths)> {
    let mut paths = StorePaths::default();
    let mut out = Vec::new();
    for (i, (route, clients)) in routes.iter().zip(clients).enumerate() {
        let Connect::Jump { login } = &route.connect else {
            bail!("store route {i} is no login");
        };
        if clients.is_empty() {
            println!(
                "! No destination of this config uses the store {} and nothing was picked in it: it is not used",
                route.url
            );
            continue;
        }
        pin_store_key(target, route)?;
        if via != StoreVia::Relay {
            println!("Trying the direct way from {} to {login} …", target.host);
            match direct(target, i, route, login, clients, has_backup_key) {
                Ok((r, socks)) => {
                    paths.masters.extend(socks);
                    paths.lines.push(format!(
                        "{}: direct; uses your SSH agent on {} to log in to {login}",
                        route.url, target.host
                    ));
                    out.push(r);
                    continue;
                }
                Err(e) if via == StoreVia::Direct => {
                    return Err(e.context("the direct way to the store failed (--store-via direct)"))
                }
                Err(e) => println!("✗ direct: {e:#}. Using the relay through this computer."),
            }
        }
        let (r, children) = relay(target, route, login, clients, has_backup_key)?;
        paths.relays.extend(children);
        paths.lines.push(format!(
            "{}: relay through this computer — keep this terminal open until the data is copied (step 6)",
            route.url
        ));
        out.push(r);
    }
    Ok((out, paths))
}

/// The store's host key on the new machine, under its alias.
fn pin_store_key(target: &Target, route: &StoreRoute) -> Result<()> {
    let line = format!("{} {}", route.host_alias(), route.host_key);
    let file = format!("{WORK_DIR}/{STORE_KNOWN_HOSTS}");
    let script = format!(
        "umask 077; mkdir -p {WORK_DIR}; touch {file}; grep -qxF {l} {file} || echo {l} >> {file}",
        l = shell_quote(&line)
    );
    if !target.ssh_ok(true, &script) {
        bail!("writing the store's host key on {} failed", target.host);
    }
    Ok(())
}

fn inner_opts(route: &StoreRoute, known_hosts: &str) -> Vec<String> {
    vec![
        "-o".into(),
        format!("HostKeyAlias={}", route.host_alias()),
        "-o".into(),
        format!("UserKnownHostsFile={known_hosts}"),
        "-o".into(),
        "StrictHostKeyChecking=yes".into(),
    ]
}

/// The master connections of `clients` on the new machine, through the
/// resolved route of `login`, logged in with your forwarded agent.
fn direct(
    target: &Target,
    i: usize,
    route: &StoreRoute,
    login: &str,
    clients: &[String],
    has_backup_key: bool,
) -> Result<(StoreRoute, Vec<String>)> {
    if std::env::var_os("SSH_AUTH_SOCK").is_none_or(|s| s.is_empty()) {
        println!("  (no SSH agent here: the new machine logs in to {login} with your admin key)");
    }
    let (hops, files) = resolve_chain(login, &store_ssh::ssh_g)?;
    let dir = format!("{WORK_DIR}/store-{i}");
    let known = format!("{dir}/known_hosts");
    let mut known_text = hops_known_hosts(&hops, &files)?;
    known_text.push_str(&format!("{} {}\n", route.host_alias(), route.host_key));
    let config = hops_config(
        &hops,
        std::path::Path::new(&known),
        std::path::Path::new(&format!("{WORK_DIR}/{ADMIN_KEY}")),
    );
    for (name, text) in [("ssh_config", config), ("known_hosts", known_text)] {
        target.write_file(&dir, name, text.as_bytes())?;
    }
    let last = hop_name(hops.len() - 1);
    let mut socks: Vec<String> = Vec::new();
    for client in clients {
        let sock = master_socket(std::path::Path::new(&dir), client)
            .display()
            .to_string();
        let log = format!("{dir}/{client}.log");
        let mut master: Vec<String> = vec![
            "ssh".into(),
            "-F".into(),
            format!("{dir}/ssh_config"),
            "-f".into(),
            "-N".into(),
            "-M".into(),
            "-S".into(),
            sock.clone(),
            "-o".into(),
            "ControlPersist=yes".into(),
            "-J".into(),
            last.clone(),
            "-p".into(),
            route.ssh_port.to_string(),
        ];
        master.extend(inner_opts(route, &known));
        master.extend(key_args(has_backup_key));
        master.push(format!("backup-{client}@127.0.0.1"));
        let probe = shell_words([
            "ssh",
            "-S",
            &sock,
            "-o",
            "ControlMaster=no",
            "-o",
            "BatchMode=yes",
            "-p",
            &route.ssh_port.to_string(),
            &format!("backup-{client}@127.0.0.1"),
            "true",
        ]);
        let script = format!(
            "rm -f {s}; timeout 40 {m} </dev/null >{l} 2>&1 && timeout 20 {probe} </dev/null >>{l} 2>&1 || {{ tail -n 3 {l} >&2; exit 1; }}",
            s = shell_quote(&sock),
            m = shell_words(&master),
            l = shell_quote(&log),
        );
        let out = target
            .ssh_agent(&script)
            .stdin(Stdio::null())
            .output()
            .context("open the master connection")?;
        if !out.status.success() {
            for s in &socks {
                target.ssh_ok(
                    true,
                    &format!("ssh -S {} -O exit neo-store", shell_quote(s)),
                );
            }
            bail!(
                "{} did not let the new machine in through {login}: {}",
                route.host,
                reason(&String::from_utf8_lossy(&out.stderr))
            );
        }
        println!("✓ {} reaches the store for {client} directly", target.host);
        socks.push(sock);
    }
    Ok((
        StoreRoute {
            connect: Connect::Master { dir: dir.into() },
            ..route.clone()
        },
        socks,
    ))
}

/// ssh's last words, said plainly.
fn reason(log: &str) -> String {
    let log = log.trim();
    if log.contains("Host key verification failed") || log.contains("No ED25519 host key") {
        format!("a host key did not match your known_hosts ({log})")
    } else if log.contains("timed out")
        || log.contains("Could not resolve")
        || log.contains("No route")
    {
        format!("it did not answer from the new machine ({log})")
    } else if log.contains("Permission denied") {
        format!("the login was refused ({log})")
    } else if log.is_empty() {
        "no answer within 40 s".into()
    } else {
        log.to_string()
    }
}

fn free_local_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// A port for the new machine's end of the relay.
fn remote_port(attempt: u32) -> u16 {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    30000 + ((n / 7 + attempt * 7919) % 30000) as u16
}

/// This computer carries the bytes: `-L` over your login, `-R` to the new machine.
fn relay(
    target: &Target,
    route: &StoreRoute,
    login: &str,
    clients: &[String],
    has_backup_key: bool,
) -> Result<(StoreRoute, Vec<Child>)> {
    let p = free_local_port()?;
    println!(
        "Opening the relay: {} → this computer → {login} → the store …",
        target.host
    );
    let mut local = Command::new("ssh")
        .args([
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "ServerAliveCountMax=10",
            "-L",
            &format!("127.0.0.1:{p}:127.0.0.1:{}", route.ssh_port),
            login,
        ])
        .stdin(Stdio::null())
        .spawn()
        .context("ssh -L to the store")?;
    let mut last_err = String::new();
    for attempt in 0..3 {
        let q = remote_port(attempt);
        let mut remote = target
            .ssh_command(true)
            .args([
                "-N",
                "-o",
                "ExitOnForwardFailure=yes",
                "-R",
                &format!("127.0.0.1:{q}:127.0.0.1:{p}"),
            ])
            .arg(&target.host)
            .stdin(Stdio::null())
            .spawn()
            .context("ssh -R to the new machine")?;
        let known = format!("{WORK_DIR}/{STORE_KNOWN_HOSTS}");
        let mut probe: Vec<String> = vec![
            "ssh".into(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "ConnectTimeout=10".into(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
        ];
        probe.extend(inner_opts(route, &known));
        probe.extend(key_args(has_backup_key));
        probe.extend([
            "-p".into(),
            q.to_string(),
            format!("backup-{}@127.0.0.1", clients[0]),
            "true".into(),
        ]);
        let probe = format!("{} </dev/null", shell_words(&probe));
        let start = Instant::now();
        loop {
            if !matches!(local.try_wait(), Ok(None)) {
                let _ = remote.kill();
                bail!("ssh -L {p} through {login} stopped: check that `ssh {login}` works");
            }
            if !matches!(remote.try_wait(), Ok(None)) {
                last_err = format!("port {q} on the new machine is taken, or the login failed");
                break;
            }
            if target.ssh_ok(true, &probe) {
                println!("✓ The relay works (new machine port {q})");
                return Ok((
                    StoreRoute {
                        connect: Connect::Port { port: q },
                        ..route.clone()
                    },
                    vec![local, remote],
                ));
            }
            if start.elapsed() > Duration::from_secs(30) {
                let _ = remote.kill();
                let _ = local.kill();
                bail!("the relay to the store did not answer within 30 s (the store refused the key, or {login} cannot reach 127.0.0.1:{})", route.ssh_port);
            }
            sleep(Duration::from_secs(1));
        }
        let _ = remote.wait();
    }
    let _ = local.kill();
    bail!("the relay could not open: {last_err}")
}
