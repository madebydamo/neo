//! Evaluations of the config repo ("generations"), their cached results and
//! the repls that compute them (docs/design/eval-latency.md).
//!
//! Pages are served from the newest evaluation that finished; a settings change
//! never blocks them. Settings saved since are laid over the results
//! (overlay.rs), the next evaluation runs in the background (in a second repl,
//! or in the same one with `neoSettingsDir`), replays every cached query, and
//! swaps in. Results that differ are announced over the WS so open pages reload
//! just those parts. Structural changes (modules, inputs, plugins) still make
//! the pages that list services wait.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio::sync::{broadcast, watch, Mutex as AsyncMutex, Notify};

use super::errors::eval_error_ui;
use super::extract::{error_pane_for, OptionIndex};
use super::overlay;
use super::repl::{remove_roots_matching, write_extract_files, NixEvaluator, ReplSpec};
use super::snapshot::{stat_probe, ChangeKind, Snapshot};
use crate::commands::web::guides::GuidesExtract;
use crate::commands::web::icons::IconSet;
use crate::commands::web::routes::privacy::PrivacyExtract;
use crate::commands::web::types::{
    EvalErrorUi, EvalMark, IndexContext, NavigatorContext, OptionPaneContext,
};
use crate::commands::web::util::escape_html;

/// A repl is replaced (not rebound again) after this many generations or this
/// much memory: the repl never frees an old binding.
const MAX_GENS_PER_REPL: usize = 6;
const MAX_REPL_RSS: u64 = 1536 * 1024 * 1024;
/// Saves within this window become one evaluation.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// How often the config repo is checked for changes made outside the web UI.
const PROBE_EVERY: Duration = Duration::from_secs(1);
/// A failed evaluation of the same files is retried no sooner than this.
const RETRY_FAILED_AFTER: Duration = Duration::from_secs(30);
/// Env: number of repls (2 = evaluate in the background, 1 = save memory).
const REPLS_ENV: &str = "NEO_WEB_EVAL_REPLS";

/// One eval-backed query. `id()` is what pages and the WS push name it by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum QueryKey {
    Services,
    Theme,
    Navigator,
    Guides,
    Privacy,
    OptionIndex,
    SecretPaths,
    PluginOwners,
    Pane { core: bool, name: String },
}

impl QueryKey {
    pub fn id(&self) -> String {
        match self {
            Self::Services => "services".into(),
            Self::Theme => "theme".into(),
            Self::Navigator => "navigator".into(),
            Self::Guides => "guides".into(),
            Self::Privacy => "privacy".into(),
            Self::OptionIndex => "option-index".into(),
            Self::SecretPaths => "secret-paths".into(),
            Self::PluginOwners => "plugin-owners".into(),
            Self::Pane { core: true, name } => format!("pane:core:{name}"),
            Self::Pane { core: false, name } => format!("pane:service:{name}"),
        }
    }

    /// Lists of services and plugins: wait for a structural change to land.
    fn structural(&self) -> bool {
        match self {
            Self::Services | Self::Navigator | Self::PluginOwners => true,
            Self::Pane { core, name } => *core && name == "plugins",
            _ => false,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Services => "Nix error while extracting service list",
            Self::Navigator => "Nix error while building navigator",
            Self::Guides => "Nix error while extracting guides",
            Self::Privacy => "Nix error while building the privacy map",
            Self::Pane { core: true, .. } => "Nix error for core section",
            Self::Pane { core: false, .. } => "Nix error for service options",
            _ => "Nix error",
        }
    }
}

/// A cached query result.
#[derive(Clone)]
enum Cached {
    Services(Arc<IndexContext>),
    Theme(Arc<String>),
    Navigator(Arc<NavigatorContext>),
    Pane(Arc<OptionPaneContext>),
    Guides(Arc<GuidesExtract>),
    Privacy(Arc<PrivacyExtract>),
    OptionIndex(Arc<OptionIndex>),
    SecretPaths(Arc<Vec<Vec<String>>>),
    PluginOwners(Arc<HashMap<String, Vec<String>>>),
}

impl Cached {
    /// Stable serialization for comparing two evaluations.
    fn json(&self) -> String {
        let v = match self {
            Self::Services(x) => serde_json::to_value(&**x),
            Self::Theme(x) => serde_json::to_value(&**x),
            Self::Navigator(x) => serde_json::to_value(&**x),
            Self::Pane(x) => serde_json::to_value(&**x),
            Self::Guides(x) => serde_json::to_value(&**x),
            Self::Privacy(x) => serde_json::to_value(&**x),
            Self::OptionIndex(x) => serde_json::to_value(&**x),
            Self::SecretPaths(x) => serde_json::to_value(&**x),
            Self::PluginOwners(x) => serde_json::to_value(&**x),
        };
        // serde_json::Value objects are sorted maps: HashMap order cannot differ.
        v.map(|v| v.to_string()).unwrap_or_default()
    }

    /// With `new` settings laid over a result evaluated from `old`.
    fn overlaid(&self, old: &Value, new: &Value) -> Cached {
        if old.is_null() || new.is_null() {
            return self.clone();
        }
        match self {
            Self::Services(x) => {
                let mut c = (**x).clone();
                if overlay::services(&mut c, old, new) {
                    return Self::Services(Arc::new(c));
                }
            }
            Self::Pane(x) => {
                let mut c = (**x).clone();
                if overlay::pane(&mut c, old, new) {
                    return Self::Pane(Arc::new(c));
                }
            }
            _ => {}
        }
        self.clone()
    }
}

async fn run_query(ev: &mut NixEvaluator, key: &QueryKey) -> Result<Cached> {
    Ok(match key {
        QueryKey::Services => Cached::Services(Arc::new(ev.extract_services().await?)),
        QueryKey::Theme => Cached::Theme(Arc::new(ev.extract_neo_theme().await?)),
        QueryKey::Navigator => Cached::Navigator(Arc::new(ev.extract_proxied_services().await?)),
        QueryKey::Guides => Cached::Guides(Arc::new(ev.extract_guides().await?)),
        QueryKey::Privacy => Cached::Privacy(Arc::new(ev.extract_privacy().await?)),
        QueryKey::OptionIndex => Cached::OptionIndex(Arc::new(ev.extract_option_index().await?)),
        QueryKey::SecretPaths => Cached::SecretPaths(Arc::new(ev.extract_secret_paths().await?)),
        QueryKey::PluginOwners => Cached::PluginOwners(Arc::new(ev.extract_plugin_owners().await?)),
        QueryKey::Pane { core: true, name } => {
            Cached::Pane(Arc::new(ev.extract_neo_section(name).await?))
        }
        QueryKey::Pane { core: false, name } => {
            Cached::Pane(Arc::new(ev.extract_service_options(name).await?))
        }
    })
}

struct Entry {
    value: Cached,
    used: Instant,
}

/// One evaluation of the config repo and the results read from it.
struct Generation {
    id: u64,
    snapshot: Arc<Snapshot>,
    /// Repl slot that holds `f_<id>`.
    slot: usize,
    /// Copy of the settings files this generation read (in-process rebind).
    dir: PathBuf,
    cache: Mutex<HashMap<QueryKey, Entry>>,
}

impl Generation {
    fn cache(&self) -> MutexGuard<'_, HashMap<QueryKey, Entry>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn get(&self, key: &QueryKey) -> Option<Cached> {
        let mut c = self.cache();
        let e = c.get_mut(key)?;
        e.used = Instant::now();
        Some(e.value.clone())
    }

    fn has(&self, key: &QueryKey) -> bool {
        self.cache().contains_key(key)
    }

    fn put(&self, key: QueryKey, value: Cached) {
        self.cache().insert(
            key,
            Entry {
                value,
                used: Instant::now(),
            },
        );
    }

    /// Cached keys, most recently used first.
    fn keys_mru(&self) -> Vec<QueryKey> {
        let c = self.cache();
        let mut keys: Vec<_> = c.iter().map(|(k, e)| (k.clone(), e.used)).collect();
        keys.sort_by_key(|k| std::cmp::Reverse(k.1));
        keys.into_iter().map(|(k, _)| k).collect()
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The newest evaluation failed (it is not served).
#[derive(Clone)]
struct Failure {
    content: u64,
    message: String,
    at: Instant,
}

struct State {
    serving: Option<Arc<Generation>>,
    building: Option<ChangeKind>,
    /// Newest snapshot of the config repo.
    latest: Arc<Snapshot>,
    failed: Option<Failure>,
    /// A fresh repl was requested (store repair, broken repl), with the reason.
    restart: Option<String>,
    /// Why the next evaluation runs (save, file change), for the log.
    reason: Option<String>,
    /// Key id → generation whose result last differed from the one before.
    changed: HashMap<String, u64>,
    /// Same, but compared with the older result plus overlay (what pages
    /// rendered provisionally showed).
    changed_overlaid: HashMap<String, u64>,
    next_id: u64,
    next_serial: u64,
}

struct Job {
    kind: ChangeKind,
    fresh: bool,
    why: String,
}

pub struct EvalPool {
    spec: Arc<ReplSpec>,
    icons: Arc<IconSet>,
    config_dir: PathBuf,
    base_dir: PathBuf,
    procs: Vec<Arc<AsyncMutex<Option<NixEvaluator>>>>,
    state: Mutex<State>,
    /// Bumped on every state change; waiters re-check.
    epoch: watch::Sender<u64>,
    wake: Notify,
    /// Pages wait for an evaluation (none yet, or a structural change).
    blocking: Arc<AtomicBool>,
    /// An evaluation is running or due.
    pending: Arc<AtomicBool>,
    push: broadcast::Sender<String>,
}

impl EvalPool {
    pub fn new(
        nix_cmd: &str,
        config_dir: &str,
        blocking: Arc<AtomicBool>,
        pending: Arc<AtomicBool>,
        push: broadcast::Sender<String>,
    ) -> Result<Arc<Self>> {
        let base_dir = std::env::temp_dir().join(format!("neo-web-eval-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base_dir);
        let extract_dir = base_dir.join("extract");
        write_extract_files(&extract_dir)?;
        let roots_dir = crate::commands::web::util::state_dir()
            .unwrap_or_else(|| base_dir.clone())
            .join("gcroots");
        // Roots of a previous neo-web run: its repls are gone.
        remove_roots_matching(&roots_dir, None);
        let neo_flake_path = std::env::var("NEO_FLAKE_PATH")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && PathBuf::from(s).is_dir());
        if let Some(p) = &neo_flake_path {
            println!("web: nix repl will load host flake with --override-input neo → {p}");
        }
        let spec = Arc::new(ReplSpec {
            nix_cmd: nix_cmd.to_string(),
            config_dir: config_dir.to_string(),
            neo_flake_path,
            extract_dir,
            roots_dir,
        });
        let repls = std::env::var(REPLS_ENV)
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(2)
            .clamp(1, 2);
        println!(
            "web: {repls} nix repl(s) for evaluations{}",
            if repls == 1 {
                " (no background evaluation)"
            } else {
                ""
            }
        );
        let latest = Arc::new(Snapshot::read(Path::new(config_dir)));
        blocking.store(true, Ordering::Relaxed);
        pending.store(true, Ordering::Relaxed);
        Ok(Arc::new(Self {
            spec,
            icons: Arc::new(IconSet::nix_store()),
            config_dir: PathBuf::from(config_dir),
            base_dir,
            procs: (0..repls)
                .map(|_| Arc::new(AsyncMutex::new(None)))
                .collect(),
            state: Mutex::new(State {
                serving: None,
                building: None,
                latest,
                failed: None,
                restart: None,
                reason: Some("startup".into()),
                changed: HashMap::new(),
                changed_overlaid: HashMap::new(),
                next_id: 1,
                next_serial: 1,
            }),
            epoch: watch::channel(0).0,
            wake: Notify::new(),
            blocking,
            pending,
            push,
        }))
    }

    pub fn icons(&self) -> Arc<IconSet> {
        self.icons.clone()
    }

    /// Run the evaluation loop (once, at startup).
    pub fn start(self: &Arc<Self>) {
        let pool = self.clone();
        tokio::spawn(async move { pool.supervise().await });
    }

    fn st(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn bump(&self) {
        self.epoch.send_modify(|e| *e += 1);
        self.update_flags();
    }

    fn update_flags(&self) {
        let st = self.st();
        let serving_content = st.serving.as_ref().map(|g| g.snapshot.content);
        let blocking = st.serving.is_none() && st.failed.is_none()
            || st.building == Some(ChangeKind::Structural);
        let failed_latest = st
            .failed
            .as_ref()
            .is_some_and(|f| f.content == st.latest.content);
        let pending = st.building.is_some()
            || st.restart.is_some()
            || (serving_content != Some(st.latest.content) && !failed_latest);
        self.blocking.store(blocking, Ordering::Relaxed);
        self.pending.store(pending, Ordering::Relaxed);
    }

    // ---- change detection ---------------------------------------------------

    /// Re-read the config repo when any file moved. True when contents changed.
    fn probe(&self, force: bool) -> bool {
        let stat = stat_probe(&self.config_dir);
        if !force && stat == self.st().latest.stat {
            return false;
        }
        let snap = Snapshot::read(&self.config_dir);
        let mut st = self.st();
        let changed = snap.content != st.latest.content;
        st.latest = Arc::new(snap);
        drop(st);
        if changed {
            self.update_flags();
        }
        changed
    }

    /// Settings were written by the web UI (save, restore, secrets, branch
    /// switch): evaluate them in the background. Never blocks on Nix.
    pub fn note_change(&self, why: &str) {
        if self.probe(true) {
            self.st().reason = Some(why.to_string());
            self.bump();
        }
        self.wake.notify_one();
    }

    /// Replace the repls (e.g. after a store repair) and wait for the new
    /// evaluation. Err with the evaluation error when it fails.
    pub async fn restart_and_wait(&self, why: &str) -> Result<(), String> {
        let mut rx = self.epoch.subscribe();
        let before = {
            let mut st = self.st();
            st.restart = Some(why.to_string());
            st.serving.as_ref().map_or(0, |g| g.id)
        };
        self.bump();
        self.wake.notify_one();
        let wait = async {
            loop {
                rx.borrow_and_update();
                {
                    let st = self.st();
                    if st.restart.is_none() && st.building.is_none() {
                        if st.serving.as_ref().is_some_and(|g| g.id > before) {
                            return Ok(());
                        }
                        if let Some(f) = &st.failed {
                            return Err(f.message.clone());
                        }
                    }
                }
                if rx.changed().await.is_err() {
                    return Err("evaluator stopped".to_string());
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(900), wait)
            .await
            .unwrap_or_else(|_| Err("timed out waiting for the new evaluation".to_string()))
    }

    // ---- serving ------------------------------------------------------------

    fn failure_error(st: &State) -> Option<anyhow::Error> {
        st.failed.as_ref().map(|f| anyhow!("{}", f.message))
    }

    /// The generation to serve `key` from, waiting while there is none yet or a
    /// structural change is being evaluated for a list of services.
    async fn serving_for(
        &self,
        key: &QueryKey,
        rx: &mut watch::Receiver<u64>,
    ) -> Result<(Arc<Generation>, Arc<Snapshot>)> {
        loop {
            rx.borrow_and_update();
            let decision = {
                let st = self.st();
                let busy = st.building.is_some() || st.restart.is_some();
                let structural_wait =
                    key.structural() && st.building == Some(ChangeKind::Structural);
                match &st.serving {
                    Some(g) if !structural_wait => Some(Ok((g.clone(), st.latest.clone()))),
                    None if !busy => Self::failure_error(&st).map(Err),
                    _ => None,
                }
            };
            match decision {
                Some(Ok(x)) => return Ok(x),
                Some(Err(e)) => {
                    // Retry (rate-limited by the supervisor) while reporting it.
                    self.wake.notify_one();
                    return Err(e);
                }
                None => {}
            }
            if rx.changed().await.is_err() {
                return Err(anyhow!("evaluator stopped"));
            }
        }
    }

    async fn get(&self, key: &QueryKey) -> (Result<Cached>, EvalMark) {
        let mut rx = self.epoch.subscribe();
        loop {
            let (gen, latest) = match self.serving_for(key, &mut rx).await {
                Ok(x) => x,
                Err(e) => return (Err(e), EvalMark::default()),
            };
            let provisional = gen.snapshot.content != latest.content;
            let mark = EvalMark {
                gen: gen.id,
                provisional,
            };
            let finish = |c: Cached| {
                if provisional {
                    c.overlaid(&gen.snapshot.settings, &latest.settings)
                } else {
                    c
                }
            };
            if let Some(c) = gen.get(key) {
                return (Ok(finish(c)), mark);
            }
            let slot = self.procs[gen.slot].clone();
            let mut guard = slot.lock().await;
            // Warm-up or another request may have filled it while we waited.
            if let Some(c) = gen.get(key) {
                return (Ok(finish(c)), mark);
            }
            let Some(ev) = guard.as_mut().filter(|ev| ev.has_gen(gen.id) && !ev.broken) else {
                drop(guard);
                // This generation's repl is gone (replaced or broken).
                {
                    let mut st = self.st();
                    let serving = st.serving.as_ref().is_some_and(|g| g.id == gen.id);
                    if serving && st.building.is_none() && st.restart.is_none() {
                        if let Some(e) = Self::failure_error(&st) {
                            return (Err(e), mark);
                        }
                        st.restart = Some("the serving repl is gone".into());
                        drop(st);
                        self.bump();
                        self.wake.notify_one();
                    }
                }
                let _ = rx.changed().await;
                continue;
            };
            ev.use_gen(gen.id);
            let t = Instant::now();
            let r = run_query(ev, key).await;
            let secs = t.elapsed().as_secs_f64();
            if secs >= 0.5 || r.is_err() {
                println!(
                    "web: eval {} on generation {}: {secs:.1}s{}",
                    key.id(),
                    gen.id,
                    if r.is_err() { " (failed)" } else { "" }
                );
            }
            if ev.broken {
                *guard = None;
                drop(guard);
                {
                    let mut st = self.st();
                    if st.serving.as_ref().is_some_and(|g| g.id == gen.id) {
                        st.restart = Some(format!("repl broke during {}", key.id()));
                    }
                }
                self.bump();
                self.wake.notify_one();
            }
            return match r {
                Ok(c) => {
                    gen.put(key.clone(), c.clone());
                    (Ok(finish(c)), mark)
                }
                Err(e) => (Err(e), mark),
            };
        }
    }

    // ---- typed queries ------------------------------------------------------

    pub async fn services(&self) -> IndexContext {
        let (r, mark) = self.get(&QueryKey::Services).await;
        let mut ctx = match r {
            Ok(Cached::Services(x)) => (*x).clone(),
            other => IndexContext {
                eval_error: self.error_ui(&QueryKey::Services, other.err()),
                ..Default::default()
            },
        };
        ctx.eval = mark;
        ctx
    }

    pub async fn theme(&self) -> String {
        match self.get(&QueryKey::Theme).await.0 {
            Ok(Cached::Theme(t)) => (*t).clone(),
            _ => "light".to_string(),
        }
    }

    pub async fn navigator(&self) -> NavigatorContext {
        match self.get(&QueryKey::Navigator).await.0 {
            Ok(Cached::Navigator(n)) => (*n).clone(),
            other => NavigatorContext {
                eval_error: self.error_ui(&QueryKey::Navigator, other.err()),
                ..Default::default()
            },
        }
    }

    /// A service (or core section) pane, settings saved since laid over it.
    /// Secrets are still placeholders on encrypted configs (panes.rs reveals them).
    pub async fn pane(&self, is_core: bool, name: &str) -> OptionPaneContext {
        let key = QueryKey::Pane {
            core: is_core,
            name: name.to_string(),
        };
        let (r, mark) = self.get(&key).await;
        let mut pane = match r {
            Ok(Cached::Pane(p)) => (*p).clone(),
            other => {
                let mut p = error_pane_for(name, is_core);
                let what = format!("{} ({name})", key.label());
                p.eval_error = match other {
                    Err(e) => eval_error_ui(&what, &e),
                    Ok(_) => EvalErrorUi::message(what),
                };
                p
            }
        };
        pane.eval = mark;
        pane
    }

    pub async fn guides(&self) -> (Result<GuidesExtract>, EvalMark) {
        let (r, mark) = self.get(&QueryKey::Guides).await;
        let r = r.and_then(|c| match c {
            Cached::Guides(g) => Ok((*g).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        });
        (r, mark)
    }

    pub async fn privacy(&self) -> (Result<PrivacyExtract>, EvalMark) {
        let (r, mark) = self.get(&QueryKey::Privacy).await;
        let r = r.and_then(|c| match c {
            Cached::Privacy(p) => Ok((*p).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        });
        (r, mark)
    }

    pub async fn option_index(&self) -> Result<OptionIndex> {
        match self.get(&QueryKey::OptionIndex).await.0? {
            Cached::OptionIndex(i) => Ok((*i).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        }
    }

    pub async fn secret_paths(&self) -> Result<Vec<Vec<String>>> {
        match self.get(&QueryKey::SecretPaths).await.0? {
            Cached::SecretPaths(p) => Ok((*p).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        }
    }

    pub async fn plugin_owners(&self) -> Result<HashMap<String, Vec<String>>> {
        match self.get(&QueryKey::PluginOwners).await.0? {
            Cached::PluginOwners(o) => Ok((*o).clone()),
            _ => Err(anyhow!("unexpected cache entry")),
        }
    }

    fn error_ui(&self, key: &QueryKey, err: Option<anyhow::Error>) -> EvalErrorUi {
        let err = err.unwrap_or_else(|| anyhow!("unexpected cache entry"));
        eprintln!("web: {} failed: {err:#}", key.id());
        eval_error_ui(key.label(), &err)
    }

    // ---- evaluation loop ----------------------------------------------------

    async fn supervise(self: Arc<Self>) {
        loop {
            if self.next_job().is_none() {
                tokio::select! {
                    _ = self.wake.notified() => {}
                    _ = tokio::time::sleep(PROBE_EVERY) => {}
                }
                if self.probe(false) {
                    let mut st = self.st();
                    st.reason
                        .get_or_insert_with(|| "config files changed".into());
                }
                continue;
            }
            // Let a burst of saves settle into one evaluation.
            tokio::time::sleep(DEBOUNCE).await;
            self.probe(false);
            if let Some(job) = self.next_job() {
                self.build(job).await;
            }
        }
    }

    fn next_job(&self) -> Option<Job> {
        let st = self.st();
        let latest = &st.latest;
        let recent_failure = st
            .failed
            .as_ref()
            .is_some_and(|f| f.content == latest.content && f.at.elapsed() < RETRY_FAILED_AFTER);
        let reason = st.reason.clone();
        if let Some(why) = &st.restart {
            let kind = st
                .serving
                .as_ref()
                .and_then(|g| g.snapshot.change_to(latest))
                .unwrap_or(ChangeKind::Values);
            return Some(Job {
                kind,
                fresh: true,
                why: why.clone(),
            });
        }
        match &st.serving {
            None if recent_failure => None,
            None => Some(Job {
                kind: ChangeKind::Structural,
                fresh: true,
                why: reason.unwrap_or_else(|| "retry after a failed evaluation".into()),
            }),
            Some(g) => {
                let kind = g.snapshot.change_to(latest)?;
                // The same files failed before: wait for the next change, unless
                // the serving generation lost its repl (retry now and then).
                if st
                    .failed
                    .as_ref()
                    .is_some_and(|f| f.content == latest.content)
                {
                    let repl_gone = self.procs[g.slot]
                        .try_lock()
                        .is_ok_and(|p| p.as_ref().is_none_or(|ev| !ev.has_gen(g.id)));
                    if recent_failure || !repl_gone {
                        return None;
                    }
                    return Some(Job {
                        kind,
                        fresh: true,
                        why: "retry: the serving repl is gone".into(),
                    });
                }
                Some(Job {
                    kind,
                    fresh: false,
                    why: reason.unwrap_or_else(|| "config files changed".into()),
                })
            }
        }
    }

    async fn build(self: &Arc<Self>, job: Job) {
        let started = Instant::now();
        let (id, snapshot, old) = {
            let mut st = self.st();
            let id = st.next_id;
            st.next_id += 1;
            st.building = Some(job.kind);
            st.restart = None;
            st.reason = None;
            (id, st.latest.clone(), st.serving.clone())
        };
        self.bump();
        let kind = match job.kind {
            ChangeKind::Values => "settings",
            ChangeKind::Structural => "structure",
        };
        println!(
            "web: evaluating generation {id} ({kind} change: {}{})",
            job.why,
            if job.fresh { ", new repl" } else { "" }
        );
        let slot = match (&old, self.procs.len()) {
            (Some(g), 2) => 1 - g.slot,
            _ => 0,
        };
        let gen = Arc::new(Generation {
            id,
            snapshot,
            slot,
            dir: self.base_dir.join(format!("gen-{id}")),
            cache: Mutex::new(HashMap::new()),
        });
        let proc_slot = self.procs[slot].clone();
        let mut guard = proc_slot.lock().await;
        let result = self.evaluate(&gen, old.as_deref(), &job, &mut guard).await;
        if guard.as_ref().is_some_and(|ev| ev.broken) {
            *guard = None;
        }
        match result {
            Ok(rest) => {
                let (changed, overlaid) = old.as_deref().map(|o| diff(o, &gen)).unwrap_or_default();
                {
                    let mut st = self.st();
                    st.serving = Some(gen.clone());
                    st.building = None;
                    st.failed = None;
                    for k in &changed {
                        st.changed.insert(k.clone(), id);
                    }
                    for k in &overlaid {
                        st.changed_overlaid.insert(k.clone(), id);
                    }
                }
                drop(guard);
                self.bump();
                println!(
                    "web: generation {id} serving after {:.1}s; changed: {}",
                    started.elapsed().as_secs_f64(),
                    if changed.is_empty() {
                        "nothing".to_string()
                    } else {
                        changed.join(", ")
                    }
                );
                self.push_status();
                match old {
                    None => self.after_cold_start(),
                    Some(old) if !rest.is_empty() => self.finish_replay(old, gen, rest),
                    Some(_) => {}
                }
            }
            Err(e) => {
                drop(guard);
                let message = format!("{e:#}");
                println!(
                    "web: generation {id} failed after {:.1}s: {message}",
                    started.elapsed().as_secs_f64()
                );
                {
                    let mut st = self.st();
                    st.building = None;
                    st.failed = Some(Failure {
                        content: gen.snapshot.content,
                        message,
                        at: Instant::now(),
                    });
                }
                self.bump();
                self.push_status();
            }
        }
    }

    /// Bind `gen` in its repl (rebinding in place or starting a fresh one) and
    /// compute the results pages need: the configuration page's first, then
    /// everything the serving generation had cached. Fails when the change
    /// breaks a query that worked before. A structural change computes only the
    /// lists pages wait for; the other keys are returned for [`Self::finish_replay`].
    async fn evaluate(
        &self,
        gen: &Generation,
        old: Option<&Generation>,
        job: &Job,
        guard: &mut Option<NixEvaluator>,
    ) -> Result<Vec<QueryKey>> {
        let t = Instant::now();
        let reuse = !job.fresh
            && guard.as_ref().is_some_and(|ev| {
                !ev.broken
                    && ev.supports_rebind
                    && ev.structure == gen.snapshot.structure
                    && ev.bound_count() < MAX_GENS_PER_REPL
                    && ev.rss_bytes() < MAX_REPL_RSS
            });
        if !reuse {
            // Free the old repl before loading another.
            *guard = None;
            let why = format!("generation {}: {}", gen.id, job.why);
            let ev = NixEvaluator::spawn(
                self.spec.clone(),
                self.icons.clone(),
                self.next_serial(),
                gen.snapshot.structure,
                &why,
            )
            .await?;
            *guard = Some(ev);
        }
        let Some(ev) = guard.as_mut() else {
            return Err(anyhow!("no nix repl"));
        };
        gen.snapshot.write_files(&gen.dir)?;
        ev.bind(gen.id, Some(&gen.dir)).await?;
        let bound = t.elapsed().as_secs_f64();

        let mut keys = vec![QueryKey::Services, QueryKey::Theme];
        let mut rest = Vec::new();
        for k in old.map(Generation::keys_mru).unwrap_or_default() {
            if keys.contains(&k) {
                continue;
            }
            if job.kind == ChangeKind::Structural && !k.structural() {
                rest.push(k);
            } else {
                keys.push(k);
            }
        }
        for k in &keys {
            ev.use_gen(gen.id);
            match run_query(ev, k).await {
                Ok(c) => gen.put(k.clone(), c),
                Err(e) if ev.broken || old.is_some_and(|o| o.has(k)) => return Err(e),
                // Never worked: not this change's fault; the page shows the error.
                Err(_) => {}
            }
        }
        if !reuse {
            ev.add_gc_roots();
        }
        println!(
            "web: generation {} bound in {bound:.1}s ({}), {} results in {:.1}s",
            gen.id,
            if reuse { "same repl" } else { "new repl" },
            keys.len(),
            t.elapsed().as_secs_f64() - bound,
        );
        Ok(rest)
    }

    /// After a structural change was swapped in: compute the remaining results
    /// the previous generation had (through the cache, so pages asking first
    /// share the work), then announce what changed.
    fn finish_replay(
        self: &Arc<Self>,
        old: Arc<Generation>,
        gen: Arc<Generation>,
        keys: Vec<QueryKey>,
    ) {
        let pool = self.clone();
        tokio::spawn(async move {
            let t = Instant::now();
            for k in &keys {
                if pool.st().serving.as_ref().map(|g| g.id) != Some(gen.id) {
                    return;
                }
                let _ = pool.get(k).await;
            }
            let (changed, overlaid) = diff(&old, &gen);
            {
                let mut st = pool.st();
                for k in &changed {
                    let e = st.changed.entry(k.clone()).or_insert(gen.id);
                    *e = (*e).max(gen.id);
                }
                for k in &overlaid {
                    let e = st.changed_overlaid.entry(k.clone()).or_insert(gen.id);
                    *e = (*e).max(gen.id);
                }
            }
            println!(
                "web: generation {} replayed {} more results in {:.1}s; changed: {}",
                gen.id,
                keys.len(),
                t.elapsed().as_secs_f64(),
                if changed.is_empty() {
                    "nothing".to_string()
                } else {
                    changed.join(", ")
                }
            );
            if !changed.is_empty() {
                pool.push_status();
            }
        });
    }

    /// After the first evaluation: compute what other pages read, so their
    /// first visit is fast too, and start the second repl for the next change.
    fn after_cold_start(self: &Arc<Self>) {
        let pool = self.clone();
        tokio::spawn(async move {
            for key in [
                QueryKey::Navigator,
                QueryKey::Guides,
                QueryKey::Privacy,
                QueryKey::SecretPaths,
                QueryKey::OptionIndex,
            ] {
                let _ = pool.get(&key).await;
            }
            pool.prestart_spare().await;
        });
    }

    /// Load the flake in the idle slot so the next settings change only re-evaluates.
    async fn prestart_spare(&self) {
        // Without rebind support every change loads the flake anew anyway.
        if self.procs.len() < 2 || !self.spec.supports_rebind() {
            return;
        }
        let Some(spare) = self.st().serving.as_ref().map(|g| 1 - g.slot) else {
            return;
        };
        let slot = self.procs[spare].clone();
        let mut guard = slot.lock().await;
        if guard.is_some() {
            return;
        }
        let structure = self.st().latest.structure;
        match NixEvaluator::spawn(
            self.spec.clone(),
            self.icons.clone(),
            self.next_serial(),
            structure,
            "spare for the next change",
        )
        .await
        {
            Ok(ev) => {
                ev.add_gc_roots();
                *guard = Some(ev);
            }
            Err(e) => println!("web: spare nix repl not started: {e:#}"),
        }
    }

    fn next_serial(&self) -> u64 {
        let mut st = self.st();
        st.next_serial += 1;
        st.next_serial - 1
    }

    // ---- page status ---------------------------------------------------------

    /// `#neo-eval-sync` (read by static/eval_sync.js) plus `#neo-eval-error`,
    /// as OOB swaps for the WS or inline for the page.
    pub fn status_html(&self, oob: bool) -> String {
        let st = self.st();
        let gen = st.serving.as_ref().map_or(0, |g| g.id);
        let theme = st
            .serving
            .as_ref()
            .and_then(|g| g.get(&QueryKey::Theme))
            .and_then(|c| match c {
                Cached::Theme(t) => Some((*t).clone()),
                _ => None,
            })
            .unwrap_or_default();
        let json = |m: &HashMap<String, u64>| {
            escape_html(&serde_json::to_string(m).unwrap_or_else(|_| "{}".into()))
        };
        let oob_attr = if oob { r#" hx-swap-oob="true""# } else { "" };
        let sync = format!(
            r#"<div id="neo-eval-sync" hidden{oob_attr} data-gen="{gen}" data-pending="{}" data-theme="{}" data-changed="{}" data-changed-overlaid="{}"></div>"#,
            self.pending.load(Ordering::Relaxed),
            escape_html(&theme),
            json(&st.changed),
            json(&st.changed_overlaid),
        );
        let failure = st
            .failed
            .as_ref()
            .filter(|f| f.content == st.latest.content && st.serving.is_some());
        let error = match failure {
            None => format!(r#"<div id="neo-eval-error"{oob_attr}></div>"#),
            Some(f) => {
                let ui = eval_error_ui(
                    "Your latest change does not evaluate",
                    &anyhow!("{}", f.message),
                );
                format!(
                    r#"<div id="neo-eval-error"{oob_attr}><div role="alert" class="alert alert-error alert-soft mb-3 text-sm items-start"><div class="min-w-0 flex-1"><div class="font-semibold">Your latest change does not evaluate</div><div class="mt-0.5 text-xs opacity-80">Pages show the last configuration that did, with your saved values. Fix the setting and save again.</div><div class="mt-2 text-xs whitespace-pre-wrap break-words font-mono opacity-80 max-h-40 overflow-auto">{}</div></div></div></div>"#,
                    escape_html(ui.error.as_deref().unwrap_or(&f.message)),
                )
            }
        };
        format!("{sync}{error}")
    }

    fn push_status(&self) {
        let _ = self.push.send(self.status_html(true));
    }
}

/// Keys whose result differs between two generations: plainly, and against
/// the older result with the newer settings laid over it.
fn diff(old: &Generation, new: &Generation) -> (Vec<String>, Vec<String>) {
    let mut changed = Vec::new();
    let mut overlaid = Vec::new();
    let new_cache = new.cache();
    let old_cache = old.cache();
    for (k, n) in new_cache.iter() {
        let Some(o) = old_cache.get(k) else {
            continue;
        };
        let nj = n.value.json();
        if o.value.json() != nj {
            changed.push(k.id());
        }
        let predicted = o
            .value
            .overlaid(&old.snapshot.settings, &new.snapshot.settings)
            .json();
        if predicted != nj {
            overlaid.push(k.id());
        }
    }
    changed.sort();
    overlaid.sort();
    (changed, overlaid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn generation(id: u64, settings: Value) -> Generation {
        Generation {
            id,
            snapshot: Arc::new(Snapshot {
                settings,
                ..Default::default()
            }),
            slot: 0,
            dir: std::env::temp_dir().join(format!("neo-pool-test-{}-{id}", std::process::id())),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn pane(subdomain: &str) -> Cached {
        let mut p = error_pane_for("immich", false);
        // As extract_pane fills them.
        p.options = vec![serde_json::from_value(json!({
            "name": "subdomain", "type": {"kind": "str"}, "typeLabel": "str",
            "default": "immich", "current": subdomain, "currentDisplay": subdomain,
            "defaultDisplay": "immich"
        }))
        .unwrap()];
        p.options_json = serde_json::to_string(&p.options).unwrap();
        Cached::Pane(Arc::new(p))
    }

    #[test]
    fn keys_ids_and_structure() {
        assert_eq!(
            QueryKey::Pane {
                core: false,
                name: "a".into()
            }
            .id(),
            "pane:service:a"
        );
        assert_eq!(
            QueryKey::Pane {
                core: true,
                name: "ssh".into()
            }
            .id(),
            "pane:core:ssh"
        );
        assert!(QueryKey::Services.structural());
        assert!(QueryKey::Pane {
            core: true,
            name: "plugins".into()
        }
        .structural());
        assert!(!QueryKey::Pane {
            core: false,
            name: "plugins".into()
        }
        .structural());
        assert!(!QueryKey::Theme.structural());
    }

    #[test]
    fn mru_order() {
        let g = generation(1, json!({}));
        g.put(QueryKey::Services, Cached::Theme(Arc::new("x".into())));
        std::thread::sleep(Duration::from_millis(2));
        g.put(QueryKey::Theme, Cached::Theme(Arc::new("light".into())));
        std::thread::sleep(Duration::from_millis(2));
        let _ = g.get(&QueryKey::Services);
        assert_eq!(g.keys_mru(), [QueryKey::Services, QueryKey::Theme]);
    }

    #[test]
    fn diff_separates_predicted_changes() {
        let key = QueryKey::Pane {
            core: false,
            name: "immich".into(),
        };
        let old = generation(1, json!({"services": {"immich": {"subdomain": "a"}}}));
        let new = generation(2, json!({"services": {"immich": {"subdomain": "b"}}}));
        old.put(key.clone(), pane("a"));
        old.put(QueryKey::Theme, Cached::Theme(Arc::new("light".into())));
        new.put(key.clone(), pane("b"));
        new.put(QueryKey::Theme, Cached::Theme(Arc::new("light".into())));
        // The pane changed, but exactly as the overlay predicted.
        assert_eq!(
            diff(&old, &new),
            (vec!["pane:service:immich".into()], vec![])
        );

        // A derived difference the overlay cannot know.
        let other = generation(3, json!({"services": {"immich": {"subdomain": "b"}}}));
        other.put(key.clone(), pane("derived"));
        assert_eq!(
            diff(&old, &other),
            (
                vec!["pane:service:immich".into()],
                vec!["pane:service:immich".into()]
            )
        );
    }
}
