use crate::engine::Engine;
use crate::registry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub home: PathBuf,
    pub engines: Mutex<HashMap<String, Engine>>,
    pub busy: Mutex<std::collections::HashSet<String>>,
}

impl AppState {
    pub fn new() -> Self {
        let home = registry::home();
        let _ = std::fs::create_dir_all(&home);
        Self {
            home,
            engines: Mutex::new(HashMap::new()),
            busy: Mutex::new(std::collections::HashSet::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
