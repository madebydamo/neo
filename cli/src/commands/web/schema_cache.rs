//! Process-local cache of option schemas for helper script resolution.
//! Populated on pane load; helper runs use the cache on hit (no nix evaluator mutex).

use std::collections::HashMap;
use std::time::Instant;

use super::structs::OptionSchema;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct SchemaCacheKey {
    pub is_core: bool,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct CachedSchema {
    pub options: Vec<OptionSchema>,
    pub loaded_at: Instant,
}

#[derive(Default, Debug)]
pub struct SchemaCache {
    entries: HashMap<SchemaCacheKey, CachedSchema>,
}

impl SchemaCache {
    pub fn get(&self, is_core: bool, name: &str) -> Option<Vec<OptionSchema>> {
        let key = SchemaCacheKey {
            is_core,
            name: name.to_string(),
        };
        self.entries.get(&key).map(|e| e.options.clone())
    }

    pub fn put(&mut self, is_core: bool, name: &str, options: Vec<OptionSchema>) {
        let key = SchemaCacheKey {
            is_core,
            name: name.to_string(),
        };
        self.entries.insert(
            key,
            CachedSchema {
                options,
                loaded_at: Instant::now(),
            },
        );
    }

    #[allow(dead_code)]
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    #[allow(dead_code)]
    pub fn invalidate(&mut self, is_core: bool, name: &str) {
        let key = SchemaCacheKey {
            is_core,
            name: name.to_string(),
        };
        self.entries.remove(&key);
    }
}
