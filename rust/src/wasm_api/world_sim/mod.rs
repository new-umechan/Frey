use std::collections::HashMap;

use wasm_bindgen::prelude::*;

mod api;
mod helpers;
mod state;
mod types;

use state::{ManagedWorld, WorldArchive};

#[wasm_bindgen]
pub struct WorldSimController {
    worlds: HashMap<String, ManagedWorld>,
    archives: HashMap<String, WorldArchive>,
    next_world_seq: u64,
}

impl Default for WorldSimController {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WorldSimController {
        WorldSimController {
            worlds: HashMap::new(),
            archives: HashMap::new(),
            next_world_seq: 1,
        }
    }
}

impl WorldSimController {
    fn next_world_id(&mut self) -> String {
        let id = format!("world-{:06}", self.next_world_seq);
        self.next_world_seq = self.next_world_seq.saturating_add(1);
        id
    }
}
