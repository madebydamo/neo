//! The streamproxy edge: an existing Neo server with a public address, reached
//! as `homeserver@…`. Bootstrap adds the new server's tunnel there (`neo
//! settings patch`, token over stdin) and activates it.
//!
//! The edge may run an older neo without `settings`: this neo's binary is
//! copied into its store first (homeserver is a Nix trusted user; the edge
//! substitutes what it can from its caches) and run through the edge's own
//! neo wrapper, so it gets that install's environment (sops, defaults).

use anyhow::{bail, Context, Result};
use std::env;
use std::io::Write;
use std::process::Stdio;

use super::probe::Login;
use crate::utils::command::{execute_command, nix_command, shell_quote};

/// PATH for commands over ssh: sudo's wrapper first, then the system.
const REMOTE_PATH: &str = "PATH=/run/wrappers/bin:/run/current-system/sw/bin:$PATH";

pub struct Edge {
    pub login: Login,
    /// Shell function `n` that runs the neo for `settings` there (this one
    /// when it could be copied).
    neo: String,
}

/// What the edge has for a tunnel name.
#[derive(Debug, PartialEq)]
pub enum Existing {
    None,
    /// Same name, same domain.
    Same,
    /// Same name, another domain: another server's tunnel.
    Other(String),
}

impl Edge {
    /// Check the login and bring this neo there.
    pub fn connect(spec: &str) -> Result<Self> {
        let spec = spec.strip_prefix("ssh://").unwrap_or(spec);
        let spec = if spec.contains('@') {
            spec.to_string()
        } else {
            format!("homeserver@{spec}")
        };
        let login = Login::new(&spec, None, false);
        if !login.ok("true") {
            bail!(
                "cannot log in to {} with SSH (BatchMode; is your key in its System → SSH keys?)",
                login.user_host
            );
        }
        let neo = match copy_neo(&login) {
            Ok(bin) => through_wrapper(&bin),
            Err(e) => {
                println!(
                    "! could not copy this neo to the edge ({e:#}); using the one installed there"
                );
                "n() { neo \"$@\"; }".into()
            }
        };
        let edge = Self { login, neo };
        edge.run_neo("settings --help >/dev/null")
            .context("the edge's neo has no `settings` command; update the edge first")?;
        Ok(edge)
    }

    fn neo_cmd(&self, args: &str) -> String {
        format!("export {REMOTE_PATH}; {}; n {args}", self.neo)
    }

    fn run_neo(&self, args: &str) -> Result<String> {
        self.login.output(&self.neo_cmd(args))
    }

    /// The name SSH reaches the edge by.
    pub fn address(&self) -> &str {
        self.login.host()
    }

    /// Public addresses on the edge's interfaces: where the new server's
    /// tunnel connects and where its domain must point. The SSH name may be a
    /// private one (an ssh_config alias, NetBird, Tailscale).
    pub fn public_addresses(&self) -> Result<Vec<std::net::IpAddr>> {
        let out = self
            .login
            .output("ip -o addr show scope global | awk '{print \"IP\", $4}'")?;
        Ok(super::probe::parse_probe(&out).public_addresses())
    }

    /// Tunnels on the edge: name → domain.
    pub fn entries(&self) -> Result<Vec<(String, String)>> {
        let out = self.run_neo("settings get services.streamproxy.entries")?;
        let v: serde_json::Value =
            serde_json::from_str(out.trim()).context("parse the edge's streamproxy entries")?;
        Ok(v.as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, e)| {
                        (
                            k.clone(),
                            e.get("url")
                                .and_then(|u| u.as_str())
                                .unwrap_or("")
                                .to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn existing(&self, name: &str, domain: &str) -> Result<Existing> {
        Ok(classify(&self.entries()?, name, domain))
    }

    /// Add or update the tunnel `name` for `domain` and switch streamproxy on.
    pub fn push(&self, name: &str, domain: &str, token: &str) -> Result<()> {
        let ops = serde_json::json!([
            {"set": {"path": ["services", "streamproxy", "enabled"], "value": true}},
            {"set": {"path": ["services", "streamproxy", "entries", name], "value": {
                "url": domain,
                "token": token,
                "wildcard": true,
                "includeTopLevel": true,
            }}},
        ]);
        let mut child = self
            .login
            .ssh(&self.neo_cmd("settings patch"))
            .stdin(Stdio::piped())
            .spawn()
            .context("ssh to the edge")?;
        child
            .stdin
            .take()
            .context("ssh stdin")?
            .write_all(ops.to_string().as_bytes())?;
        if !child.wait()?.success() {
            bail!("writing the tunnel into the edge's settings failed (see above)");
        }
        Ok(())
    }

    /// `neo activate` on the edge, with its own neo (output streamed).
    pub fn activate(&self) -> Result<()> {
        println!(
            "Activating {} (this takes a few minutes) …",
            self.login.user_host
        );
        execute_command(&mut self.login.ssh(&format!("{REMOTE_PATH} neo activate")))
            .context("activation on the edge failed; fix it there and run bootstrap again")
    }
}

fn classify(entries: &[(String, String)], name: &str, domain: &str) -> Existing {
    match entries.iter().find(|(n, _)| n == name) {
        None => Existing::None,
        Some((_, url)) if url == domain => Existing::Same,
        Some((_, url)) => Existing::Other(url.clone()),
    }
}

/// Copy this neo's binary (the unwrapped one the wrapper runs) into the
/// edge's store; its path there.
fn copy_neo(login: &Login) -> Result<String> {
    let bin = env::current_exe().context("find this neo")?;
    let bin = bin.display().to_string();
    let Some(store_path) = bin
        .strip_prefix("/nix/store/")
        .and_then(|r| r.split('/').next())
        .map(|name| format!("/nix/store/{name}"))
    else {
        bail!("{bin} is not in the Nix store");
    };
    let runs = || login.ok(&format!("{} --version", shell_quote(&bin)));
    if runs() {
        return Ok(bin);
    }
    println!("Copying this neo to {} …", login.user_host);
    let status = nix_command("nix")
        .args([
            "copy",
            "--no-check-sigs",
            "--substitute-on-destination",
            "--to",
        ])
        .arg(format!("ssh-ng://{}", login.user_host))
        .arg(&store_path)
        .env("NIX_SSHOPTS", login.opts().join(" "))
        .status()
        .context("nix copy")?;
    if !status.success() {
        bail!("nix copy failed");
    }
    if !runs() {
        bail!("the copied neo does not run there");
    }
    Ok(bin)
}

/// Shell function `n` that runs `bin` with the environment of the edge's own
/// neo wrapper (its exec line pointed at `bin`), or bare when there is none.
fn through_wrapper(bin: &str) -> String {
    let sed = format!("s|/nix/store/[a-z0-9]*-neo-unwrapped-[^/]*/bin/neo|{bin}|");
    format!(
        "n() {{ w=$(readlink -f \"$(command -v neo)\"); if [ -n \"$w\" ] && grep -q neo-unwrapped \"$w\"; then bash -c \"$(sed {} \"$w\")\" neo \"$@\"; else {} \"$@\"; fi; }}",
        shell_quote(&sed),
        shell_quote(bin)
    )
}

/// A random tunnel password (48 hex characters).
pub fn random_token() -> Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .context("read /dev/urandom")?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_entries() {
        let e = vec![
            ("home".to_string(), "home.example.com".to_string()),
            ("other".to_string(), "x.example.com".to_string()),
        ];
        assert_eq!(classify(&e, "new", "new.example.com"), Existing::None);
        assert_eq!(classify(&e, "home", "home.example.com"), Existing::Same);
        assert_eq!(
            classify(&e, "other", "new.example.com"),
            Existing::Other("x.example.com".into())
        );
    }

    #[test]
    fn tokens_are_random_hex() {
        let a = random_token().unwrap();
        assert_eq!(a.len(), 48);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, random_token().unwrap());
    }
}
