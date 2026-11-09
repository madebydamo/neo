//! Cross-process operation locks.
//!
//! Every mutating Neo operation (CLI subcommand, web job, scheduled updater)
//! takes a set of **scopes**, each in **shared** or **exclusive** mode
//! (readers–writer):
//!
//! | Scope | Meaning |
//! |-------|---------|
//! | `system` | the running system and the config repo (activate, update, generation switch, store repair, data restore) |
//! | `service/<name>` | a service's app data (restore, clear appdata, snapshot) |
//! | `unit/<unit>` | one systemd unit (start/stop/restart, image pull, updater restart) |
//!
//! System-changing ops take `system` exclusively; service ops take `system`
//! shared plus their own scopes exclusively, so they run in parallel with each
//! other but never under an activation.
//!
//! Each scope is an `flock(2)` on `<run>/locks/<scope>.lock` (`/run/neo` on a host,
//! see [`LockManager::system`]). The kernel drops a lock when its holder exits, so
//! a crashed operation never leaves a lock behind. Next to the lock files every
//! acquisition writes a **holder file** (`<pid>-<seq>.holder`: op kind, label,
//! pid, start time, scopes) that the holder keeps `flock`ed exclusively; a holder
//! file whose lock is free belongs to a dead process and is removed by the next
//! reader. The holder files only feed messages ("Blocked: Activation in progress
//! (started 12:03)") and the web UI; the scope `flock`s alone decide.
//!
//! **Unit start guards**: [`LockGuard::guard_units`] creates `<run>/guard/<unit>`.
//! A system-wide drop-in (`service.d/` + `target.d/`, `AssertPathExists=!/run/neo/guard/%n`)
//! makes systemd refuse to start a guarded unit — whoever asks (timers,
//! dependencies, a manual `systemctl start`). Guards are removed when the lock
//! is released, and by the stale-holder cleanup after a crash.
//!
//! **Inheritance**: a CLI process that holds scopes exclusively exports them in
//! `NEO_LOCKS_HELD` ([`LockGuard::export_to_children`]) so a nested `neo` call
//! (e.g. `neo update` → `nix run .#neo -- migrate`) does not deadlock on its parent.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Runtime root on a Neo host (created by tmpfiles, owned by homeserver).
pub const RUN_DIR: &str = "/run/neo";
/// Scope keys held exclusively by an ancestor process (space separated).
pub const INHERIT_ENV: &str = "NEO_LOCKS_HELD";
/// Override for the runtime root (tests, development).
pub const RUN_DIR_ENV: &str = "NEO_RUN_DIR";
/// Seconds a CLI command waits for its locks (default 0: fail fast).
pub const WAIT_ENV: &str = "NEO_LOCK_WAIT";

const HOLDER_EXT: &str = ".holder";
const POLL: Duration = Duration::from_millis(250);

static SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LockMode {
    #[serde(rename = "sh")]
    Shared,
    #[serde(rename = "ex")]
    Exclusive,
}

impl LockMode {
    pub fn conflicts(self, other: LockMode) -> bool {
        self == LockMode::Exclusive || other == LockMode::Exclusive
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LockMode::Shared => "sh",
            LockMode::Exclusive => "ex",
        }
    }

    fn flock_op(self) -> libc::c_int {
        match self {
            LockMode::Shared => libc::LOCK_SH,
            LockMode::Exclusive => libc::LOCK_EX,
        }
    }
}

/// What an operation locks. Ordered (system < service < unit) so every process
/// takes scopes in the same order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    System,
    Service(String),
    Unit(String),
}

/// Unit type suffixes; a bare name (`docker-foo`) means `.service`.
const UNIT_SUFFIXES: &[&str] = &[
    "service", "target", "timer", "socket", "mount", "path", "slice", "scope",
];

/// Full unit name as systemd's `%n` expands it (`docker-foo` → `docker-foo.service`).
pub fn full_unit_name(unit: &str) -> String {
    match unit.rsplit_once('.') {
        Some((_, suffix)) if UNIT_SUFFIXES.contains(&suffix) => unit.to_string(),
        _ => format!("{unit}.service"),
    }
}

impl Scope {
    pub fn service(name: &str) -> Scope {
        Scope::Service(name.to_string())
    }

    /// Unit scope; the name is normalized to its full unit name.
    pub fn unit(name: &str) -> Scope {
        Scope::Unit(full_unit_name(name))
    }

    /// Stable key used in holder files, `NEO_LOCKS_HELD` and the web UI
    /// (`system`, `service/<name>`, `unit/<unit>`).
    pub fn key(&self) -> String {
        match self {
            Scope::System => "system".to_string(),
            Scope::Service(n) => format!("service/{n}"),
            Scope::Unit(u) => format!("unit/{u}"),
        }
    }

    pub fn parse(key: &str) -> Option<Scope> {
        if key == "system" {
            return Some(Scope::System);
        }
        let (kind, name) = key.split_once('/')?;
        if name.is_empty() {
            return None;
        }
        match kind {
            "service" => Some(Scope::Service(name.to_string())),
            "unit" => Some(Scope::Unit(name.to_string())),
            _ => None,
        }
    }

    /// Lock file name (no path separators whatever the name contains).
    /// Shell users (docker-updater) build `unit_<unit>.lock` the same way.
    pub fn file_name(&self) -> String {
        let (kind, name) = match self {
            Scope::System => return "system.lock".to_string(),
            Scope::Service(n) => ("service", n),
            Scope::Unit(u) => ("unit", u),
        };
        let mut s = format!("{kind}_");
        for b in name.bytes() {
            if b.is_ascii_alphanumeric() || b"._@-".contains(&b) {
                s.push(b as char);
            } else {
                s.push_str(&format!("%{b:02X}"));
            }
        }
        s.push_str(".lock");
        s
    }

    /// Human description for messages.
    pub fn describe(&self) -> String {
        match self {
            Scope::System => "the system".to_string(),
            Scope::Service(n) => format!("service {n}"),
            Scope::Unit(u) => format!("unit {u}"),
        }
    }
}

/// Scopes an operation needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockSpec {
    scopes: Vec<(Scope, LockMode)>,
}

impl LockSpec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a scope; a scope added twice keeps the stronger mode.
    pub fn with(mut self, scope: Scope, mode: LockMode) -> Self {
        match self.scopes.iter_mut().find(|(s, _)| *s == scope) {
            Some((_, m)) => *m = (*m).max(mode),
            None => self.scopes.push((scope, mode)),
        }
        self.scopes.sort();
        self
    }

    /// Activation, update, generation switch, store repair, data restore:
    /// nothing else may run.
    pub fn system_change() -> Self {
        Self::new().with(Scope::System, LockMode::Exclusive)
    }

    /// Brief config-repo write (settings save / discard): not under an
    /// activation or update, otherwise parallel.
    pub fn system_shared() -> Self {
        Self::new().with(Scope::System, LockMode::Shared)
    }

    /// Operation on a service's data with its units stopped (restore, clear
    /// appdata): system shared, the service and every unit exclusive.
    pub fn service_data<'a>(service: &str, units: impl IntoIterator<Item = &'a str>) -> Self {
        let mut spec = Self::system_shared().with(Scope::service(service), LockMode::Exclusive);
        for u in units {
            spec = spec.with(Scope::unit(u), LockMode::Exclusive);
        }
        spec
    }

    /// Start/stop/restart or image pull of one unit.
    pub fn unit(unit: &str) -> Self {
        Self::new().with(Scope::unit(unit), LockMode::Exclusive)
    }

    pub fn scopes(&self) -> &[(Scope, LockMode)] {
        &self.scopes
    }

    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }
}

/// Who is asking (written to the holder file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpInfo {
    /// Machine kind (`activation`, `restore`, …).
    pub kind: String,
    /// Human label (`Activation`, `Snapshot restore of calino`).
    pub label: String,
    /// Operation id of the monitored op, when there is one.
    pub op_id: Option<String>,
}

impl OpInfo {
    pub fn new(kind: &str, label: impl Into<String>) -> Self {
        OpInfo {
            kind: kind.to_string(),
            label: label.into(),
            op_id: None,
        }
    }

    pub fn with_op_id(mut self, id: impl Into<String>) -> Self {
        self.op_id = Some(id.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldScope {
    pub scope: String,
    pub mode: LockMode,
}

/// Contents of a holder file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
    #[serde(default)]
    pub id: String,
    pub scopes: Vec<HeldScope>,
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_id: Option<String>,
    pub pid: u32,
    /// Unix seconds.
    pub started_at: i64,
    /// Units with a start guard (see [`LockGuard::guard_units`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guarded_units: Vec<String>,
}

impl Holder {
    /// Mode this holder holds `scope` in.
    pub fn mode_for(&self, scope: &Scope) -> Option<LockMode> {
        let key = scope.key();
        self.scopes.iter().find(|h| h.scope == key).map(|h| h.mode)
    }

    /// Whether this holder blocks `scope` in `mode`.
    pub fn blocks(&self, scope: &Scope, mode: LockMode) -> bool {
        self.mode_for(scope).is_some_and(|m| m.conflicts(mode))
    }

    /// `Activation in progress (started 12:03)`.
    pub fn message(&self) -> String {
        format!(
            "{} in progress (started {})",
            self.label,
            local_hhmm(self.started_at)
        )
    }
}

/// A scope could not be taken because someone else holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockConflict {
    pub scope: Scope,
    pub wanted: LockMode,
    /// Live holders that block `scope` (empty when the holder left no info,
    /// e.g. `flock(1)` in a shell script).
    pub holders: Vec<Holder>,
}

impl fmt::Display for LockConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.holders.first() {
            Some(h) => write!(f, "Blocked: {}", h.message()),
            None => write!(
                f,
                "Blocked: {} is in use by another operation",
                self.scope.describe()
            ),
        }
    }
}

#[derive(Debug)]
pub enum LockError {
    Conflict(LockConflict),
    Io(String),
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::Conflict(c) => c.fmt(f),
            LockError::Io(e) => write!(f, "operation lock unavailable: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

/// Held locks. Dropping it removes unit start guards, the holder file, and
/// releases every scope (also when the task that owns it panics or is aborted).
#[derive(Debug)]
pub struct LockGuard {
    locks: Vec<File>,
    holder: Option<(PathBuf, File, Holder)>,
    guard_dir: PathBuf,
    /// Exclusively held scope keys (for [`Self::export_to_children`]).
    exclusive: Vec<String>,
}

impl LockGuard {
    fn empty(guard_dir: PathBuf) -> Self {
        LockGuard {
            locks: Vec::new(),
            holder: None,
            guard_dir,
            exclusive: Vec::new(),
        }
    }

    /// True when nothing was locked (everything inherited from a parent).
    pub fn is_empty(&self) -> bool {
        self.locks.is_empty()
    }

    /// Make systemd refuse to start `units` until [`Self::release_unit_guards`]
    /// or drop. Marker files contain this holder's id so a stale-holder cleanup
    /// never removes a newer holder's guard.
    pub fn guard_units(&mut self, units: &[String]) -> Result<(), String> {
        let names: Vec<String> = units.iter().map(|u| full_unit_name(u)).collect();
        if let Some(bad) = names.iter().find(|n| !marker_name_ok(n)) {
            return Err(format!("invalid unit name for start guard: {bad}"));
        }
        let Some((_, file, holder)) = self.holder.as_mut() else {
            return Err("no holder file (lock directory not writable)".to_string());
        };
        for n in &names {
            if !holder.guarded_units.contains(n) {
                holder.guarded_units.push(n.clone());
            }
        }
        // Record first: a crash after this leaves markers the cleanup knows about.
        write_holder_in_place(file, holder).map_err(|e| format!("holder file: {e}"))?;
        fs::create_dir_all(&self.guard_dir)
            .map_err(|e| format!("{}: {e}", self.guard_dir.display()))?;
        for n in &names {
            let p = self.guard_dir.join(n);
            fs::write(&p, &holder.id).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        Ok(())
    }

    /// Remove all start guards (units may be started again). The scopes stay held.
    pub fn release_unit_guards(&mut self) {
        let Some((_, file, holder)) = self.holder.as_mut() else {
            return;
        };
        if holder.guarded_units.is_empty() {
            return;
        }
        for n in &holder.guarded_units {
            remove_marker_if_owned(&self.guard_dir, n, &holder.id);
        }
        holder.guarded_units.clear();
        let _ = write_holder_in_place(file, holder);
    }

    /// Units currently guarded by this lock.
    pub fn guarded_units(&self) -> Vec<String> {
        self.holder
            .as_ref()
            .map(|(_, _, h)| h.guarded_units.clone())
            .unwrap_or_default()
    }

    /// Let child processes (nested `neo` calls) reuse the exclusively held scopes.
    /// Only for single-threaded CLI entry points (mutates the process environment).
    pub fn export_to_children(&self) {
        if self.exclusive.is_empty() {
            return;
        }
        let mut keys: BTreeSet<String> = std::env::var(INHERIT_ENV)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        keys.extend(self.exclusive.iter().cloned());
        let v = keys.into_iter().collect::<Vec<_>>().join(" ");
        std::env::set_var(INHERIT_ENV, v);
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        self.release_unit_guards();
        if let Some((path, _file, _)) = self.holder.take() {
            // Unlink before closing: a reader that still sees the file finds it
            // locked (alive) or, after close, stale — both harmless.
            let _ = fs::remove_file(&path);
        }
        // Closing the lock files releases the flocks.
        self.locks.clear();
    }
}

/// Lock directory + unit guard directory.
#[derive(Clone, Debug)]
pub struct LockManager {
    dir: PathBuf,
    guard_dir: PathBuf,
    inherited: BTreeSet<Scope>,
}

impl LockManager {
    /// Manager rooted at `root` (`<root>/locks`, `<root>/guard`), no inherited scopes.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        LockManager {
            dir: root.join("locks"),
            guard_dir: root.join("guard"),
            inherited: BTreeSet::new(),
        }
    }

    /// Process-wide manager: `$NEO_RUN_DIR`, else `/run/neo` when it exists or can
    /// be created, else a per-user temp dir (laptop / dev, where no other Neo
    /// process runs as another user). Inherits `NEO_LOCKS_HELD`.
    pub fn system() -> Self {
        let root = match std::env::var(RUN_DIR_ENV) {
            Ok(v) if !v.is_empty() => PathBuf::from(v),
            _ => {
                let run = PathBuf::from(RUN_DIR);
                if run.join("locks").is_dir() || fs::create_dir_all(run.join("locks")).is_ok() {
                    run
                } else {
                    // SAFETY: getuid has no preconditions.
                    let uid = unsafe { libc::getuid() };
                    std::env::temp_dir().join(format!("neo-run-{uid}"))
                }
            }
        };
        let mut m = Self::new(root);
        m.inherited = std::env::var(INHERIT_ENV)
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(Scope::parse)
            .collect();
        m
    }

    pub fn lock_dir(&self) -> &Path {
        &self.dir
    }

    pub fn guard_dir(&self) -> &Path {
        &self.guard_dir
    }

    /// Take every scope of `spec` without waiting. All or nothing.
    pub fn try_acquire(&self, spec: &LockSpec, info: &OpInfo) -> Result<LockGuard, LockError> {
        let wanted: Vec<&(Scope, LockMode)> = spec
            .scopes()
            .iter()
            .filter(|(s, _)| !self.inherited.contains(s))
            .collect();
        if wanted.is_empty() {
            return Ok(LockGuard::empty(self.guard_dir.clone()));
        }
        if let Err(e) = fs::create_dir_all(&self.dir) {
            if !self.dir.is_dir() {
                return Err(LockError::Io(format!("{}: {e}", self.dir.display())));
            }
        }

        let mut locks = Vec::with_capacity(wanted.len());
        for (scope, mode) in &wanted {
            let path = self.dir.join(scope.file_name());
            let file = open_lock_file(&path)
                .map_err(|e| LockError::Io(format!("{}: {e}", path.display())))?;
            match flock_nb_settle(&file, *mode) {
                Ok(()) => locks.push(file),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    drop(locks);
                    let holders = self
                        .holders()
                        .into_iter()
                        .filter(|h| h.blocks(scope, *mode))
                        .collect();
                    return Err(LockError::Conflict(LockConflict {
                        scope: scope.clone(),
                        wanted: *mode,
                        holders,
                    }));
                }
                Err(e) => return Err(LockError::Io(format!("flock {}: {e}", path.display()))),
            }
        }

        let exclusive = wanted
            .iter()
            .filter(|(_, m)| *m == LockMode::Exclusive)
            .map(|(s, _)| s.key())
            .collect();
        let holder = match self.write_holder(&wanted, info) {
            Ok(h) => Some(h),
            Err(e) => {
                // Locks still protect; only the "who" message is missing.
                eprintln!("neo: lock holder info not written: {e}");
                None
            }
        };
        Ok(LockGuard {
            locks,
            holder,
            guard_dir: self.guard_dir.clone(),
            exclusive,
        })
    }

    /// Like [`Self::try_acquire`], retrying on conflict until `wait` has passed.
    pub fn acquire(
        &self,
        spec: &LockSpec,
        info: &OpInfo,
        wait: Duration,
    ) -> Result<LockGuard, LockError> {
        let deadline = Instant::now() + wait;
        loop {
            match self.try_acquire(spec, info) {
                Err(LockError::Conflict(c)) if Instant::now() < deadline => {
                    let _ = c;
                    std::thread::sleep(POLL);
                }
                other => return other,
            }
        }
    }

    /// Live holders, oldest first. Removes holder files of dead processes and
    /// the unit start guards they left behind.
    pub fn holders(&self) -> Vec<Holder> {
        let Ok(rd) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            if name.starts_with('.') {
                // Holder file being created; clean up one abandoned by a crash.
                if name.ends_with(".tmp") && is_unlocked(&path) && older_than(&path, 60) {
                    let _ = fs::remove_file(&path);
                }
                continue;
            }
            if !name.ends_with(HOLDER_EXT) {
                continue;
            }
            let Ok(mut f) = File::open(&path) else {
                continue;
            };
            let mut raw = String::new();
            let _ = f.read_to_string(&mut raw);
            let parsed: Option<Holder> = serde_json::from_str(&raw).ok();
            match flock(&f, libc::LOCK_SH | libc::LOCK_NB) {
                Ok(()) => {
                    // Nobody holds it: the process died.
                    if let Some(h) = &parsed {
                        for u in &h.guarded_units {
                            remove_marker_if_owned(&self.guard_dir, u, &h.id);
                        }
                    }
                    let _ = fs::remove_file(&path);
                }
                Err(_) => {
                    if let Some(h) = parsed {
                        out.push(h);
                    }
                }
            }
        }
        out.sort_by(|a, b| (a.started_at, &a.id).cmp(&(b.started_at, &b.id)));
        out
    }

    fn write_holder(
        &self,
        wanted: &[&(Scope, LockMode)],
        info: &OpInfo,
    ) -> std::io::Result<(PathBuf, File, Holder)> {
        let pid = std::process::id();
        let id = format!("{pid}-{}", SEQ.fetch_add(1, Ordering::Relaxed));
        let holder = Holder {
            id: id.clone(),
            scopes: wanted
                .iter()
                .map(|(s, m)| HeldScope {
                    scope: s.key(),
                    mode: *m,
                })
                .collect(),
            kind: info.kind.clone(),
            label: info.label.clone(),
            op_id: info.op_id.clone(),
            pid,
            started_at: now_epoch(),
            guarded_units: Vec::new(),
        };
        let tmp = self.dir.join(format!(".{id}.tmp"));
        let path = self.dir.join(format!("{id}{HOLDER_EXT}"));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        // Lock before it gets its visible name: readers treat unlocked holder files as stale.
        flock(&file, libc::LOCK_EX | libc::LOCK_NB)?;
        file.write_all(&serde_json::to_vec(&holder).unwrap_or_default())?;
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        Ok((path, file, holder))
    }
}

fn open_lock_file(path: &Path) -> std::io::Result<File> {
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
    {
        Ok(f) => Ok(f),
        // Created by another user (e.g. root's flock(1)): flock works on a read-only fd.
        Err(e) if e.kind() == ErrorKind::PermissionDenied => File::open(path),
        Err(e) => Err(e),
    }
}

fn flock(file: &File, op: libc::c_int) -> std::io::Result<()> {
    loop {
        // SAFETY: valid open fd; flock has no memory-safety preconditions.
        let r = unsafe { libc::flock(file.as_raw_fd(), op) };
        if r == 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

/// Non-blocking flock that tolerates a just-released lock: a process forked
/// (for any child command) between our open and its exec briefly shares the
/// lock's open file description, so a release is not visible for a few ms.
fn flock_nb_settle(file: &File, mode: LockMode) -> std::io::Result<()> {
    let mut tries = 0;
    loop {
        match flock(file, mode.flock_op() | libc::LOCK_NB) {
            Err(e) if e.kind() == ErrorKind::WouldBlock && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            other => return other,
        }
    }
}

fn is_unlocked(path: &Path) -> bool {
    File::open(path)
        .map(|f| flock(&f, libc::LOCK_SH | libc::LOCK_NB).is_ok())
        .unwrap_or(false)
}

fn older_than(path: &Path, secs: u64) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|d| d.as_secs() > secs)
}

fn write_holder_in_place(file: &mut File, holder: &Holder) -> std::io::Result<()> {
    let data = serde_json::to_vec(holder).unwrap_or_default();
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&data)?;
    file.flush()
}

/// A unit guard marker is named exactly like the unit (`%n`).
fn marker_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\0')
}

fn remove_marker_if_owned(guard_dir: &Path, unit: &str, holder_id: &str) {
    if !marker_name_ok(unit) {
        return;
    }
    let p = guard_dir.join(unit);
    if fs::read_to_string(&p).is_ok_and(|owner| owner.trim() == holder_id) {
        let _ = fs::remove_file(&p);
    }
}

fn now_epoch() -> i64 {
    super::command::now_secs() as i64
}

/// `HH:MM` in the host's local time zone.
pub fn local_hhmm(epoch: i64) -> String {
    let t = epoch as libc::time_t;
    // SAFETY: zeroed tm is a valid out-parameter; localtime_r is thread-safe.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if ok {
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    } else {
        format!("{:02}:{:02}", (epoch / 3600) % 24, (epoch / 60) % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(name: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("neo-locks-test-{}-{name}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    fn info(label: &str) -> OpInfo {
        OpInfo::new("test", label)
    }

    fn conflict(r: Result<LockGuard, LockError>) -> LockConflict {
        match r {
            Err(LockError::Conflict(c)) => c,
            Err(e) => panic!("expected conflict, got {e}"),
            Ok(_) => panic!("expected conflict, got lock"),
        }
    }

    #[test]
    fn shared_holders_coexist_exclusive_blocks() {
        let m = LockManager::new(tmp_root("rw"));
        let a = m
            .try_acquire(&LockSpec::system_shared(), &info("A"))
            .unwrap();
        let b = m
            .try_acquire(&LockSpec::system_shared(), &info("B"))
            .unwrap();
        let c = conflict(m.try_acquire(&LockSpec::system_change(), &info("C")));
        assert_eq!(c.scope, Scope::System);
        assert_eq!(c.holders.len(), 2);
        drop(a);
        drop(b);
        let ex = m
            .try_acquire(&LockSpec::system_change(), &info("C"))
            .unwrap();
        let c = conflict(m.try_acquire(&LockSpec::system_shared(), &info("D")));
        assert_eq!(c.holders[0].label, "C");
        assert!(c
            .to_string()
            .starts_with("Blocked: C in progress (started "));
        drop(ex);
    }

    #[test]
    fn release_on_drop_frees_scope_and_holder_file() {
        let root = tmp_root("drop");
        let m = LockManager::new(&root);
        {
            let _g = m
                .try_acquire(&LockSpec::system_change(), &info("A"))
                .unwrap();
            assert_eq!(m.holders().len(), 1);
        }
        assert!(m.holders().is_empty());
        let holder_files = fs::read_dir(root.join("locks"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(HOLDER_EXT))
            .count();
        assert_eq!(holder_files, 0);
        m.try_acquire(&LockSpec::system_change(), &info("B"))
            .unwrap();
    }

    #[test]
    fn service_ops_parallel_but_not_same_service_or_under_activation() {
        let m = LockManager::new(tmp_root("svc"));
        let a = m
            .try_acquire(
                &LockSpec::service_data("a", ["docker-a"]),
                &info("restore a"),
            )
            .unwrap();
        // Another service: fine.
        let b = m
            .try_acquire(
                &LockSpec::service_data("b", ["docker-b"]),
                &info("restore b"),
            )
            .unwrap();
        // Same service / its unit: blocked.
        let c = conflict(m.try_acquire(&LockSpec::service_data("a", []), &info("clear a")));
        assert_eq!(c.scope, Scope::service("a"));
        let c = conflict(m.try_acquire(&LockSpec::unit("docker-a.service"), &info("restart")));
        assert_eq!(c.scope, Scope::unit("docker-a"));
        assert_eq!(c.holders[0].label, "restore a");
        // Unrelated unit: fine.
        m.try_acquire(&LockSpec::unit("docker-z"), &info("restart z"))
            .unwrap();
        // Activation waits for both restores.
        conflict(m.try_acquire(&LockSpec::system_change(), &info("activate")));
        drop(a);
        conflict(m.try_acquire(&LockSpec::system_change(), &info("activate")));
        drop(b);
        let act = m
            .try_acquire(&LockSpec::system_change(), &info("activate"))
            .unwrap();
        // ...and blocks a new restore.
        conflict(m.try_acquire(&LockSpec::service_data("a", ["docker-a"]), &info("restore")));
        // Unit control alone does not need the system scope.
        m.try_acquire(&LockSpec::unit("docker-a"), &info("restart"))
            .unwrap();
        drop(act);
    }

    #[test]
    fn all_or_nothing() {
        let m = LockManager::new(tmp_root("aon"));
        let held = m
            .try_acquire(&LockSpec::unit("docker-x"), &info("pull"))
            .unwrap();
        conflict(m.try_acquire(
            &LockSpec::service_data("x", ["docker-w", "docker-x"]),
            &info("restore"),
        ));
        // The system/service/docker-w locks taken before the conflict were released.
        m.try_acquire(&LockSpec::system_change(), &info("activate"))
            .unwrap();
        drop(held);
    }

    #[test]
    fn spec_merges_and_orders() {
        let s = LockSpec::new()
            .with(Scope::unit("u"), LockMode::Shared)
            .with(Scope::System, LockMode::Shared)
            .with(Scope::unit("u.service"), LockMode::Exclusive);
        assert_eq!(
            s.scopes(),
            &[
                (Scope::System, LockMode::Shared),
                (Scope::Unit("u.service".into()), LockMode::Exclusive)
            ]
        );
    }

    #[test]
    fn scope_keys_and_files() {
        assert_eq!(Scope::unit("docker-foo").key(), "unit/docker-foo.service");
        assert_eq!(Scope::unit("neo-foo.target").key(), "unit/neo-foo.target");
        assert_eq!(
            Scope::parse("service/calino"),
            Some(Scope::service("calino"))
        );
        assert_eq!(Scope::parse("system"), Some(Scope::System));
        assert_eq!(Scope::parse("bogus/x"), None);
        assert_eq!(
            Scope::unit("docker-foo").file_name(),
            "unit_docker-foo.service.lock"
        );
        assert_eq!(Scope::service("../x").file_name(), "service_..%2Fx.lock");
    }

    #[test]
    fn inherited_scopes_are_skipped() {
        let m = LockManager::new(tmp_root("inherit"));
        let _parent = m
            .try_acquire(&LockSpec::system_change(), &info("update"))
            .unwrap();
        let mut child = m.clone();
        child.inherited.insert(Scope::System);
        let g = child
            .try_acquire(&LockSpec::system_change(), &info("migrate"))
            .unwrap();
        assert!(g.is_empty());
    }

    #[test]
    fn wait_times_out_then_succeeds_after_release() {
        let m = LockManager::new(tmp_root("wait"));
        let g = m
            .try_acquire(&LockSpec::system_change(), &info("A"))
            .unwrap();
        let t = Instant::now();
        conflict(m.acquire(
            &LockSpec::system_change(),
            &info("B"),
            Duration::from_millis(300),
        ));
        assert!(t.elapsed() >= Duration::from_millis(300));
        let m2 = m.clone();
        let h = std::thread::spawn(move || {
            m2.acquire(
                &LockSpec::system_change(),
                &info("B"),
                Duration::from_secs(5),
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        });
        std::thread::sleep(Duration::from_millis(300));
        drop(g);
        h.join().unwrap().unwrap();
    }

    #[test]
    fn unit_guards_created_and_removed() {
        let root = tmp_root("guard");
        let m = LockManager::new(&root);
        let mut g = m
            .try_acquire(
                &LockSpec::service_data("a", ["docker-a"]),
                &info("restore a"),
            )
            .unwrap();
        g.guard_units(&["docker-a".to_string(), "neo-a.target".to_string()])
            .unwrap();
        assert!(root.join("guard/docker-a.service").exists());
        assert!(root.join("guard/neo-a.target").exists());
        assert_eq!(m.holders()[0].guarded_units.len(), 2);
        g.release_unit_guards();
        assert!(!root.join("guard/docker-a.service").exists());
        g.guard_units(&["docker-a".to_string()]).unwrap();
        drop(g);
        assert!(!root.join("guard/docker-a.service").exists());
        assert!(g_invalid(&m).is_err());
    }

    fn g_invalid(m: &LockManager) -> Result<(), String> {
        let mut g = m.try_acquire(&LockSpec::unit("x"), &info("x")).unwrap();
        g.guard_units(&["../evil".to_string()])
    }

    #[test]
    fn stale_holder_cleaned_with_its_guards_only() {
        let root = tmp_root("stale");
        let m = LockManager::new(&root);
        fs::create_dir_all(root.join("locks")).unwrap();
        fs::create_dir_all(root.join("guard")).unwrap();
        // A holder file nobody has locked = its process died.
        let dead = Holder {
            id: "999999-0".into(),
            scopes: vec![HeldScope {
                scope: "unit/docker-a.service".into(),
                mode: LockMode::Exclusive,
            }],
            kind: "restore".into(),
            label: "Restore".into(),
            op_id: None,
            pid: 999_999,
            started_at: 0,
            guarded_units: vec!["docker-a.service".into(), "docker-b.service".into()],
        };
        fs::write(
            root.join("locks/999999-0.holder"),
            serde_json::to_string(&dead).unwrap(),
        )
        .unwrap();
        fs::write(root.join("guard/docker-a.service"), "999999-0").unwrap();
        // Same unit name guarded by someone else: must survive.
        fs::write(root.join("guard/docker-b.service"), "other-1").unwrap();
        assert!(m.holders().is_empty());
        assert!(!root.join("locks/999999-0.holder").exists());
        assert!(!root.join("guard/docker-a.service").exists());
        assert!(root.join("guard/docker-b.service").exists());
    }

    #[test]
    fn conflict_without_holder_info() {
        let root = tmp_root("raw");
        let m = LockManager::new(&root);
        fs::create_dir_all(root.join("locks")).unwrap();
        // flock(1) from a shell script: lock only, no holder file.
        let f =
            open_lock_file(&root.join("locks").join(Scope::unit("docker-a").file_name())).unwrap();
        flock(&f, libc::LOCK_EX).unwrap();
        let c = conflict(m.try_acquire(&LockSpec::unit("docker-a"), &info("x")));
        assert_eq!(
            c.to_string(),
            "Blocked: unit docker-a.service is in use by another operation"
        );
    }
}
