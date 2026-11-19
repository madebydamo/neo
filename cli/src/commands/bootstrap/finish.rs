//! Password hashes, the DNS check before the install and the wait for the
//! installed server's web UI after it.

use anyhow::{bail, Context, Result};
use std::net::{IpAddr, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use super::probe::Login;
use crate::utils::command::output_with_stdin;

/// `mkpasswd` from PATH (the neo package carries it), else from nixpkgs.
fn mkpasswd() -> Command {
    let on_path = Command::new("mkpasswd")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if on_path {
        Command::new("mkpasswd")
    } else {
        let mut c = crate::utils::command::nix_command("nix");
        c.args(["shell", "nixpkgs#mkpasswd", "-c", "mkpasswd"]);
        c
    }
}

/// Hash with `mkpasswd -m <method>`, the password on stdin.
fn hash(method: &str, rounds: Option<&str>, password: &str, prefix: &str) -> Result<String> {
    let mut cmd = mkpasswd();
    cmd.args(["-m", method, "-s"]);
    if let Some(r) = rounds {
        cmd.args(["-R", r]);
    }
    let out = output_with_stdin(&mut cmd, format!("{password}\n").as_bytes(), "mkpasswd")?;
    let h = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !h.starts_with(prefix) {
        bail!("mkpasswd -m {method} gave no {prefix} hash");
    }
    Ok(h)
}

/// `user:bcrypt` for the tinyauth login.
pub fn web_user_line(user: &str, password: &str) -> Result<String> {
    Ok(format!(
        "{user}:{}",
        hash("bcrypt", Some("10"), password, "$2")?
    ))
}

/// sha-512 crypt for the Linux users.
pub fn linux_password_hash(password: &str) -> Result<String> {
    hash("sha-512", None, password, "$6$")
}

pub fn resolve(host: &str) -> Vec<IpAddr> {
    let mut v: Vec<IpAddr> = (host, 443)
        .to_socket_addrs()
        .map(|a| a.map(|s| s.ip()).collect())
        .unwrap_or_default();
    v.sort();
    v.dedup();
    v
}

/// Warn (never fail) when the domain does not point where traffic must go:
/// without it Let's Encrypt cannot issue the certificate.
pub fn check_dns(names: &[String], expected: &[IpAddr], via: &str) -> bool {
    let mut ok = true;
    let list = |v: &[IpAddr]| {
        v.iter()
            .map(IpAddr::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let target = if expected.is_empty() {
        via.to_string()
    } else {
        format!("{via} ({})", list(expected))
    };
    for name in names {
        let got = resolve(name);
        if got.is_empty() {
            println!("  ! {name} does not resolve yet (point it at {target})");
            ok = false;
        } else if expected.is_empty() || got.iter().any(|g| expected.contains(g)) {
            println!("  ✓ {name} → {}", list(&got));
        } else {
            println!(
                "  ! {name} → {}, but it should point at {target}",
                list(&got)
            );
            ok = false;
        }
    }
    if !ok {
        println!("    Without it the server gets no HTTPS certificate. Fix the DNS records (A/AAAA for the domain and *.domain); the server retries on its own.");
    }
    ok
}

/// Wait until `homeserver@` logs in on the installed system.
pub fn wait_for_ssh(login: &Login, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if login.ok("test -e /etc/neo/settings.toml") {
            return true;
        }
        sleep(Duration::from_secs(10));
    }
    false
}

/// HTTP status of `url`, with the name pinned to `ip` when given (DNS may
/// not have caught up on this computer).
fn http_status(url: &str, host: &str, ip: Option<IpAddr>) -> Option<u16> {
    let mut c = Command::new("curl");
    c.args([
        "-sk",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "--max-time",
        "10",
    ]);
    if let Some(ip) = ip {
        let ip = match ip {
            IpAddr::V6(v6) => format!("[{v6}]"),
            IpAddr::V4(v4) => v4.to_string(),
        };
        c.args(["--resolve", &format!("{host}:443:{ip}")]);
    }
    let out = c.arg(url).stderr(Stdio::null()).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Wait until the web UI answers with its login redirect (or a page).
pub fn wait_for_web(host: &str, ip: Option<IpAddr>, timeout: Duration) -> Result<bool> {
    let url = format!("https://{host}/");
    let start = Instant::now();
    let mut last = None;
    while start.elapsed() < timeout {
        let code = http_status(&url, host, ip);
        if code != last {
            println!(
                "  {url}: {}",
                code.map(|c| c.to_string())
                    .unwrap_or_else(|| "no answer yet".into())
            );
            last = code;
        }
        if matches!(code, Some(200 | 302 | 401)) {
            return Ok(true);
        }
        sleep(Duration::from_secs(15));
    }
    Ok(false)
}

/// The public key next to a private key (`<key>.pub`).
pub fn public_key_of(key: &std::path::Path) -> Result<String> {
    let pub_path = std::path::PathBuf::from(format!("{}.pub", key.display()));
    let text = std::fs::read_to_string(&pub_path)
        .with_context(|| format!("read {}", pub_path.display()))?;
    Ok(text.trim().to_string())
}
