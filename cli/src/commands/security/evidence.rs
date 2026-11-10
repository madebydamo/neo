//! What is actually running: /proc/<pid>/maps of every process, grouped by
//! systemd unit or container, and the file lists of OS packages inside the
//! containers. A vulnerable library that nothing has mapped is much less
//! urgent than one a web-facing service has loaded.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;

/// One process: who runs it and the files it has mapped (exe included).
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub comm: String,
    /// systemd unit without `.service`, e.g. `hermes-agent`.
    pub unit: Option<String>,
    /// Full container id when the process runs in a Docker container.
    pub container_id: Option<String>,
    pub paths: HashSet<String>,
}

impl Proc {
    pub fn label(&self) -> String {
        match &self.unit {
            Some(u) => format!("{u} ({})", self.comm),
            None => self.comm.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ProcIndex {
    pub procs: Vec<Proc>,
}

impl ProcIndex {
    /// Read every process. Unreadable ones (gone, kernel threads) are skipped.
    pub fn scan() -> Self {
        let mut procs = Vec::new();
        let Ok(dir) = fs::read_dir("/proc") else {
            return Self::default();
        };
        for e in dir.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(maps) = fs::read_to_string(format!("/proc/{pid}/maps")) else {
                continue;
            };
            let mut paths: HashSet<String> = maps
                .lines()
                .filter_map(map_path)
                .map(|p| p.trim_end_matches(" (deleted)").to_string())
                .collect();
            if paths.is_empty() {
                continue;
            }
            if let Ok(exe) = fs::read_link(format!("/proc/{pid}/exe")) {
                paths.insert(exe.to_string_lossy().into_owned());
            }
            let comm = fs::read_to_string(format!("/proc/{pid}/comm"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let cgroup = fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default();
            let (unit, container_id) = parse_cgroup(&cgroup);
            procs.push(Proc {
                pid,
                comm,
                unit,
                container_id,
                paths,
            });
        }
        Self { procs }
    }

    /// Host processes with a store path of `<pname>-<version>` (any output) mapped.
    pub fn nix_users(&self, pname: &str, version: &str) -> Vec<&Proc> {
        let want = format!("{pname}-{version}");
        self.procs
            .iter()
            .filter(|p| p.container_id.is_none())
            .filter(|p| p.paths.iter().any(|path| store_name_matches(path, &want)))
            .collect()
    }

    pub fn container_procs<'a>(&'a self, container_id: &'a str) -> impl Iterator<Item = &'a Proc> {
        self.procs
            .iter()
            .filter(move |p| p.container_id.as_deref() == Some(container_id))
    }
}

/// The path column of a maps line (6th field), when it is a file.
fn map_path(line: &str) -> Option<&str> {
    let mut rest = line;
    for _ in 0..5 {
        let i = rest.find(char::is_whitespace)?;
        rest = rest[i..].trim_start();
    }
    rest.starts_with('/').then_some(rest)
}

/// `0::/system.slice/hermes-agent.service` → unit `hermes-agent`;
/// `…/docker-<64 hex>.scope` → that container id.
pub fn parse_cgroup(text: &str) -> (Option<String>, Option<String>) {
    let mut unit = None;
    let mut container = None;
    for line in text.lines() {
        let path = line.splitn(3, ':').nth(2).unwrap_or("");
        for seg in path.split('/') {
            if let Some(id) = seg
                .strip_prefix("docker-")
                .and_then(|s| s.strip_suffix(".scope"))
            {
                container = Some(id.to_string());
            } else if let Some(u) = seg.strip_suffix(".service") {
                unit = Some(u.to_string());
            }
        }
    }
    (unit, container)
}

/// `/nix/store/<hash>-zlib-1.3.2/lib/libz.so.1` matches `zlib-1.3.2`, as does
/// another output (`…-openssl-3.0.13-bin`); `zlib-1.3.21` does not.
fn store_name_matches(path: &str, want: &str) -> bool {
    let Some(rest) = path.strip_prefix("/nix/store/") else {
        return false;
    };
    let name = rest.split('/').next().unwrap_or("");
    let Some((_, name)) = name.split_once('-') else {
        return false;
    };
    name == want
        || name.strip_prefix(want).is_some_and(|s| {
            s.starts_with('-') && !s[1..].starts_with(|c: char| c.is_ascii_digit())
        })
}

/// Files an OS package installed in a container, read through a process's
/// root (`/proc/<pid>/root`). Debian/Ubuntu dpkg lists and Alpine's apk db;
/// None for other distributions.
pub fn os_package_files(pid: u32, package: &str) -> Option<HashSet<String>> {
    let root = format!("/proc/{pid}/root");
    let dpkg = format!("{root}/var/lib/dpkg/info");
    if let Ok(dir) = fs::read_dir(&dpkg) {
        let wanted = [format!("{package}.list")];
        let prefix = format!("{package}:");
        for e in dir.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if wanted.contains(&n) || (n.starts_with(&prefix) && n.ends_with(".list")) {
                let text = fs::read_to_string(e.path()).ok()?;
                return Some(text.lines().map(|l| usr_merged(l.trim())).collect());
            }
        }
        return None;
    }
    if let Ok(db) = fs::read_to_string(format!("{root}/lib/apk/db/installed")) {
        return Some(apk_files(&db, package));
    }
    None
}

fn apk_files(db: &str, package: &str) -> HashSet<String> {
    let mut files = HashSet::new();
    for block in db.split("\n\n") {
        if !block.lines().any(|l| l == format!("P:{package}")) {
            continue;
        }
        let mut dir = String::new();
        for l in block.lines() {
            if let Some(d) = l.strip_prefix("F:") {
                dir = d.to_string();
            } else if let Some(f) = l.strip_prefix("R:") {
                files.insert(usr_merged(&format!("/{dir}/{f}")));
            }
        }
    }
    files
}

/// `/usr/lib/x.so` and `/lib/x.so` are the same file on merged-/usr systems,
/// and package lists and maps do not agree on which one they name.
fn usr_merged(p: &str) -> String {
    p.strip_prefix("/usr").unwrap_or(p).to_string()
}

/// Result of the loaded-library check for one image group.
pub struct ImageLoad {
    pub loaded_by: BTreeSet<String>,
    pub checked: bool,
}

/// Which running containers of these have one of the package's files mapped.
/// Paths in maps are as the container sees them, like the package lists.
pub fn image_load(
    procs: &ProcIndex,
    containers: &[(String, String)],
    package: &str,
    os_pkg: bool,
    pkg_paths: &[String],
    file_cache: &mut HashMap<(String, String), Option<HashSet<String>>>,
) -> ImageLoad {
    let mut out = ImageLoad {
        loaded_by: BTreeSet::new(),
        checked: false,
    };
    for (name, id) in containers {
        let ps: Vec<&Proc> = procs.container_procs(id).collect();
        let Some(first) = ps.first() else {
            continue;
        };
        if os_pkg {
            let files = file_cache
                .entry((id.clone(), package.to_string()))
                .or_insert_with(|| os_package_files(first.pid, package));
            let Some(files) = files else {
                continue;
            };
            out.checked = true;
            for p in &ps {
                if p.paths.iter().any(|m| files.contains(&usr_merged(m))) {
                    out.loaded_by.insert(format!("{name} ({})", p.comm));
                }
            }
        } else {
            // Language packages: only native modules show up in maps (a
            // .so under the package's directory). Absence proves little.
            let dirs: Vec<String> = pkg_paths
                .iter()
                .filter_map(|p| {
                    let p = format!("/{}", p.trim_start_matches('/'));
                    p.rsplit_once('/').map(|(d, _)| d.to_string())
                })
                .filter(|d| d.len() > 1)
                .collect();
            for p in &ps {
                if p.paths
                    .iter()
                    .any(|m| dirs.iter().any(|d| m.starts_with(d.as_str())))
                {
                    out.loaded_by.insert(format!("{name} ({})", p.comm));
                }
            }
        }
    }
    out
}

/// unit → services that list it in `systemdUnits`.
pub fn unit_owners(
    services: &BTreeMap<String, super::config::ServiceInfo>,
) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for (name, s) in services {
        for u in &s.units {
            m.insert(u.clone(), name.clone());
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_path_column() {
        assert_eq!(
            map_path("7f0000000000-7f0000001000 r-xp 00000000 fd:01 123  /nix/store/abc-zlib-1.3.2/lib/libz.so.1"),
            Some("/nix/store/abc-zlib-1.3.2/lib/libz.so.1")
        );
        assert_eq!(map_path("7f-80 rw-p 00000000 00:00 0 [heap]"), None);
        assert_eq!(map_path("7f-80 rw-p 00000000 00:00 0"), None);
    }

    #[test]
    fn cgroup_units_and_containers() {
        assert_eq!(
            parse_cgroup("0::/system.slice/hermes-agent.service\n"),
            (Some("hermes-agent".into()), None)
        );
        let (_, c) = parse_cgroup("0::/system.slice/docker-0123abcd.scope\n");
        assert_eq!(c.as_deref(), Some("0123abcd"));
    }

    #[test]
    fn store_names() {
        assert!(store_name_matches(
            "/nix/store/h-zlib-1.3.2/lib/libz.so",
            "zlib-1.3.2"
        ));
        assert!(store_name_matches(
            "/nix/store/h-openssl-3.0.13-bin/bin/openssl",
            "openssl-3.0.13"
        ));
        assert!(!store_name_matches(
            "/nix/store/h-zlib-1.3.21/lib/libz.so",
            "zlib-1.3.2"
        ));
        assert!(!store_name_matches(
            "/nix/store/h-zlib-1.3.2.1/lib",
            "zlib-1.3.2"
        ));
        assert!(!store_name_matches("/usr/lib/libz.so", "zlib-1.3.2"));
    }

    #[test]
    fn apk_db_files() {
        let db = "P:musl\nV:1\nF:lib\nR:ld-musl-x86_64.so.1\n\nP:zlib\nF:lib\nR:libz.so.1\nR:libz.so.1.3\n";
        let f = apk_files(db, "zlib");
        assert!(f.contains("/lib/libz.so.1"));
        assert_eq!(usr_merged("/usr/lib/libz.so.1"), "/lib/libz.so.1");
        assert_eq!(f.len(), 2);
    }
}
