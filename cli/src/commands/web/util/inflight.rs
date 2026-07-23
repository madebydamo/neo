use std::collections::HashSet;
use std::sync::Mutex;

/// Thread-safe set of in-flight operation keys (container pulls, clear-appdata, …).
#[derive(Debug, Default)]
pub struct InFlightSet {
    set: Mutex<HashSet<String>>,
}

impl InFlightSet {
    pub fn new() -> Self {
        Self {
            set: Mutex::new(HashSet::new()),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.set.lock().map(|s| s.contains(key)).unwrap_or(false)
    }

    /// Insert `key`. Returns false if already present.
    pub fn try_begin(&self, key: &str) -> bool {
        match self.set.lock() {
            Ok(mut s) => {
                if s.contains(key) {
                    false
                } else {
                    s.insert(key.to_string());
                    true
                }
            }
            Err(_) => false,
        }
    }

    pub fn end(&self, key: &str) {
        if let Ok(mut s) = self.set.lock() {
            s.remove(key);
        }
    }
}
