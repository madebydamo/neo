//! The evaluation loop: when to evaluate ([`decide`]), binding a generation in
//! a repl, swapping it in and replaying what the previous one had cached.
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use super::super::repl::NixEvaluator;
use super::super::snapshot::ChangeKind;
use super::generation::{diff, Generation};
use super::query::{run_query, QueryKey};
use super::{EvalPool, Failure, State};

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

/// An evaluation to run.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Job {
    pub(super) kind: ChangeKind,
    /// In a new repl (not rebound in the one that has the old generation).
    pub(super) fresh: bool,
    pub(super) why: String,
}

/// Whether to evaluate now, and how. `repl_gone`: the serving generation's
/// repl no longer holds it (replaced or broken).
pub(super) fn decide(st: &State, repl_gone: bool, now: Instant) -> Option<Job> {
    let latest = &st.latest;
    let failed_latest = st.failed.as_ref().filter(|f| f.content == latest.content);
    let recent_failure =
        failed_latest.is_some_and(|f| now.saturating_duration_since(f.at) < RETRY_FAILED_AFTER);
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
            if failed_latest.is_some() {
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

impl EvalPool {
    pub(super) async fn supervise(self: Arc<Self>) {
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
        // A busy slot is in use, so its repl is not gone.
        let repl_gone = st.serving.as_ref().is_some_and(|g| {
            self.procs[g.slot]
                .try_lock()
                .is_ok_and(|p| p.as_ref().is_none_or(|ev| !ev.has_gen(g.id)))
        });
        decide(&st, repl_gone, Instant::now())
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
        eprintln!(
            "web: evaluating generation {id} ({kind} change: {}{})",
            job.why,
            if job.fresh { ", new repl" } else { "" }
        );
        let slot = match (&old, self.procs.len()) {
            (Some(g), 2) => 1 - g.slot,
            _ => 0,
        };
        let gen = Arc::new(Generation::new(
            id,
            snapshot,
            slot,
            self.base_dir.join(format!("gen-{id}")),
        ));
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
                eprintln!(
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
                eprintln!(
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
        gen.snapshot.write_files(gen.dir())?;
        ev.bind(gen.id, Some(gen.dir())).await?;
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
        eprintln!(
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
            eprintln!(
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
                QueryKey::Access,
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
            Err(e) => eprintln!("web: spare nix repl not started: {e:#}"),
        }
    }

    fn next_serial(&self) -> u64 {
        let mut st = self.st();
        st.next_serial += 1;
        st.next_serial - 1
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::snapshot::Snapshot;
    use super::super::generation::test_generation;
    use super::*;

    /// A snapshot with these content and structure hashes.
    fn snap(content: u64, structure: u64) -> Snapshot {
        Snapshot {
            content,
            structure,
            ..Default::default()
        }
    }

    fn state(latest: Snapshot, serving: Option<Snapshot>) -> State {
        let mut st = State::new(Arc::new(latest));
        st.reason = None;
        st.serving = serving.map(|s| Arc::new(test_generation(1, s)));
        st
    }

    fn failure(content: u64, at: Instant) -> Option<Failure> {
        Some(Failure {
            content,
            message: "boom".into(),
            at,
        })
    }

    #[test]
    fn nothing_to_do_when_serving_the_latest_files() {
        let st = state(snap(1, 1), Some(snap(1, 1)));
        assert_eq!(decide(&st, false, Instant::now()), None);
        // A gone repl alone is no reason: pages ask for a restart when they need it.
        assert_eq!(decide(&st, true, Instant::now()), None);
    }

    #[test]
    fn changes_rebind_in_place() {
        let mut st = state(snap(2, 1), Some(snap(1, 1)));
        st.reason = Some("save".into());
        assert_eq!(
            decide(&st, false, Instant::now()),
            Some(Job {
                kind: ChangeKind::Values,
                fresh: false,
                why: "save".into()
            })
        );
        let st = state(snap(2, 2), Some(snap(1, 1)));
        assert_eq!(
            decide(&st, false, Instant::now()),
            Some(Job {
                kind: ChangeKind::Structural,
                fresh: false,
                why: "config files changed".into()
            })
        );
    }

    #[test]
    fn restart_runs_in_a_fresh_repl_even_without_changes() {
        let mut st = state(snap(1, 1), Some(snap(1, 1)));
        st.restart = Some("store repair".into());
        // A recent failure does not hold back an explicit restart.
        st.failed = failure(1, Instant::now());
        assert_eq!(
            decide(&st, false, Instant::now()),
            Some(Job {
                kind: ChangeKind::Values,
                fresh: true,
                why: "store repair".into()
            })
        );
        st.latest = Arc::new(snap(2, 2));
        assert_eq!(
            decide(&st, false, Instant::now()).map(|j| j.kind),
            Some(ChangeKind::Structural)
        );
    }

    #[test]
    fn first_evaluation_and_retry_after_failure() {
        let mut st = state(snap(1, 1), None);
        st.reason = Some("startup".into());
        assert_eq!(
            decide(&st, false, Instant::now()),
            Some(Job {
                kind: ChangeKind::Structural,
                fresh: true,
                why: "startup".into()
            })
        );
        let now = Instant::now();
        st.reason = None;
        st.failed = failure(1, now);
        assert_eq!(decide(&st, false, now), None);
        assert_eq!(
            decide(&st, false, now + RETRY_FAILED_AFTER).map(|j| j.why),
            Some("retry after a failed evaluation".into())
        );
        // Other files than the ones that failed: at once.
        st.latest = Arc::new(snap(2, 1));
        assert!(decide(&st, false, now).is_some());
    }

    #[test]
    fn same_failed_files_wait_unless_the_serving_repl_is_gone() {
        let now = Instant::now();
        let mut st = state(snap(2, 1), Some(snap(1, 1)));
        st.failed = failure(2, now);
        // Recent: wait, gone repl or not.
        assert_eq!(decide(&st, false, now), None);
        assert_eq!(decide(&st, true, now), None);
        // Later: only when the serving repl is gone.
        let later = now + RETRY_FAILED_AFTER;
        assert_eq!(decide(&st, false, later), None);
        assert_eq!(
            decide(&st, true, later),
            Some(Job {
                kind: ChangeKind::Values,
                fresh: true,
                why: "retry: the serving repl is gone".into()
            })
        );
        // A failure of other files does not hold back the latest ones.
        st.failed = failure(3, now);
        assert_eq!(decide(&st, false, now).map(|j| j.fresh), Some(false));
    }
}
