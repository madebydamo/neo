//! What the new server looks like before anything changes: reachable as root,
//! its disks (by-id names) and whether it has a public address.

use anyhow::{bail, Context, Result};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::commands::restore::remote::{split_port, INSTALLER_TEST};

/// An SSH login (`user@host[:port]`) with optional key.
#[derive(Clone, Debug)]
pub struct Login {
    pub user_host: String,
    pub port: Option<String>,
    pub key: Option<PathBuf>,
    /// The host key changes (kexec installer, fresh install): never stored.
    pub ephemeral: bool,
}

impl Login {
    pub fn new(spec: &str, key: Option<&Path>, ephemeral: bool) -> Self {
        let (user_host, port) = split_port(spec);
        Self {
            user_host,
            port,
            key: key.map(Path::to_path_buf),
            ephemeral,
        }
    }

    /// Host part without the user (IPv6 without brackets).
    pub fn host(&self) -> &str {
        self.user_host
            .rsplit_once('@')
            .map(|(_, h)| h)
            .unwrap_or(&self.user_host)
    }

    pub fn user(&self) -> &str {
        self.user_host
            .split_once('@')
            .map(|(u, _)| u)
            .unwrap_or("root")
    }

    /// `user@host:port` again (as typed, with the port).
    pub fn spec(&self) -> String {
        match &self.port {
            Some(p) if self.host().contains(':') => {
                format!("{}@[{}]:{p}", self.user(), self.host())
            }
            Some(p) => format!("{}:{p}", self.user_host),
            None => self.user_host.clone(),
        }
    }

    pub fn with_user(&self, user: &str) -> Self {
        Self {
            user_host: format!("{user}@{}", self.host()),
            ..self.clone()
        }
    }

    pub fn opts(&self) -> Vec<String> {
        let mut o: Vec<String> = vec![
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "ConnectTimeout=10".into(),
            "-o".into(),
            "ServerAliveInterval=15".into(),
        ];
        if self.ephemeral {
            for s in [
                "StrictHostKeyChecking=no",
                "UserKnownHostsFile=/dev/null",
                "LogLevel=ERROR",
            ] {
                o.extend(["-o".into(), s.into()]);
            }
        } else {
            o.extend(["-o".into(), "StrictHostKeyChecking=accept-new".into()]);
        }
        if let Some(p) = &self.port {
            o.extend(["-p".into(), p.clone()]);
        }
        if let Some(k) = &self.key {
            o.extend(["-i".into(), k.display().to_string()]);
        }
        o
    }

    pub fn ssh(&self, script: &str) -> Command {
        let mut c = Command::new("ssh");
        c.args(self.opts()).arg(&self.user_host).arg(script);
        c
    }

    pub fn ok(&self, script: &str) -> bool {
        self.ssh(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// stdout of `script`; fails with its stderr.
    pub fn output(&self, script: &str) -> Result<String> {
        let out = self
            .ssh(script)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("ssh {}", self.user_host))?;
        if !out.status.success() {
            bail!(
                "{} on {} failed: {}",
                script.split_whitespace().next().unwrap_or("command"),
                self.user_host,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

/// The kexec installer nixos-anywhere boots (same release it uses).
const KEXEC_URL: &str = "https://github.com/nix-community/nixos-images/releases/download/nixos-25.05/nixos-kexec-installer-noninteractive-$(uname -m)-linux.tar.gz";

/// Boot the NixOS installer into RAM through a user with sudo (asked on this
/// terminal, which nixos-anywhere cannot do). The installer takes the keys of
/// root and of that user, so `root@` logs in afterwards.
pub fn kexec_with_sudo(login: &Login) -> Result<()> {
    let script = format!(
        "set -e; d=\"$HOME/neo-kexec\"; rm -rf \"$d\"; mkdir -p \"$d\"; cd \"$d\"; \
         echo 'Downloading the NixOS installer …'; \
         if command -v curl >/dev/null; then curl -fsSL \"{KEXEC_URL}\"; else wget -qO- \"{KEXEC_URL}\"; fi | tar -xz; \
         echo 'sudo starts it (your password for {user} may be asked):'; \
         sudo TMPDIR=\"$d\" setsid --wait \"$d/kexec/run\" 2>&1 | tee \"$d/run.log\" || true; \
         grep -q 'machine will boot into nixos' \"$d/run.log\"",
        user = login.user()
    );
    let mut c = Command::new("ssh");
    c.arg("-t")
        .args(login.opts())
        .arg(&login.user_host)
        .arg(script);
    let status = c.status().context("ssh")?;
    if !status.success() {
        bail!(
            "starting the installer on {} failed (see above)",
            login.user_host
        );
    }
    Ok(())
}

/// Wait until `root@` logs in to a NixOS installer there.
pub fn wait_for_installer(root: &Login, timeout: std::time::Duration) -> Result<()> {
    let start = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_secs(15));
    while start.elapsed() < timeout {
        if root.ok(INSTALLER_TEST) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
    bail!("{} did not come back as a NixOS installer", root.user_host)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    /// Kernel name (sda, nvme0n1, vda).
    pub name: String,
    pub size: u64,
    pub model: String,
    /// The stable name to install onto: a /dev/disk/by-id link, else /dev/<name>.
    pub path: String,
}

impl Disk {
    pub fn describe(&self) -> String {
        let model = if self.model.is_empty() {
            String::new()
        } else {
            format!(", {}", self.model)
        };
        format!(
            "{} ({}, {}{model})",
            self.path,
            self.name,
            human_size(self.size)
        )
    }
}

#[derive(Debug, Default)]
pub struct Probe {
    pub os: String,
    pub installer: bool,
    pub disks: Vec<Disk>,
    /// Global addresses on its interfaces.
    pub addresses: Vec<IpAddr>,
}

impl Probe {
    pub fn public_addresses(&self) -> Vec<IpAddr> {
        self.addresses.iter().copied().filter(is_public).collect()
    }
}

const PROBE_SCRIPT: &str = r#"
. /etc/os-release 2>/dev/null; echo "OS ${PRETTY_NAME:-unknown}"
if { @INSTALLER_TEST@; }; then echo INSTALLER; fi
for l in /dev/disk/by-id/*; do
  case "$l" in *-part[0-9]*) continue;; esac
  [ -e "$l" ] && echo "ID $(readlink -f "$l") $l"
done
lsblk -dnbP -o NAME,SIZE,TYPE,MODEL 2>/dev/null | sed 's/^/DISK /'
ip -o addr show scope global 2>/dev/null | awk '{print "IP", $4}'
"#;

pub fn probe(login: &Login) -> Result<Probe> {
    if !login.ok("true") {
        bail!(
            "cannot log in to {} with your SSH key. Put your public key into root's authorized_keys there (ssh-copy-id {}) and run again",
            login.user_host,
            login.user_host
        );
    }
    let script = PROBE_SCRIPT.replace("@INSTALLER_TEST@", INSTALLER_TEST);
    let out = login.output(&format!("sh -c {}", shell_single(&script)))?;
    Ok(parse_probe(&out))
}

fn shell_single(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// `KEY="value" …` pairs of `lsblk -P`.
fn pairs(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = line.trim();
    while let Some((k, after)) = rest.split_once("=\"") {
        let Some((v, tail)) = after.split_once('"') else {
            break;
        };
        out.push((k.trim().to_string(), v.to_string()));
        rest = tail;
    }
    out
}

/// The by-id link to show for a disk: not the wwn-/eui. ones when a
/// readable one exists.
fn best_id<'a>(ids: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut ids: Vec<&str> = ids.collect();
    ids.sort_by_key(|p| {
        let name = p.rsplit('/').next().unwrap_or(p);
        let opaque = name.starts_with("wwn-")
            || name.starts_with("nvme-eui.")
            || name.starts_with("nvme-nvme.");
        (opaque, name.len())
    });
    ids.first().copied()
}

pub fn parse_probe(out: &str) -> Probe {
    let mut p = Probe::default();
    let mut ids: Vec<(String, String)> = Vec::new();
    for line in out.lines() {
        if let Some(os) = line.strip_prefix("OS ") {
            p.os = os.trim().to_string();
        } else if line.trim() == "INSTALLER" {
            p.installer = true;
        } else if let Some(rest) = line.strip_prefix("ID ") {
            if let Some((dev, link)) = rest.split_once(' ') {
                ids.push((dev.to_string(), link.trim().to_string()));
            }
        } else if let Some(rest) = line.strip_prefix("DISK ") {
            let kv = pairs(rest);
            let get = |k: &str| {
                kv.iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            if get("TYPE") != "disk" {
                continue;
            }
            let name = get("NAME");
            let size: u64 = get("SIZE").parse().unwrap_or(0);
            // Floppies, RAM disks and anything below 1 GB are no install target.
            let virtual_dev = ["zram", "loop", "ram", "fd", "sr"]
                .iter()
                .any(|p| name.starts_with(p));
            if virtual_dev || size < 1_000_000_000 {
                continue;
            }
            p.disks.push(Disk {
                size,
                model: get("MODEL").trim().to_string(),
                path: format!("/dev/{name}"),
                name,
            });
        } else if let Some(cidr) = line.strip_prefix("IP ") {
            let addr = cidr.split('/').next().unwrap_or("");
            if let Ok(ip) = addr.parse() {
                p.addresses.push(ip);
            }
        }
    }
    for d in &mut p.disks {
        let dev = format!("/dev/{}", d.name);
        if let Some(id) = best_id(
            ids.iter()
                .filter(|(target, _)| *target == dev)
                .map(|(_, l)| l.as_str()),
        ) {
            d.path = id.to_string();
        }
    }
    p
}

/// Reachable from the internet: not private, shared (CGNAT), loopback or
/// link-local; IPv6 not unique-local.
pub fn is_public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            let cgnat = o[0] == 100 && (o[1] & 0xc0) == 64;
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_documentation()
                || cgnat)
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            let unique_local = (first & 0xfe00) == 0xfc00;
            let link_local = (first & 0xffc0) == 0xfe80;
            !(v6.is_loopback() || v6.is_unspecified() || unique_local || link_local)
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else {
        format!("{gb:.0} GB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_probe() {
        let out = "OS Ubuntu 24.04 LTS\n\
            ID /dev/sda /dev/disk/by-id/wwn-0x5000\n\
            ID /dev/sda /dev/disk/by-id/ata-Samsung_SSD_870_S1\n\
            ID /dev/nvme0n1 /dev/disk/by-id/nvme-eui.0025\n\
            DISK NAME=\"sda\" SIZE=\"500107862016\" TYPE=\"disk\" MODEL=\"Samsung SSD 870\"\n\
            DISK NAME=\"nvme0n1\" SIZE=\"1000204886016\" TYPE=\"disk\" MODEL=\"\"\n\
            DISK NAME=\"vda\" SIZE=\"21474836480\" TYPE=\"disk\" MODEL=\"\"\n\
            DISK NAME=\"sr0\" SIZE=\"1\" TYPE=\"rom\" MODEL=\"QEMU\"\n\
            DISK NAME=\"zram0\" SIZE=\"1\" TYPE=\"disk\" MODEL=\"\"\n\
            DISK NAME=\"fd0\" SIZE=\"4096\" TYPE=\"disk\" MODEL=\"\"\n\
            IP 192.168.1.20/24\n\
            IP 203.0.113.7/24\n\
            IP 2a01:4f8::1/64\n\
            IP fd00::5/64\n";
        let p = parse_probe(out);
        assert_eq!(p.os, "Ubuntu 24.04 LTS");
        assert!(!p.installer);
        let paths: Vec<&str> = p.disks.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/dev/disk/by-id/ata-Samsung_SSD_870_S1",
                "/dev/disk/by-id/nvme-eui.0025",
                "/dev/vda"
            ]
        );
        assert_eq!(p.disks[0].model, "Samsung SSD 870");
        assert_eq!(p.disks[0].size, 500107862016);
        let public: Vec<String> = p.public_addresses().iter().map(|i| i.to_string()).collect();
        // 203.0.113.0/24 is a documentation range, so only the IPv6 one is public.
        assert_eq!(public, ["2a01:4f8::1"]);
    }

    #[test]
    fn public_addresses() {
        for (ip, public) in [
            ("10.1.2.3", false),
            ("172.20.0.1", false),
            ("100.100.1.1", false),
            ("100.128.0.1", true),
            ("95.217.1.2", true),
            ("fe80::1", false),
            ("fd12::1", false),
            ("2001:4860::8888", true),
        ] {
            assert_eq!(is_public(&ip.parse().unwrap()), public, "{ip}");
        }
    }

    #[test]
    fn login_parts() {
        let l = Login::new("homeserver@edge.example.com:2222", None, false);
        assert_eq!(l.host(), "edge.example.com");
        assert_eq!(l.port.as_deref(), Some("2222"));
        assert_eq!(l.with_user("root").user_host, "root@edge.example.com");
        assert_eq!(l.user(), "homeserver");
        assert_eq!(l.with_user("root").spec(), "root@edge.example.com:2222");
    }
}
