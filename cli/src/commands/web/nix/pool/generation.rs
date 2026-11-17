//! One evaluation of the config repo ("generation") and its cached results.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use super::super::snapshot::Snapshot;
use super::query::{Cached, QueryKey};

struct Entry {
    value: Cached,
    used: Instant,
}

/// One evaluation of the config repo and the results read from it.
pub(super) struct Generation {
    pub(super) id: u64,
    pub(super) snapshot: Arc<Snapshot>,
    /// Repl slot that holds `f_<id>`.
    pub(super) slot: usize,
    /// Copy of the settings files this generation read (in-process rebind).
    dir: PathBuf,
    cache: Mutex<HashMap<QueryKey, Entry>>,
}

impl Generation {
    pub(super) fn new(id: u64, snapshot: Arc<Snapshot>, slot: usize, dir: PathBuf) -> Self {
        Self {
            id,
            snapshot,
            slot,
            dir,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Where this generation's copy of the settings files lives.
    pub(super) fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn cache(&self) -> MutexGuard<'_, HashMap<QueryKey, Entry>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn get(&self, key: &QueryKey) -> Option<Cached> {
        let mut c = self.cache();
        let e = c.get_mut(key)?;
        e.used = Instant::now();
        Some(e.value.clone())
    }

    pub(super) fn has(&self, key: &QueryKey) -> bool {
        self.cache().contains_key(key)
    }

    pub(super) fn put(&self, key: QueryKey, value: Cached) {
        self.cache().insert(
            key,
            Entry {
                value,
                used: Instant::now(),
            },
        );
    }

    /// Cached keys, most recently used first.
    pub(super) fn keys_mru(&self) -> Vec<QueryKey> {
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

/// Keys whose result differs between two generations: plainly, and against
/// the older result with the newer settings laid over it.
pub(super) fn diff(old: &Generation, new: &Generation) -> (Vec<String>, Vec<String>) {
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

/// A generation of a snapshot with these settings (tests of the pool).
#[cfg(test)]
pub(super) fn test_generation(id: u64, snapshot: Snapshot) -> Generation {
    Generation::new(
        id,
        Arc::new(snapshot),
        0,
        std::env::temp_dir().join(format!("neo-pool-test-{}-{id}", std::process::id())),
    )
}

#[cfg(test)]
mod tests {
    use super::super::super::extract::error_pane_for;
    use super::*;
    use serde_json::{json, Value};
    use std::time::Duration;

    fn generation(id: u64, settings: Value) -> Generation {
        test_generation(
            id,
            Snapshot {
                settings,
                ..Default::default()
            },
        )
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
