use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::geology_types::GeologyParams;
use crate::sim::world;
use crate::sim::{first_phase, ExecWorldPhase};

use super::types::{DeltaRange, FieldDeltaResponse};

pub(super) const DEFAULT_HISTORY_LIMIT: usize = 512;
pub(super) const HISTORY_SNAPSHOT_INTERVAL: u64 = 64;
pub(super) const DELTA_FULL_THRESHOLD_RATIO: f32 = 0.40;

fn bitmap_word_len(values_len: usize) -> usize {
    values_len.div_ceil(32)
}

fn bitmap_mark(bitmap: &mut [u32], index: usize) {
    let word_index = index / 32;
    let bit_offset = index % 32;
    if let Some(word) = bitmap.get_mut(word_index) {
        *word |= 1u32 << bit_offset;
    }
}

fn bitmap_clear(bitmap: &mut [u32]) {
    bitmap.fill(0);
}

fn bitmap_any(bitmap: &[u32]) -> bool {
    bitmap.iter().any(|word| *word != 0)
}

fn bitmap_count(bitmap: &[u32]) -> usize {
    bitmap.iter().map(|word| word.count_ones() as usize).sum()
}

fn bitmap_indices(bitmap: &[u32], max_len: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(bitmap_count(bitmap));
    for (word_index, mut word) in bitmap.iter().copied().enumerate() {
        while word != 0 {
            let bit = word.trailing_zeros() as usize;
            let index = word_index * 32 + bit;
            if index >= max_len {
                break;
            }
            out.push(index);
            word &= word - 1;
        }
    }
    out
}

fn range_cell_count(ranges: &[RangeDelta], max_len: usize) -> usize {
    ranges
        .iter()
        .map(|range| {
            let start = (range.start as usize).min(max_len);
            let end = (range.end as usize).min(max_len);
            end.saturating_sub(start)
        })
        .sum()
}

fn should_emit_bitmap(bitmap_words: usize, range_count: usize) -> bool {
    range_count > 0 && bitmap_words < range_count.saturating_mul(2)
}

#[derive(Clone)]
pub(super) struct RangeDelta {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone)]
pub(super) struct F32FieldTracker {
    pub shadow: Vec<f32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub dirty_bitmap: Vec<u32>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(super) struct I32FieldTracker {
    pub shadow: Vec<i32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub dirty_bitmap: Vec<u32>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(super) struct U32FieldTracker {
    pub shadow: Vec<u32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub dirty_bitmap: Vec<u32>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(super) struct WorldTransportCache {
    pub height: F32FieldTracker,
    pub lake_depth: F32FieldTracker,
    pub volcanism: F32FieldTracker,
    pub vertex_buoyancy: F32FieldTracker,
    pub plate_id: U32FieldTracker,
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
    pub ice_pressure: F32FieldTracker,
    pub ocean_temperature: F32FieldTracker,
    pub wind_u: F32FieldTracker,
    pub wind_v: F32FieldTracker,
    pub moisture_flux_u: F32FieldTracker,
    pub moisture_flux_v: F32FieldTracker,
    pub river_transport_cost: F32FieldTracker,
}

#[derive(Clone)]
pub(super) struct ManagedWorld {
    pub world: world::World,
    pub hydrology_dynamics: Option<ErosionAutomatonState>,
    pub geology_dynamics: Option<world::GeologyDynamicsState>,
    pub feedback: world::FeedbackQueue,
    pub simulation_rate: f32,
    pub geology_params: GeologyParams,
    pub transport_cache: WorldTransportCache,
    pub exec_state: ManagedWorldExecState,
    pub applied_intervention_seq: u64,
}

#[derive(Clone)]
pub(super) struct WorldHistorySnapshot {
    pub core: world::WorldCore,
    pub hydrology_dynamics: Option<ErosionAutomatonState>,
    pub geology_dynamics: Option<world::GeologyDynamicsState>,
    pub applied_intervention_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) enum InterventionCommand {
    SetSimulationRate { value: f32 },
    SetTargetSeaRatio { value: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct InterventionEvent {
    pub tick: u64,
    pub sequence: u64,
    pub command: InterventionCommand,
}

#[derive(Clone)]
pub(super) struct WorldArchive {
    pub history: BTreeMap<u64, WorldHistorySnapshot>,
    pub interventions: Vec<InterventionEvent>,
    pub next_intervention_seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManagedWorldExecState {
    pub next_phase: ExecWorldPhase,
    pub remaining_steps: u32,
    pub pending_post_step: bool,
}

impl Default for ManagedWorldExecState {
    fn default() -> Self {
        Self {
            next_phase: first_phase(),
            remaining_steps: 0,
            pending_post_step: false,
        }
    }
}

impl F32FieldTracker {
    pub fn new(values: &[f32]) -> Self {
        Self {
            shadow: values.to_vec(),
            dirty_ranges: Vec::new(),
            dirty_bitmap: vec![0; bitmap_word_len(values.len())],
            force_full: false,
        }
    }

    pub fn observe(&mut self, values: &[f32]) {
        if self.shadow.len() != values.len() {
            self.shadow = values.to_vec();
            self.force_full = true;
            self.dirty_ranges.clear();
            self.dirty_bitmap = vec![0; bitmap_word_len(values.len())];
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
            bitmap_mark(&mut self.dirty_bitmap, index);
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
            bitmap_clear(&mut self.dirty_bitmap);
        }
    }

    pub fn take_delta(&mut self, field_kind: &str) -> Option<FieldDeltaResponse> {
        if self.force_full {
            self.force_full = false;
            self.dirty_ranges.clear();
            bitmap_clear(&mut self.dirty_bitmap);
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "full".to_string(),
                ranges: vec![DeltaRange {
                    start: 0,
                    end: self.shadow.len() as u32,
                }],
                dirty_bitmap: None,
                f32_data: Some(self.shadow.clone()),
                u32_data: None,
                i32_data: None,
            });
        }
        if self.dirty_ranges.is_empty() || !bitmap_any(&self.dirty_bitmap) {
            return None;
        }

        let changed_cells = bitmap_count(&self.dirty_bitmap);
        let range_cells = range_cell_count(&self.dirty_ranges, self.shadow.len());
        let use_bitmap = changed_cells > 0
            && changed_cells < self.shadow.len()
            && should_emit_bitmap(self.dirty_bitmap.len(), self.dirty_ranges.len())
            && range_cells >= changed_cells;
        if use_bitmap {
            let indices = bitmap_indices(&self.dirty_bitmap, self.shadow.len());
            let values = indices
                .iter()
                .map(|index| self.shadow[*index])
                .collect::<Vec<_>>();
            let bitmap = self.dirty_bitmap.clone();
            self.dirty_ranges.clear();
            bitmap_clear(&mut self.dirty_bitmap);
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "bitmap".to_string(),
                ranges: Vec::new(),
                dirty_bitmap: Some(bitmap),
                f32_data: Some(values),
                u32_data: None,
                i32_data: None,
            });
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
        bitmap_clear(&mut self.dirty_bitmap);
        Some(FieldDeltaResponse {
            field_kind: field_kind.to_string(),
            mode: "delta".to_string(),
            ranges,
            dirty_bitmap: None,
            f32_data: Some(values),
            u32_data: None,
            i32_data: None,
        })
    }

    pub fn discard_pending(&mut self) {
        self.force_full = false;
        self.dirty_ranges.clear();
        bitmap_clear(&mut self.dirty_bitmap);
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
            dirty_bitmap: vec![0; bitmap_word_len(values.len())],
            force_full: false,
        }
    }

    pub fn observe(&mut self, values: &[i32]) {
        if self.shadow.len() != values.len() {
            self.shadow = values.to_vec();
            self.force_full = true;
            self.dirty_ranges.clear();
            self.dirty_bitmap = vec![0; bitmap_word_len(values.len())];
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
            bitmap_mark(&mut self.dirty_bitmap, index);
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
            bitmap_clear(&mut self.dirty_bitmap);
        }
    }

    pub fn take_delta(&mut self, field_kind: &str) -> Option<FieldDeltaResponse> {
        if self.force_full {
            self.force_full = false;
            self.dirty_ranges.clear();
            bitmap_clear(&mut self.dirty_bitmap);
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "full".to_string(),
                ranges: vec![DeltaRange {
                    start: 0,
                    end: self.shadow.len() as u32,
                }],
                dirty_bitmap: None,
                f32_data: None,
                u32_data: None,
                i32_data: Some(self.shadow.clone()),
            });
        }
        if self.dirty_ranges.is_empty() || !bitmap_any(&self.dirty_bitmap) {
            return None;
        }

        let changed_cells = bitmap_count(&self.dirty_bitmap);
        let range_cells = range_cell_count(&self.dirty_ranges, self.shadow.len());
        let use_bitmap = changed_cells > 0
            && changed_cells < self.shadow.len()
            && should_emit_bitmap(self.dirty_bitmap.len(), self.dirty_ranges.len())
            && range_cells >= changed_cells;
        if use_bitmap {
            let indices = bitmap_indices(&self.dirty_bitmap, self.shadow.len());
            let values = indices
                .iter()
                .map(|index| self.shadow[*index])
                .collect::<Vec<_>>();
            let bitmap = self.dirty_bitmap.clone();
            self.dirty_ranges.clear();
            bitmap_clear(&mut self.dirty_bitmap);
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "bitmap".to_string(),
                ranges: Vec::new(),
                dirty_bitmap: Some(bitmap),
                f32_data: None,
                u32_data: None,
                i32_data: Some(values),
            });
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
        bitmap_clear(&mut self.dirty_bitmap);
        Some(FieldDeltaResponse {
            field_kind: field_kind.to_string(),
            mode: "delta".to_string(),
            ranges,
            dirty_bitmap: None,
            f32_data: None,
            u32_data: None,
            i32_data: Some(values),
        })
    }

    pub fn discard_pending(&mut self) {
        self.force_full = false;
        self.dirty_ranges.clear();
        bitmap_clear(&mut self.dirty_bitmap);
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

impl U32FieldTracker {
    pub fn new(values: &[u32]) -> Self {
        Self {
            shadow: values.to_vec(),
            dirty_ranges: Vec::new(),
            dirty_bitmap: vec![0; bitmap_word_len(values.len())],
            force_full: false,
        }
    }

    pub fn observe(&mut self, values: &[u32]) {
        if self.shadow.len() != values.len() {
            self.shadow = values.to_vec();
            self.force_full = true;
            self.dirty_ranges.clear();
            self.dirty_bitmap = vec![0; bitmap_word_len(values.len())];
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
            bitmap_mark(&mut self.dirty_bitmap, index);
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
            bitmap_clear(&mut self.dirty_bitmap);
        }
    }

    pub fn take_delta(&mut self, field_kind: &str) -> Option<FieldDeltaResponse> {
        if self.force_full {
            self.force_full = false;
            self.dirty_ranges.clear();
            bitmap_clear(&mut self.dirty_bitmap);
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "full".to_string(),
                ranges: vec![DeltaRange {
                    start: 0,
                    end: self.shadow.len() as u32,
                }],
                dirty_bitmap: None,
                f32_data: None,
                u32_data: Some(self.shadow.clone()),
                i32_data: None,
            });
        }
        if self.dirty_ranges.is_empty() || !bitmap_any(&self.dirty_bitmap) {
            return None;
        }

        let changed_cells = bitmap_count(&self.dirty_bitmap);
        let range_cells = range_cell_count(&self.dirty_ranges, self.shadow.len());
        let use_bitmap = changed_cells > 0
            && changed_cells < self.shadow.len()
            && should_emit_bitmap(self.dirty_bitmap.len(), self.dirty_ranges.len())
            && range_cells >= changed_cells;
        if use_bitmap {
            let indices = bitmap_indices(&self.dirty_bitmap, self.shadow.len());
            let values = indices
                .iter()
                .map(|index| self.shadow[*index])
                .collect::<Vec<_>>();
            let bitmap = self.dirty_bitmap.clone();
            self.dirty_ranges.clear();
            bitmap_clear(&mut self.dirty_bitmap);
            return Some(FieldDeltaResponse {
                field_kind: field_kind.to_string(),
                mode: "bitmap".to_string(),
                ranges: Vec::new(),
                dirty_bitmap: Some(bitmap),
                f32_data: None,
                u32_data: Some(values),
                i32_data: None,
            });
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
        bitmap_clear(&mut self.dirty_bitmap);
        Some(FieldDeltaResponse {
            field_kind: field_kind.to_string(),
            mode: "delta".to_string(),
            ranges,
            dirty_bitmap: None,
            f32_data: None,
            u32_data: Some(values),
            i32_data: None,
        })
    }

    pub fn discard_pending(&mut self) {
        self.force_full = false;
        self.dirty_ranges.clear();
        bitmap_clear(&mut self.dirty_bitmap);
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

fn collect_plate_ids(world: &world::World) -> Vec<u32> {
    world
        .state
        .geology
        .plate_id
        .iter()
        .map(|plate_id| plate_id.as_u32())
        .collect()
}

impl WorldTransportCache {
    pub fn from_world(
        world: &world::World,
        geology_dynamics: Option<&world::GeologyDynamicsState>,
    ) -> Self {
        let mantle_heat = geology_dynamics
            .map(|dynamics| dynamics.mantle_heat.clone())
            .filter(|values| values.len() == world.state.geology.height.len())
            .unwrap_or_else(|| vec![0.5; world.state.geology.height.len()]);
        let ice_pressure =
            if world.state.glaciology.ice_load.len() == world.state.geology.height.len() {
                world.state.glaciology.ice_load.clone()
            } else {
                vec![0.0; world.state.geology.height.len()]
            };
        Self {
            height: F32FieldTracker::new(&world.state.geology.height),
            lake_depth: F32FieldTracker::new(&world.state.geology.lake_depth),
            volcanism: F32FieldTracker::new(&world.state.geology.volcanism),
            vertex_buoyancy: F32FieldTracker::new(&world.state.geology.vertex_buoyancy),
            plate_id: U32FieldTracker::new(&collect_plate_ids(world)),
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
            ice_pressure: F32FieldTracker::new(&ice_pressure),
            ocean_temperature: F32FieldTracker::new(&world.state.climate.ocean_temperature),
            wind_u: F32FieldTracker::new(&world.state.climate.wind_u),
            wind_v: F32FieldTracker::new(&world.state.climate.wind_v),
            moisture_flux_u: F32FieldTracker::new(&world.state.climate.moisture_flux_u),
            moisture_flux_v: F32FieldTracker::new(&world.state.climate.moisture_flux_v),
            river_transport_cost: F32FieldTracker::new(&world.state.hydrology.river_transport_cost),
        }
    }

    pub fn observe_world(
        &mut self,
        world: &world::World,
        geology_dynamics: Option<&world::GeologyDynamicsState>,
    ) {
        self.height.observe(&world.state.geology.height);
        self.lake_depth.observe(&world.state.geology.lake_depth);
        self.volcanism.observe(&world.state.geology.volcanism);
        self.vertex_buoyancy
            .observe(&world.state.geology.vertex_buoyancy);
        self.plate_id.observe(&collect_plate_ids(world));
        self.river_flux.observe(&world.state.hydrology.river_flow);
        self.river_next.observe(&world.state.hydrology.river_next);

        let mantle_heat = geology_dynamics
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
        if world.state.glaciology.ice_load.len() == world.state.geology.height.len() {
            self.ice_pressure.observe(&world.state.glaciology.ice_load);
        } else {
            let fallback = vec![0.0; world.state.geology.height.len()];
            self.ice_pressure.observe(&fallback);
        }
        self.ocean_temperature
            .observe(&world.state.climate.ocean_temperature);
        self.wind_u.observe(&world.state.climate.wind_u);
        self.wind_v.observe(&world.state.climate.wind_v);
        self.moisture_flux_u
            .observe(&world.state.climate.moisture_flux_u);
        self.moisture_flux_v
            .observe(&world.state.climate.moisture_flux_v);
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
        if include_field("lake_depth") {
            if let Some(delta) = self.lake_depth.take_delta("lake_depth") {
                deltas.push(delta);
            }
        } else {
            self.lake_depth.discard_pending();
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
        if include_field("plate_id") {
            if let Some(delta) = self.plate_id.take_delta("plate_id") {
                deltas.push(delta);
            }
        } else {
            self.plate_id.discard_pending();
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
        if include_field("ice_pressure") {
            if let Some(delta) = self.ice_pressure.take_delta("ice_pressure") {
                deltas.push(delta);
            }
        } else {
            self.ice_pressure.discard_pending();
        }
        if include_field("ocean_temperature") {
            if let Some(delta) = self.ocean_temperature.take_delta("ocean_temperature") {
                deltas.push(delta);
            }
        } else {
            self.ocean_temperature.discard_pending();
        }
        if include_field("wind_u") {
            if let Some(delta) = self.wind_u.take_delta("wind_u") {
                deltas.push(delta);
            }
        } else {
            self.wind_u.discard_pending();
        }
        if include_field("wind_v") {
            if let Some(delta) = self.wind_v.take_delta("wind_v") {
                deltas.push(delta);
            }
        } else {
            self.wind_v.discard_pending();
        }
        if include_field("moisture_flux_u") {
            if let Some(delta) = self.moisture_flux_u.take_delta("moisture_flux_u") {
                deltas.push(delta);
            }
        } else {
            self.moisture_flux_u.discard_pending();
        }
        if include_field("moisture_flux_v") {
            if let Some(delta) = self.moisture_flux_v.take_delta("moisture_flux_v") {
                deltas.push(delta);
            }
        } else {
            self.moisture_flux_v.discard_pending();
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
    pub fn matched_geology_dynamics(&self) -> Option<&world::GeologyDynamicsState> {
        self.geology_dynamics
            .as_ref()
            .filter(|state| state.vertex_states.len() == self.world.state.geology.height.len())
    }

    pub fn matched_hydrology_dynamics(&self) -> Option<&ErosionAutomatonState> {
        self.hydrology_dynamics
            .as_ref()
            .filter(|state| state.sink_id.len() == self.world.state.geology.height.len())
    }

    pub fn with_exec_states<R>(
        &mut self,
        run: impl FnOnce(
            &mut world::World,
            &mut world::FeedbackQueue,
            &mut Option<world::GeologyDynamicsState>,
            &mut Option<ErosionAutomatonState>,
        ) -> R,
    ) -> R {
        run(
            &mut self.world,
            &mut self.feedback,
            &mut self.geology_dynamics,
            &mut self.hydrology_dynamics,
        )
    }

    pub fn snapshot_world(&self) -> WorldHistorySnapshot {
        WorldHistorySnapshot {
            core: self.world.core_owned(),
            hydrology_dynamics: self.hydrology_dynamics.clone(),
            geology_dynamics: self.geology_dynamics.clone(),
            applied_intervention_seq: self.applied_intervention_seq,
        }
    }

    pub fn reset_exec_state(&mut self) {
        self.exec_state = ManagedWorldExecState::default();
    }

    pub fn observe_after_world_change(&mut self) {
        self.transport_cache
            .observe_world(&self.world, self.geology_dynamics.as_ref());
    }

    pub fn exec_is_busy(&self) -> bool {
        self.exec_state.pending_post_step || self.exec_state.remaining_steps > 0
    }
}

impl WorldArchive {
    pub fn new() -> Self {
        Self {
            history: BTreeMap::new(),
            interventions: Vec::new(),
            next_intervention_seq: 0,
        }
    }

    pub fn insert_snapshot(&mut self, tick: u64, snapshot: WorldHistorySnapshot) {
        self.history.insert(tick, snapshot);
    }

    pub fn save_snapshot_if_needed(&mut self, managed: &ManagedWorld) {
        if !managed
            .world
            .clock
            .tick
            .is_multiple_of(HISTORY_SNAPSHOT_INTERVAL)
        {
            return;
        }
        self.insert_snapshot(managed.world.clock.tick, managed.snapshot_world());
        while self.history.len() > DEFAULT_HISTORY_LIMIT {
            if let Some(oldest) = self.history.keys().next().copied() {
                self.history.remove(&oldest);
            } else {
                break;
            }
        }
    }

    pub fn enqueue_intervention(
        &mut self,
        managed: &mut ManagedWorld,
        command: InterventionCommand,
    ) -> InterventionEvent {
        let event = InterventionEvent {
            tick: managed.world.clock.tick,
            sequence: self.next_intervention_seq,
            command,
        };
        self.next_intervention_seq = self.next_intervention_seq.saturating_add(1);
        self.apply_event(managed, &event);
        self.interventions.push(event.clone());
        event
    }

    pub fn apply_pending_interventions_for_tick(&self, managed: &mut ManagedWorld, tick: u64) {
        for event in self.interventions.iter().filter(|entry| entry.tick == tick) {
            if event.sequence < managed.applied_intervention_seq {
                continue;
            }
            self.apply_event(managed, event);
        }
    }

    fn apply_event(&self, managed: &mut ManagedWorld, event: &InterventionEvent) {
        match event.command {
            InterventionCommand::SetSimulationRate { value } => {
                managed.simulation_rate = value.clamp(0.1, 32.0);
            }
            InterventionCommand::SetTargetSeaRatio { value } => {
                managed.world.control.target_sea_ratio = value.clamp(0.02, 0.98);
            }
        }
        managed.applied_intervention_seq = event.sequence.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        F32FieldTracker, I32FieldTracker, ManagedWorld, ManagedWorldExecState, U32FieldTracker,
        WorldTransportCache,
    };
    use crate::sim::geology_types::{GeologyInternal, PlateId};
    use crate::sim::world;

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
    fn u32_tracker_collects_plate_id_delta_ranges() {
        let mut tracker = U32FieldTracker::new(&[0, 1, 2, 3, 4, 5]);
        tracker.observe(&[0, 8, 2, 3, 9, 5]);

        let delta = tracker.take_delta("plate_id").expect("plate_id delta");
        assert_eq!(delta.field_kind, "plate_id");
        assert_eq!(delta.mode, "delta");
        assert_eq!(delta.ranges.len(), 2);
        assert_eq!(delta.u32_data.expect("u32 data"), vec![8, 9]);
        assert!(tracker.take_delta("plate_id").is_none());
    }

    #[test]
    fn f32_tracker_can_emit_bitmap_delta_for_sparse_updates() {
        let mut tracker = F32FieldTracker::new(&vec![0.0; 128]);
        let mut next = vec![0.0; 128];
        for index in [0usize, 33, 66, 99, 127] {
            next[index] = 1.0;
        }
        tracker.observe(&next);

        let delta = tracker.take_delta("height").expect("bitmap delta");
        assert_eq!(delta.mode, "bitmap");
        let bitmap = delta.dirty_bitmap.expect("bitmap");
        assert_eq!(bitmap.len(), 4);
        assert_eq!(
            delta.f32_data.expect("f32 data"),
            vec![1.0, 1.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn world_sync_state_discards_pending_for_excluded_fields() {
        let mut state = WorldTransportCache {
            height: F32FieldTracker::new(&[1.0, 1.0]),
            lake_depth: F32FieldTracker::new(&[0.0, 0.0]),
            volcanism: F32FieldTracker::new(&[0.0, 0.0]),
            vertex_buoyancy: F32FieldTracker::new(&[0.0, 0.0]),
            plate_id: U32FieldTracker::new(&[0, 0]),
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
            ice_pressure: F32FieldTracker::new(&[0.0, 0.0]),
            ocean_temperature: F32FieldTracker::new(&[10.0, 10.0]),
            wind_u: F32FieldTracker::new(&[0.0, 0.0]),
            wind_v: F32FieldTracker::new(&[0.0, 0.0]),
            moisture_flux_u: F32FieldTracker::new(&[0.0, 0.0]),
            moisture_flux_v: F32FieldTracker::new(&[0.0, 0.0]),
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

    #[test]
    fn world_sync_state_tracks_ice_pressure_deltas() {
        let mut state = WorldTransportCache {
            height: F32FieldTracker::new(&[1.0, 1.0]),
            lake_depth: F32FieldTracker::new(&[0.0, 0.0]),
            volcanism: F32FieldTracker::new(&[0.0, 0.0]),
            vertex_buoyancy: F32FieldTracker::new(&[0.0, 0.0]),
            plate_id: U32FieldTracker::new(&[0, 0]),
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
            ice_pressure: F32FieldTracker::new(&[0.0, 0.0]),
            ocean_temperature: F32FieldTracker::new(&[10.0, 10.0]),
            wind_u: F32FieldTracker::new(&[0.0, 0.0]),
            wind_v: F32FieldTracker::new(&[0.0, 0.0]),
            moisture_flux_u: F32FieldTracker::new(&[0.0, 0.0]),
            moisture_flux_v: F32FieldTracker::new(&[0.0, 0.0]),
            river_transport_cost: F32FieldTracker::new(&[0.2, 0.2]),
        };

        state.ice_pressure.observe(&[0.0, 0.7]);

        let deltas = state.take_world_field_deltas(|field_kind| field_kind == "ice_pressure");
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].field_kind, "ice_pressure");
        assert_eq!(deltas[0].mode, "full");
    }

    #[test]
    fn snapshot_world_captures_core_only() {
        let geology = world::GeologyState {
            height: vec![0.2],
            lake_depth: vec![0.0],
            plate_id: vec![PlateId(0)],
            erosion_rate: vec![0.0],
            deposition_rate: vec![0.0],
            volcanism: vec![0.0],
            vertex_buoyancy: vec![0.0],
            geology_internal: vec![GeologyInternal::default()],
            boundary_condition: vec![0.0],
        };
        let mesh = world::WorldMesh {
            positions: vec![[0.0, 0.0, 1.0]],
            nbr_offsets: vec![0, 0],
            nbrs: vec![],
        };
        let sim_world = world::World::new(mesh, geology);
        assert!(!sim_world.projections.is_empty());

        let managed = ManagedWorld {
            world: sim_world.clone(),
            hydrology_dynamics: None,
            geology_dynamics: None,
            feedback: world::FeedbackQueue::new(sim_world.cell_count()),
            simulation_rate: 1.0,
            geology_params: crate::GeologyParams::default(),
            transport_cache: WorldTransportCache::from_world(&sim_world, None),
            exec_state: ManagedWorldExecState::default(),
            applied_intervention_seq: 0,
        };

        let snapshot = managed.snapshot_world();
        assert_eq!(snapshot.core.cells.geology.height, vec![0.2]);
        assert_eq!(snapshot.core.clock.tick, sim_world.clock.tick);
        assert_eq!(
            snapshot.core.entities.polity_count(),
            sim_world.entities.polity_count()
        );
    }
}
