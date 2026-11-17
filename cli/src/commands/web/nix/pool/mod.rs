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
use tokio::sync::{broadcast, watch, Mutex as AsyncMutex, Notify};

use super::repl::{remove_roots_matching, write_extract_files, NixEvaluator, ReplSpec};
use super::snapshot::{stat_probe, ChangeKind, Snapshot};
use crate::commands::web::icons::IconSet;
use crate::commands::web::types::EvalMark;

mod generation;
mod query;
mod scheduler;
mod status;

use generation::Generation;
pub use query::QueryKey;
use query::{run_query, Cached};

/// Env: number of repls (2 = evaluate in the background, 1 = save memory).
const REPLS_ENV: &str = "NEO_WEB_EVAL_REPLS";

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

impl State {
    fn new(latest: Arc<Snapshot>) -> Self {
        Self {
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
        }
    }

    fn serves(&self, gen_id: u64) -> bool {
        self.serving.as_ref().is_some_and(|g| g.id == gen_id)
    }

    /// The newest evaluation's error, when it failed.
    fn failure_error(&self) -> Option<anyhow::Error> {
        self.failed.as_ref().map(|f| anyhow!("{}", f.message))
    }

    /// The repl of generation `gen_id` is gone (replaced or broken). When it is
    /// the serving one and nothing is evaluating: Err with the newest failure,
    /// else a fresh repl is asked for (true: wake the supervisor).
    fn repl_gone(&mut self, gen_id: u64) -> Result<bool> {
        if !self.serves(gen_id) || self.building.is_some() || self.restart.is_some() {
            return Ok(false);
        }
        if let Some(e) = self.failure_error() {
            return Err(e);
        }
        self.restart = Some("the serving repl is gone".into());
        Ok(true)
    }

    /// The repl broke while answering `key` for generation `gen_id`: a fresh
    /// one is asked for when that generation is serving.
    fn repl_broke(&mut self, gen_id: u64, key: &QueryKey) {
        if self.serves(gen_id) {
            self.restart = Some(format!("repl broke during {}", key.id()));
        }
    }
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
            eprintln!("web: nix repl will load host flake with --override-input neo → {p}");
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
        eprintln!(
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
            state: Mutex::new(State::new(latest)),
            epoch: watch::channel(0).0,
            wake: Notify::new(),
            blocking,
            pending,
            push,
        }))
    }

    /// A pool that never evaluates (tests of code that takes an AppConfig but
    /// reads no evaluation): no repl, extract files or GC roots.
    #[cfg(test)]
    pub fn for_tests(config_dir: &Path) -> Arc<Self> {
        let base_dir = std::env::temp_dir().join("neo-web-eval-tests");
        Arc::new(Self {
            spec: Arc::new(ReplSpec {
                nix_cmd: "nix".into(),
                config_dir: config_dir.display().to_string(),
                neo_flake_path: None,
                extract_dir: base_dir.join("extract"),
                roots_dir: base_dir.join("gcroots"),
            }),
            icons: Arc::new(IconSet::nix_store()),
            config_dir: config_dir.to_path_buf(),
            base_dir,
            procs: Vec::new(),
            state: Mutex::new(State::new(Arc::new(Snapshot::default()))),
            epoch: watch::channel(0).0,
            wake: Notify::new(),
            blocking: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicBool::new(false)),
            push: broadcast::channel(1).0,
        })
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
                    key.waits_for_structure() && st.building == Some(ChangeKind::Structural);
                match &st.serving {
                    Some(g) if !structural_wait => Some(Ok((g.clone(), st.latest.clone()))),
                    None if !busy => st.failure_error().map(Err),
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
                let gone = self.st().repl_gone(gen.id);
                match gone {
                    Err(e) => return (Err(e), mark),
                    Ok(true) => {
                        self.bump();
                        self.wake.notify_one();
                    }
                    Ok(false) => {}
                }
                let _ = rx.changed().await;
                continue;
            };
            ev.use_gen(gen.id);
            let t = Instant::now();
            let r = run_query(ev, key).await;
            let secs = t.elapsed().as_secs_f64();
            if secs >= 0.5 || r.is_err() {
                eprintln!(
                    "web: eval {} on generation {}: {secs:.1}s{}",
                    key.id(),
                    gen.id,
                    if r.is_err() { " (failed)" } else { "" }
                );
            }
            if ev.broken {
                *guard = None;
                drop(guard);
                self.st().repl_broke(gen.id, key);
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
}

#[cfg(test)]
mod tests {
    use super::generation::test_generation;
    use super::*;

    fn serving_state() -> State {
        let mut st = State::new(Arc::new(Snapshot::default()));
        st.serving = Some(Arc::new(test_generation(1, Snapshot::default())));
        st
    }

    #[test]
    fn gone_serving_repl_asks_for_a_restart() {
        let mut st = serving_state();
        assert!(st.repl_gone(1).unwrap());
        assert_eq!(st.restart.as_deref(), Some("the serving repl is gone"));
        // Asked for already: nothing more to wake.
        assert!(!st.repl_gone(1).unwrap());
    }

    #[test]
    fn gone_repl_of_an_older_generation_or_while_evaluating_waits() {
        let mut st = serving_state();
        assert!(!st.repl_gone(0).unwrap());
        st.building = Some(ChangeKind::Values);
        assert!(!st.repl_gone(1).unwrap());
        assert_eq!(st.restart, None);
    }

    #[test]
    fn gone_serving_repl_reports_the_newest_failure() {
        let mut st = serving_state();
        st.failed = Some(Failure {
            content: 0,
            message: "boom".into(),
            at: Instant::now(),
        });
        assert_eq!(st.repl_gone(1).unwrap_err().to_string(), "boom");
        assert_eq!(st.restart, None);
    }

    #[test]
    fn broken_repl_restarts_only_the_serving_generation() {
        let mut st = serving_state();
        st.repl_broke(0, &QueryKey::Theme);
        assert_eq!(st.restart, None);
        st.repl_broke(1, &QueryKey::Theme);
        assert_eq!(st.restart.as_deref(), Some("repl broke during theme"));
    }
}
