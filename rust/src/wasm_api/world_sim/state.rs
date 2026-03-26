use std::collections::BTreeMap;

use crate::sim::geology_types::GeologyParams;
use crate::sim::world;

use super::types::{DeltaRange, FieldDeltaResponse};

pub(super) const DEFAULT_HISTORY_LIMIT: usize = 512;
pub(super) const HISTORY_SNAPSHOT_INTERVAL: u64 = 32;
pub(super) const DELTA_FULL_THRESHOLD_RATIO: f32 = 0.40;

#[derive(Clone)]
pub(super) struct RangeDelta {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone)]
pub(super) struct F32FieldTracker {
    pub shadow: Vec<f32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(super) struct I32FieldTracker {
    pub shadow: Vec<i32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(super) struct WorldSyncState {
    pub height: F32FieldTracker,
    pub volcanism: F32FieldTracker,
    pub vertex_buoyancy: F32FieldTracker,
    pub river_flux: F32FieldTracker,
    pub river_next: I32FieldTracker,
    pub mantle_heat: F32FieldTracker,
    pub erosion_rate: F32FieldTracker,
    pub deposition_rate: F32FieldTracker,
    pub temperature: F32FieldTracker,
    pub precipitation: F32FieldTracker,
    pub evapotranspiration: F32FieldTracker,
    pub aridity: F32FieldTracker,
    pub runoff: F32FieldTracker,
    pub ocean_temperature: F32FieldTracker,
    pub river_transport_cost: F32FieldTracker,
}

#[derive(Clone)]
pub(super) struct ManagedWorld {
    pub world: world::World,
    pub simulation_rate: f32,
    pub geology_params: GeologyParams,
    pub sync_state: WorldSyncState,
    pub history: BTreeMap<u64, world::World>,
}

impl F32FieldTracker {
    pub fn new(values: &[f32]) -> Self {
        Self {
            shadow: values.to_vec(),
            dirty_ranges: Vec::new(),
            force_full: false,
        }
    }

    pub fn observe(&mut self, values: &[f32]) {
        if self.shadow.len() != values.len() {
            self.shadow = values.to_vec();
            self.force_full = true;
            self.dirty_ranges.clear();
            return;
        }

        let mut changed = 0usize;
        let mut range_start: Option<usize> = None;
        for (index, value) in values.iter().copied().enumerate() {
            if self.shadow[index] == value {
                if let Some(start) = range_start.take() {
                    self.merge_dirty_range(start, index);
                }
                continue;
            }
            self.shadow[index] = value;
            changed += 1;
            if range_start.is_none() {
                range_start = Some(index);
            }
        }
        if let Some(start) = range_start {
            self.merge_dirty_range(start, values.len());
        }
        if changed > 0 && (changed as f32) >= (values.len() as f32) * DELTA_FULL_THRESHOLD_RATIO {
            self.force_full = true;
            self.dirty_ranges.clear();
        }
    }

    pub fn take_delta(&mut self, field_kind: &str) -> Option<FieldDeltaResponse> {
        if self.force_full {
            self.force_full = false;
            self.dirty_ranges.clear();
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "full".to_string(),
                ranges: vec![DeltaRange {
                    start: 0,
                    end: self.shadow.len() as u32,
                }],
                f32_data: Some(self.shadow.clone()),
                i32_data: None,
            });
        }
        if self.dirty_ranges.is_empty() {
            return None;
        }

        let ranges = self
            .dirty_ranges
            .iter()
            .map(|range| DeltaRange {
                start: range.start,
                end: range.end,
            })
            .collect::<Vec<_>>();
        let mut values = Vec::new();
        for range in self.dirty_ranges.drain(..) {
            values.extend_from_slice(&self.shadow[range.start as usize..range.end as usize]);
        }
        Some(FieldDeltaResponse {
            field_kind: field_kind.to_string(),
            mode: "delta".to_string(),
            ranges,
            f32_data: Some(values),
            i32_data: None,
        })
    }

    pub fn discard_pending(&mut self) {
        self.force_full = false;
        self.dirty_ranges.clear();
    }

    fn merge_dirty_range(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let next = RangeDelta {
            start: start as u32,
            end: end as u32,
        };
        if let Some(last) = self.dirty_ranges.last_mut() {
            if next.start <= last.end {
                last.end = last.end.max(next.end);
                return;
            }
        }
        self.dirty_ranges.push(next);
    }
}

impl I32FieldTracker {
    pub fn new(values: &[i32]) -> Self {
        Self {
            shadow: values.to_vec(),
            dirty_ranges: Vec::new(),
            force_full: false,
        }
    }

    pub fn observe(&mut self, values: &[i32]) {
        if self.shadow.len() != values.len() {
            self.shadow = values.to_vec();
            self.force_full = true;
            self.dirty_ranges.clear();
            return;
        }

        let mut changed = 0usize;
        let mut range_start: Option<usize> = None;
        for (index, value) in values.iter().copied().enumerate() {
            if self.shadow[index] == value {
                if let Some(start) = range_start.take() {
                    self.merge_dirty_range(start, index);
                }
                continue;
            }
            self.shadow[index] = value;
            changed += 1;
            if range_start.is_none() {
                range_start = Some(index);
            }
        }
        if let Some(start) = range_start {
            self.merge_dirty_range(start, values.len());
        }
        if changed > 0 && (changed as f32) >= (values.len() as f32) * DELTA_FULL_THRESHOLD_RATIO {
            self.force_full = true;
            self.dirty_ranges.clear();
        }
    }

    pub fn take_delta(&mut self, field_kind: &str) -> Option<FieldDeltaResponse> {
        if self.force_full {
            self.force_full = false;
            self.dirty_ranges.clear();
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "full".to_string(),
                ranges: vec![DeltaRange {
                    start: 0,
                    end: self.shadow.len() as u32,
                }],
                f32_data: None,
                i32_data: Some(self.shadow.clone()),
            });
        }
        if self.dirty_ranges.is_empty() {
            return None;
        }

        let ranges = self
            .dirty_ranges
            .iter()
            .map(|range| DeltaRange {
                start: range.start,
                end: range.end,
            })
            .collect::<Vec<_>>();
        let mut values = Vec::new();
        for range in self.dirty_ranges.drain(..) {
            values.extend_from_slice(&self.shadow[range.start as usize..range.end as usize]);
        }
        Some(FieldDeltaResponse {
            field_kind: field_kind.to_string(),
            mode: "delta".to_string(),
            ranges,
            f32_data: None,
            i32_data: Some(values),
        })
    }

    pub fn discard_pending(&mut self) {
        self.force_full = false;
        self.dirty_ranges.clear();
    }

    fn merge_dirty_range(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let next = RangeDelta {
            start: start as u32,
            end: end as u32,
        };
        if let Some(last) = self.dirty_ranges.last_mut() {
            if next.start <= last.end {
                last.end = last.end.max(next.end);
                return;
            }
        }
        self.dirty_ranges.push(next);
    }
}

impl WorldSyncState {
    pub fn from_world(world: &world::World) -> Self {
        let mantle_heat = world
            .runtime
            .geology_dynamics
            .as_ref()
            .map(|dynamics| dynamics.mantle_heat.clone())
            .filter(|values| values.len() == world.state.geology.height.len())
            .unwrap_or_else(|| vec![0.5; world.state.geology.height.len()]);
        Self {
            height: F32FieldTracker::new(&world.state.geology.height),
            volcanism: F32FieldTracker::new(&world.state.geology.volcanism),
            vertex_buoyancy: F32FieldTracker::new(&world.state.geology.vertex_buoyancy),
            river_flux: F32FieldTracker::new(&world.state.hydrology.river_flow),
            river_next: I32FieldTracker::new(&world.state.hydrology.river_next),
            mantle_heat: F32FieldTracker::new(&mantle_heat),
            erosion_rate: F32FieldTracker::new(&world.state.geology.erosion_rate),
            deposition_rate: F32FieldTracker::new(&world.state.geology.deposition_rate),
            temperature: F32FieldTracker::new(&world.state.climate.temperature),
            precipitation: F32FieldTracker::new(&world.state.climate.precipitation),
            evapotranspiration: F32FieldTracker::new(&world.state.climate.evapotranspiration),
            aridity: F32FieldTracker::new(&world.state.climate.aridity),
            runoff: F32FieldTracker::new(&world.state.climate.runoff),
            ocean_temperature: F32FieldTracker::new(&world.state.climate.ocean_temperature),
            river_transport_cost: F32FieldTracker::new(&world.state.hydrology.river_transport_cost),
        }
    }

    pub fn observe_world(&mut self, world: &world::World) {
        self.height.observe(&world.state.geology.height);
        self.volcanism.observe(&world.state.geology.volcanism);
        self.vertex_buoyancy
            .observe(&world.state.geology.vertex_buoyancy);
        self.river_flux.observe(&world.state.hydrology.river_flow);
        self.river_next.observe(&world.state.hydrology.river_next);

        let mantle_heat = world
            .runtime
            .geology_dynamics
            .as_ref()
            .map(|dynamics| dynamics.mantle_heat.as_slice())
            .filter(|values| values.len() == world.state.geology.height.len());
        if let Some(values) = mantle_heat {
            self.mantle_heat.observe(values);
        } else {
            let fallback = vec![0.5; world.state.geology.height.len()];
            self.mantle_heat.observe(&fallback);
        }

        self.temperature.observe(&world.state.climate.temperature);
        self.precipitation
            .observe(&world.state.climate.precipitation);
        self.erosion_rate.observe(&world.state.geology.erosion_rate);
        self.deposition_rate
            .observe(&world.state.geology.deposition_rate);
        self.evapotranspiration
            .observe(&world.state.climate.evapotranspiration);
        self.aridity.observe(&world.state.climate.aridity);
        self.runoff.observe(&world.state.climate.runoff);
        self.ocean_temperature
            .observe(&world.state.climate.ocean_temperature);
        self.river_transport_cost
            .observe(&world.state.hydrology.river_transport_cost);
    }

    pub fn take_world_field_deltas<F>(&mut self, mut include_field: F) -> Vec<FieldDeltaResponse>
    where
        F: FnMut(&str) -> bool,
    {
        let mut deltas = Vec::new();
        if include_field("height") {
            if let Some(delta) = self.height.take_delta("height") {
                deltas.push(delta);
            }
        } else {
            self.height.discard_pending();
        }
        if include_field("volcanism") {
            if let Some(delta) = self.volcanism.take_delta("volcanism") {
                deltas.push(delta);
            }
        } else {
            self.volcanism.discard_pending();
        }
        if include_field("vertex_buoyancy") {
            if let Some(delta) = self.vertex_buoyancy.take_delta("vertex_buoyancy") {
                deltas.push(delta);
            }
        } else {
            self.vertex_buoyancy.discard_pending();
        }
        if include_field("river_flux") {
            if let Some(delta) = self.river_flux.take_delta("river_flux") {
                deltas.push(delta);
            }
        } else {
            self.river_flux.discard_pending();
        }
        if include_field("river_next") {
            if let Some(delta) = self.river_next.take_delta("river_next") {
                deltas.push(delta);
            }
        } else {
            self.river_next.discard_pending();
        }
        if include_field("mantle_heat") {
            if let Some(delta) = self.mantle_heat.take_delta("mantle_heat") {
                deltas.push(delta);
            }
        } else {
            self.mantle_heat.discard_pending();
        }
        if include_field("erosion_rate") {
            if let Some(delta) = self.erosion_rate.take_delta("erosion_rate") {
                deltas.push(delta);
            }
        } else {
            self.erosion_rate.discard_pending();
        }
        if include_field("deposition_rate") {
            if let Some(delta) = self.deposition_rate.take_delta("deposition_rate") {
                deltas.push(delta);
            }
        } else {
            self.deposition_rate.discard_pending();
        }
        if include_field("temperature") {
            if let Some(delta) = self.temperature.take_delta("temperature") {
                deltas.push(delta);
            }
        } else {
            self.temperature.discard_pending();
        }
        if include_field("precipitation") {
            if let Some(delta) = self.precipitation.take_delta("precipitation") {
                deltas.push(delta);
            }
        } else {
            self.precipitation.discard_pending();
        }
        if include_field("evapotranspiration") {
            if let Some(delta) = self.evapotranspiration.take_delta("evapotranspiration") {
                deltas.push(delta);
            }
        } else {
            self.evapotranspiration.discard_pending();
        }
        if include_field("aridity") {
            if let Some(delta) = self.aridity.take_delta("aridity") {
                deltas.push(delta);
            }
        } else {
            self.aridity.discard_pending();
        }
        if include_field("runoff") {
            if let Some(delta) = self.runoff.take_delta("runoff") {
                deltas.push(delta);
            }
        } else {
            self.runoff.discard_pending();
        }
        if include_field("ocean_temperature") {
            if let Some(delta) = self.ocean_temperature.take_delta("ocean_temperature") {
                deltas.push(delta);
            }
        } else {
            self.ocean_temperature.discard_pending();
        }
        if include_field("river_transport_cost") {
            if let Some(delta) = self.river_transport_cost.take_delta("river_transport_cost") {
                deltas.push(delta);
            }
        } else {
            self.river_transport_cost.discard_pending();
        }
        deltas
    }
}

impl ManagedWorld {
    pub fn observe_after_world_change(&mut self) {
        self.sync_state.observe_world(&self.world);
    }

    pub fn save_history_snapshot_if_needed(&mut self) {
        if self.world.clock.tick % HISTORY_SNAPSHOT_INTERVAL != 0 {
            return;
        }
        self.world
            .archive
            .history_ticks
            .insert(self.world.clock.tick, "auto".to_string());
        self.history
            .insert(self.world.clock.tick, self.world.clone());
        while self.history.len() > DEFAULT_HISTORY_LIMIT {
            if let Some(oldest) = self.history.keys().next().copied() {
                self.history.remove(&oldest);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{F32FieldTracker, I32FieldTracker, WorldSyncState};

    #[test]
    fn f32_tracker_collects_delta_ranges_once() {
        let mut tracker = F32FieldTracker::new(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
        tracker.observe(&[1.0, 7.0, 3.0, 4.0, 5.0, 6.0, 2.0, 8.0, 9.0]);

        let delta = tracker.take_delta("height").expect("delta");
        assert_eq!(delta.mode, "delta");
        assert_eq!(delta.ranges.len(), 2);
        assert_eq!(delta.f32_data.expect("f32 data"), vec![7.0, 2.0]);
        assert!(tracker.take_delta("height").is_none());
    }

    #[test]
    fn i32_tracker_forces_full_when_many_values_change() {
        let mut tracker = I32FieldTracker::new(&[1, 2, 3, 4]);
        tracker.observe(&[9, 8, 7, 4]);

        let delta = tracker.take_delta("river_next").expect("full delta");
        assert_eq!(delta.mode, "full");
        assert_eq!(delta.ranges.len(), 1);
        assert_eq!(delta.i32_data.expect("i32 data"), vec![9, 8, 7, 4]);
    }

    #[test]
    fn world_sync_state_discards_pending_for_excluded_fields() {
        let mut state = WorldSyncState {
            height: F32FieldTracker::new(&[1.0, 1.0]),
            volcanism: F32FieldTracker::new(&[0.0, 0.0]),
            vertex_buoyancy: F32FieldTracker::new(&[0.0, 0.0]),
            river_flux: F32FieldTracker::new(&[0.0, 0.0]),
            river_next: I32FieldTracker::new(&[-1, -1]),
            mantle_heat: F32FieldTracker::new(&[0.5, 0.5]),
            erosion_rate: F32FieldTracker::new(&[0.0, 0.0]),
            deposition_rate: F32FieldTracker::new(&[0.0, 0.0]),
            temperature: F32FieldTracker::new(&[10.0, 10.0]),
            precipitation: F32FieldTracker::new(&[100.0, 100.0]),
            evapotranspiration: F32FieldTracker::new(&[50.0, 50.0]),
            aridity: F32FieldTracker::new(&[1.0, 1.0]),
            runoff: F32FieldTracker::new(&[30.0, 30.0]),
            ocean_temperature: F32FieldTracker::new(&[10.0, 10.0]),
            river_transport_cost: F32FieldTracker::new(&[0.2, 0.2]),
        };

        state.height.observe(&[2.0, 1.0]);
        state.temperature.observe(&[11.0, 10.0]);

        let deltas = state.take_world_field_deltas(|field_kind| field_kind == "height");
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].field_kind, "height");

        let no_pending_temperature =
            state.take_world_field_deltas(|field_kind| field_kind == "temperature");
        assert!(no_pending_temperature.is_empty());
    }
}
