#![cfg(feature = "wasm_transport")]

use std::collections::HashMap;

use crate::application::world_runtime::{ManagedWorld, TimelineRuntime};

pub(crate) struct WorldService {
    worlds: HashMap<String, ManagedWorld>,
    timelines: HashMap<String, TimelineRuntime>,
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
            timelines: HashMap::new(),
            next_world_seq: 1,
        }
    }

    pub(crate) fn world(&self, world_id: &str) -> Option<&ManagedWorld> {
        self.worlds.get(world_id)
    }

    #[allow(dead_code)]
    pub(crate) fn world_mut(&mut self, world_id: &str) -> Option<&mut ManagedWorld> {
        self.worlds.get_mut(world_id)
    }

    pub(crate) fn timeline(&self, world_id: &str) -> Option<&TimelineRuntime> {
        self.timelines.get(world_id)
    }

    #[allow(dead_code)]
    pub(crate) fn timeline_mut(&mut self, world_id: &str) -> Option<&mut TimelineRuntime> {
        self.timelines.get_mut(world_id)
    }

    #[allow(dead_code)]
    pub(crate) fn archive(
        &self,
        world_id: &str,
    ) -> Option<&crate::application::world_runtime::TimelineArchive> {
        self.timeline(world_id).map(|timeline| timeline.archive())
    }

    pub(crate) fn world_and_timeline_mut(
        &mut self,
        world_id: &str,
    ) -> Option<(&mut ManagedWorld, &mut TimelineRuntime)> {
        let worlds = &mut self.worlds;
        let timelines = &mut self.timelines;
        let managed = worlds.get_mut(world_id)?;
        let timeline = timelines.get_mut(world_id)?;
        Some((managed, timeline))
    }

    #[allow(dead_code)]
    pub(crate) fn world_and_archive_mut(
        &mut self,
        world_id: &str,
    ) -> Option<(
        &mut ManagedWorld,
        &mut crate::application::world_runtime::TimelineArchive,
    )> {
        let (managed, timeline) = self.world_and_timeline_mut(world_id)?;
        Some((managed, timeline.archive_mut()))
    }

    #[allow(dead_code)]
    pub(crate) fn cloned_world_and_archive(
        &self,
        world_id: &str,
    ) -> Option<(
        ManagedWorld,
        crate::application::world_runtime::TimelineArchive,
    )> {
        let managed = self.worlds.get(world_id)?.clone();
        let archive = self.timelines.get(world_id)?.clone_archive();
        Some((managed, archive))
    }

    pub(crate) fn insert_world(
        &mut self,
        managed: ManagedWorld,
        timeline: TimelineRuntime,
    ) -> String {
        let world_id = self.next_world_id();
        self.timelines.insert(world_id.clone(), timeline);
        self.worlds.insert(world_id.clone(), managed);
        world_id
    }

    fn next_world_id(&mut self) -> String {
        let id = format!("world-{:06}", self.next_world_seq);
        self.next_world_seq = self.next_world_seq.saturating_add(1);
        id
    }
}
