#![cfg(feature = "wasm_transport")]

use std::collections::HashMap;

use crate::application::world_runtime::{ManagedWorld, WorldArchive};

pub(crate) struct WorldService {
    worlds: HashMap<String, ManagedWorld>,
    archives: HashMap<String, WorldArchive>,
    next_world_seq: u64,
}

impl Default for WorldService {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldService {
    pub(crate) fn new() -> Self {
        Self {
            worlds: HashMap::new(),
            archives: HashMap::new(),
            next_world_seq: 1,
        }
    }

    pub(crate) fn world(&self, world_id: &str) -> Option<&ManagedWorld> {
        self.worlds.get(world_id)
    }

    pub(crate) fn world_mut(&mut self, world_id: &str) -> Option<&mut ManagedWorld> {
        self.worlds.get_mut(world_id)
    }

    pub(crate) fn archive(&self, world_id: &str) -> Option<&WorldArchive> {
        self.archives.get(world_id)
    }

    pub(crate) fn world_and_archive_mut(
        &mut self,
        world_id: &str,
    ) -> Option<(&mut ManagedWorld, &mut WorldArchive)> {
        let worlds = &mut self.worlds;
        let archives = &mut self.archives;
        let managed = worlds.get_mut(world_id)?;
        let archive = archives.get_mut(world_id)?;
        Some((managed, archive))
    }

    pub(crate) fn cloned_world_and_archive(
        &self,
        world_id: &str,
    ) -> Option<(ManagedWorld, WorldArchive)> {
        let managed = self.worlds.get(world_id)?.clone();
        let archive = self.archives.get(world_id)?.clone();
        Some((managed, archive))
    }

    pub(crate) fn insert_world(&mut self, managed: ManagedWorld, archive: WorldArchive) -> String {
        let world_id = self.next_world_id();
        self.archives.insert(world_id.clone(), archive);
        self.worlds.insert(world_id.clone(), managed);
        world_id
    }

    fn next_world_id(&mut self) -> String {
        let id = format!("world-{:06}", self.next_world_seq);
        self.next_world_seq = self.next_world_seq.saturating_add(1);
        id
    }
}
