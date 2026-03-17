use std::collections::HashMap;

use wasm_bindgen::prelude::*;

mod api;
mod helpers;
mod state;
mod types;

use state::{ManagedWorld, SnapshotEntry};

#[wasm_bindgen]
pub struct WorldSimController {
    worlds: HashMap<String, ManagedWorld>,
    snapshots: HashMap<String, SnapshotEntry>,
    next_world_seq: u64,
    next_snapshot_seq: u64,
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
            snapshots: HashMap::new(),
            next_world_seq: 1,
            next_snapshot_seq: 1,
        }
    }
}

impl WorldSimController {
    fn next_world_id(&mut self) -> String {
        let id = format!("world-{:06}", self.next_world_seq);
        self.next_world_seq = self.next_world_seq.saturating_add(1);
        id
    }

    fn next_snapshot_id(&mut self) -> String {
        let id = format!("snapshot-{:06}", self.next_snapshot_seq);
        self.next_snapshot_seq = self.next_snapshot_seq.saturating_add(1);
        id
    }
}
