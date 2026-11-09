use std::collections::HashSet;
use std::sync::Mutex;

/// Thread-safe set of in-flight operation keys (container pulls, clear-appdata, …),
/// for display only: operation locks are what refuse a second job.
#[derive(Debug, Default)]
pub struct InFlightSet {
    set: Mutex<HashSet<String>>,
}

impl InFlightSet {
    pub fn contains(&self, key: &str) -> bool {
        self.set.lock().map(|s| s.contains(key)).unwrap_or(false)
    }

    pub fn begin(&self, key: &str) {
        if let Ok(mut s) = self.set.lock() {
            s.insert(key.to_string());
        }
    }

    pub fn end(&self, key: &str) {
        if let Ok(mut s) = self.set.lock() {
            s.remove(key);
        }
    }
}
