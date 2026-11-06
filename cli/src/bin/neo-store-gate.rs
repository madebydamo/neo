//! Forced command of every backup-store account (nix/services/backup-store,
//! docs/design/backup-restore.md §6): runs what the backup and `neo restore`
//! ask for over SSH and refuses everything else. Never starts a shell.
//!
//! Usage: neo-store-gate <files|encrypted|zfs> <dataset prefix or ->, with the
//! request in SSH_ORIGINAL_COMMAND and sftp-server in NEO_SFTP_SERVER.

use std::env;
use std::os::unix::process::CommandExt;
use std::process::{exit, Command};

const SNAPSHOT_PREFIX: &str = "neo-backup-";

#[derive(Debug, PartialEq)]
enum Plan {
    /// The login probe (`ssh host true`).
    Done,
    Run(Vec<String>),
}

/// Words of the request as a POSIX shell would split them, for the quoting
/// ssh clients send (rsync, printf %q, shell_quote). Anything that would make
/// a shell do more than split words is refused.
fn split(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if let Some(w) = word.take() {
                    words.push(w);
                }
            }
            '\\' => {
                let next = chars.next().ok_or("trailing backslash")?;
                if next == '\n' {
                    return Err("escaped newline".into());
                }
                word.get_or_insert_with(String::new).push(next);
            }
            '\'' => {
                let w = word.get_or_insert_with(String::new);
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => w.push(c),
                        None => return Err("unclosed quote".into()),
                    }
                }
            }
            '"' | '$' | '`' | ';' | '|' | '&' | '<' | '>' | '(' | ')' | '*' | '?' | '[' | ']'
            | '{' | '}' | '~' | '#' | '!' | '\n' => {
                return Err(format!("{c:?} is not allowed"));
            }
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    if let Some(w) = word {
        words.push(w);
    }
    Ok(words)
}

/// A path inside the account's home folder.
fn inside_home(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('-')
        && !path.split('/').any(|part| part == "..")
}

fn rsync(args: &[String]) -> Result<Plan, String> {
    const ALLOWED: &[&str] = &[
        "--sender",
        "--numeric-ids",
        "--delete",
        "--delete-during",
        "--delete-delay",
        "--delete-after",
        "--delete-before",
        "--delete-excluded",
        "--mkpath",
        "--force",
        "--ignore-errors",
        "--partial",
        "--stats",
    ];
    const ALLOWED_PREFIX: &[&str] = &["--log-format=", "--info=", "--debug=", "--compress-"];
    if args.get(1).map(String::as_str) != Some("--server") {
        return Err("only rsync --server".into());
    }
    let mut run: Vec<String> = ["rsync", "--server", "--fake-super"]
        .map(String::from)
        .to_vec();
    for a in &args[2..] {
        let ok = if a.starts_with("--") {
            ALLOWED.contains(&a.as_str()) || ALLOWED_PREFIX.iter().any(|p| a.starts_with(p))
        } else if a.starts_with('-') {
            a[1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.')
        } else {
            a == "." || inside_home(a)
        };
        if !ok {
            return Err(format!("rsync argument {a}"));
        }
        run.push(a.clone());
    }
    Ok(Plan::Run(run))
}

/// `name` is the account's dataset or one below it, without a snapshot.
fn dataset_ok(name: &str, prefix: &str) -> bool {
    let plain = !name.contains(['@', '%', ' ', '\t', '\n']);
    plain && (name == prefix || name.starts_with(&format!("{prefix}/")))
}

fn snapshot_ok(name: &str, prefix: &str) -> bool {
    name.split_once('@')
        .is_some_and(|(dataset, snap)| dataset_ok(dataset, prefix) && !snap.contains(['@', ' ']))
}

fn zfs(args: &[&str], prefix: &str) -> bool {
    let child = |d: &str| dataset_ok(d, prefix) && d != prefix;
    match args {
        // The backup checks that the pool exists before the first send.
        ["list", "-H", "-o", "name", d] => {
            dataset_ok(d, prefix) || prefix.starts_with(&format!("{d}/"))
        }
        ["list", "-H", "-t", "snapshot", "-o", "name", "-s" | "-S", "createtxg", "-d", "1", d] => {
            dataset_ok(d, prefix)
        }
        ["recv", "-u", "-F", "-o", "canmount=noauto", "-o", "com.sun:auto-snapshot=false", d] => {
            child(d)
        }
        ["create", "-p", "-o", "canmount=off", d] => dataset_ok(d, prefix),
        ["destroy", s] => {
            snapshot_ok(s, prefix)
                && s.split_once('@')
                    .is_some_and(|(_, snap)| snap.starts_with(SNAPSHOT_PREFIX))
        }
        ["send", s] | ["get", "-H", "-o", "value", "referenced", s] => snapshot_ok(s, prefix),
        _ => false,
    }
}

fn plan(request: &str, format: &str, prefix: &str, sftp_server: &str) -> Result<Plan, String> {
    // sshd passes a subsystem as its program path.
    if request == sftp_server {
        return if format == "zfs" {
            Err("this account holds ZFS snapshots, there are no files to read".into())
        } else {
            Ok(Plan::Run(vec![
                sftp_server.into(),
                "-u".into(),
                "077".into(),
            ]))
        };
    }
    let args = split(request)?;
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match (format, words.as_slice()) {
        (_, ["true"]) => Ok(Plan::Done),
        ("files", ["rsync", ..]) => rsync(&args),
        ("files", ["chmod", "u+w", paths @ ..]) if paths.iter().all(|p| inside_home(p)) => {
            Ok(Plan::Run(args))
        }
        ("files", ["test", "-d", p]) if inside_home(p) => Ok(Plan::Run(args)),
        ("encrypted", ["rm", "-rf", p])
            if inside_home(p)
                && p.rsplit('/')
                    .next()
                    .is_some_and(|last| last.starts_with("restic-")) =>
        {
            Ok(Plan::Run(args))
        }
        ("zfs", ["zfs", rest @ ..]) if zfs(rest, prefix) => Ok(Plan::Run(args)),
        _ => Err(format!("not a {format} backup command")),
    }
}

fn main() {
    let argv: Vec<String> = env::args().collect();
    let (Some(format), Some(prefix)) = (argv.get(1), argv.get(2)) else {
        eprintln!("usage: neo-store-gate <format> <dataset prefix>");
        exit(2);
    };
    let request = env::var("SSH_ORIGINAL_COMMAND").unwrap_or_default();
    let sftp_server = env::var("NEO_SFTP_SERVER").unwrap_or_default();
    match plan(&request, format, prefix, &sftp_server) {
        Ok(Plan::Done) => {}
        Ok(Plan::Run(words)) => {
            let err = Command::new(&words[0]).args(&words[1..]).exec();
            eprintln!("backup store: cannot run {}: {err}", words[0]);
            exit(126);
        }
        Err(why) => {
            eprintln!("backup store: refused ({why})");
            exit(126);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SFTP: &str = "/nix/store/x/libexec/sftp-server";
    const P: &str = "zroot/backup-store/home";

    fn run(request: &str, format: &str) -> Result<Plan, String> {
        plan(request, format, P, SFTP)
    }

    fn allowed(request: &str, format: &str) -> bool {
        run(request, format).is_ok()
    }

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(split(r"a\ b 'c d' e").unwrap(), ["a b", "c d", "e"]);
        for bad in [
            "a; b", "a | b", "$(x)", "`x`", "a > b", "a\nb", "'open", "\"x\"",
        ] {
            assert!(split(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn probe_for_every_format() {
        for f in ["files", "encrypted", "zfs"] {
            assert_eq!(run("true", f), Ok(Plan::Done));
        }
    }

    #[test]
    fn rsync_gets_fake_super() {
        let Ok(Plan::Run(w)) = run(
            "rsync --server -logDtprzeLsfxCIvu --numeric-ids --delete-during . dev/neo/",
            "files",
        ) else {
            panic!();
        };
        assert_eq!(&w[..3], ["rsync", "--server", "--fake-super"]);
        assert_eq!(w.last().map(String::as_str), Some("dev/neo/"));
        assert!(allowed(
            "rsync --server --sender -logDtpr . dev/neo/",
            "files"
        ));
    }

    #[test]
    fn rsync_stays_home() {
        for bad in [
            "rsync --server -a . /etc",
            "rsync --server -a . ../other/",
            "rsync --server --log-file=/tmp/x -a . d",
            "rsync --server --rsync-path=x -a . d",
            "rsync --daemon",
            "rsync -e x host:y",
        ] {
            assert!(!allowed(bad, "files"), "{bad}");
        }
    }

    #[test]
    fn format_limits_commands() {
        assert!(!allowed("rsync --server -a . d", "encrypted"));
        assert!(!allowed("rsync --server -a . d", "zfs"));
        assert!(!allowed(&format!("zfs send {P}/neo@s"), "files"));
        assert!(!allowed("rm -rf restic-1", "files"));
        assert!(allowed(SFTP, "files"));
        assert!(allowed(SFTP, "encrypted"));
        assert!(!allowed(SFTP, "zfs"));
        assert!(!allowed("sh", "files"));
        assert!(!allowed("cat /etc/shadow", "encrypted"));
    }

    #[test]
    fn encrypted_removes_only_repositories() {
        assert!(allowed("rm -rf dev/restic-20261103-120000", "encrypted"));
        assert!(!allowed("rm -rf dev", "encrypted"));
        assert!(!allowed("rm -rf ../other/restic-1", "encrypted"));
        assert!(!allowed("rm -rf /restic-1", "encrypted"));
    }

    #[test]
    fn zfs_stays_in_its_datasets() {
        let d = format!("{P}/neo");
        for ok in [
            format!("zfs list -H -o name zroot"),
            format!("zfs list -H -t snapshot -o name -S createtxg -d 1 {d}"),
            format!("zfs recv -u -F -o canmount=noauto -o com.sun:auto-snapshot=false {d}"),
            format!("zfs create -p -o canmount=off {P}"),
            format!("zfs destroy {d}@neo-backup-20261103"),
            format!("zfs send {d}@neo-backup-20261103"),
            format!("zfs get -H -o value referenced {d}@neo-backup-20261103"),
        ] {
            assert!(allowed(&ok, "zfs"), "{ok}");
        }
        for bad in [
            format!("zfs recv -u -F -o canmount=noauto -o com.sun:auto-snapshot=false {P}"),
            "zfs recv -u -F -o canmount=noauto -o com.sun:auto-snapshot=false zroot/neo".into(),
            format!("zfs recv -u -F -o mountpoint=/etc {d}"),
            format!("zfs destroy {d}"),
            format!("zfs destroy {d}@precious"),
            "zfs send zroot/neo@s".to_string(),
            format!("zfs send {P}x/neo@s"),
            "zfs destroy -r zroot".to_string(),
            format!("zfs set mountpoint=/etc {d}"),
        ] {
            assert!(!allowed(&bad, "zfs"), "{bad}");
        }
    }
}
