use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub(crate) struct ScriptRegistry {
    limit: Duration,
    scripts: HashMap<Uuid, (String, Instant)>,
}

/// Defines a local "registry" for scripts where
/// they can be stored and retrieved
impl ScriptRegistry {
    pub(crate) fn new(limit: Duration) -> Self {
        Self {
            limit,
            scripts: HashMap::new(),
        }
    }

    /// Gets a script's contents, incrementing its
    /// last accessed counter if found. An owned
    /// copy is returned given the narrow use
    /// case.
    pub(crate) fn get(&mut self, id: &Uuid) -> Option<String> {
        let now = Instant::now();

        if let Some(o) = self.scripts.get_mut(id) {
            o.1 = now;
        }

        self.scripts.get(id).map(|(s, _)| s.to_string())
    }

    /// Removes a script given its id
    pub(crate) fn remove(&mut self, id: &Uuid) {
        self.scripts.remove(id);
    }

    /// Stores a script, evicting any that haven't been used in a
    /// specified amount of time.
    pub(crate) fn store(&mut self, script: String) -> Uuid {
        let id = Uuid::new_v4();
        let now = Instant::now();

        let limit = self.limit;
        self.scripts
            .retain(|_, &mut (_, last_accessed)| now.duration_since(last_accessed) <= limit);

        self.scripts.insert(id, (script, now));

        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_registry_get_none() {
        let mut registry = ScriptRegistry::new(Duration::from_millis(0));

        assert!(registry
            .get(&Uuid::parse_str("50b0cb8f-1f59-4ba5-8935-ba54bb64bc3f").unwrap())
            .is_none());

        let script = "function() { return 3 + 4; }";

        let id = registry.store(script.to_string());

        assert_eq!(registry.get(&id), Some(script.to_string()));
    }

    #[test]
    fn test_script_registry_store_and_get() {
        let mut registry = ScriptRegistry::new(Duration::from_millis(0));

        let script = "function() { return 3 + 4; }";

        let id = registry.store(script.to_string());

        assert_eq!(registry.get(&id), Some(script.to_string()));
    }

    #[test]
    fn test_script_registry_store_and_remove() {
        let mut registry = ScriptRegistry::new(Duration::from_millis(60000));

        let script = "function() { return 3 + 4; }";

        let id = registry.store(script.to_string());

        registry.remove(&id);

        assert_eq!(registry.get(&id), None);
    }

    #[test]
    fn test_script_registry_evicts_old_entries() {
        let mut registry = ScriptRegistry::new(Duration::from_millis(1));

        let script = "function() { return 3 + 4; }";

        let id = registry.store(script.to_string());

        // Entries are lazily evicted, so cause eviction by storing a new one

        std::thread::sleep(Duration::from_millis(50));

        let _ = registry.store(script.to_string());

        // Evicted because of 1ms duration
        assert_eq!(registry.get(&id), None);
    }

    #[test]
    fn test_script_registry_get_extends_eviction() {
        let mut registry = ScriptRegistry::new(Duration::from_millis(10));

        let script = "function() { return 3 + 4; }";

        // Since entries are lazily evicted, wait 100ms so that we know
        // we'd be evicted, get our script, then immediately store one.
        // We expect to be able to get our original script then, since
        // getting it extended the eviction time

        let id = registry.store(script.to_string());

        std::thread::sleep(Duration::from_millis(100));

        let _ = registry.get(&id);

        let _ = registry.store(script.to_string());

        assert_eq!(registry.get(&id), Some(script.to_string()));
    }
}
