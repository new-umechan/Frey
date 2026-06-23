use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::mem::size_of;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::application::world_dto::{DeltaRange, FieldDeltaResponse, TimelineConfig};
use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::geology_types::{GeologyParams, PlateId, PlateRelation};
use crate::sim::polity::types::{PolityGroup, PolityRelation};
use crate::sim::world;
use crate::sim::{first_phase, ExecWorldPhase};
use verification_runtime::{
    reduce_metrics_for_headless, HeadlessMetrics, VerificationMode,
    SCIENTIFIC_BENCHMARK_SAMPLE_LIMIT,
};

pub(crate) const DEFAULT_CHECKPOINT_LIMIT: usize = 512;
pub(crate) const CHECKPOINT_SNAPSHOT_INTERVAL: u64 = 64;
pub(crate) const DEFAULT_UNDO_LOG_LIMIT: usize = 512;
pub(crate) const DEFAULT_UNDO_FUTURE_PRUNE_GRACE_TICKS: u64 = 4;
pub(crate) const TICK_BOUNDARY_COMPLETED_TICK: &str = "completed_tick";
pub(crate) const DELTA_FULL_THRESHOLD_RATIO: f32 = 0.40;
#[allow(dead_code)]
pub(crate) const DEFAULT_HISTORY_LIMIT: usize = DEFAULT_CHECKPOINT_LIMIT;
#[allow(dead_code)]
pub(crate) const HISTORY_SNAPSHOT_INTERVAL: u64 = CHECKPOINT_SNAPSHOT_INTERVAL;

fn to_world_metrics(metrics: HeadlessMetrics) -> world::WorldMetrics {
    world::WorldMetrics {
        cell_count: metrics.cell_count,
        land_cells: metrics.land_cells,
        land_ratio: metrics.land_ratio,
        sea_level_offset: 0.0,
        mean_height: metrics.mean_height,
        height_std_dev: metrics.height_std_dev,
        min_height: metrics.min_height,
        max_height: metrics.max_height,
        mean_river_flux: metrics.mean_river_flux,
        max_river_flux: metrics.max_river_flux,
        top10_river_flux_sum: metrics.top10_river_flux_sum,
        river_active_cells: metrics.river_active_cells,
        river_fragmentation_ratio: metrics.river_fragmentation_ratio,
        river_ocean_reach_ratio: metrics.river_ocean_reach_ratio,
        river_mainstem_persistence: metrics.river_mainstem_persistence,
        river_flux_concentration: metrics.river_flux_concentration,
        continent_count: metrics.continent_count,
        largest_continent_cells: metrics.largest_continent_cells,
        plate_count: 0,
        global_sediment_export: 0.0,
        marine_sediment_mass: 0.0,
        solid_earth_mass_proxy: 0.0,
        solid_earth_mass_proxy_drift: 0.0,
        ocean_water_inventory: 0.0,
        ocean_water_inventory_drift: 0.0,
        ice_inventory: 0.0,
        smoothing_limited_cells_ratio: 0.0,
        mean_smoothing_factor: 1.0,
        zero_mean_adjusted_cells_ratio: 0.0,
        zero_mean_mean_abs_correction: 0.0,
        zero_mean_std_delta: 0.0,
        geology_activity: 0.0,
        boundary_activity: 0.0,
        plate_id_churn_rate: 0.0,
        orphan_cell_count: 0.0,
        single_cell_plate_count: 0.0,
        uplift_rate: 0.0,
        subsidence_rate: 0.0,
        mean_compressive: 0.0,
        mean_tensile: 0.0,
        mean_abs_diffusive_raw: 0.0,
        mean_abs_isostatic_raw: 0.0,
        mean_thickness: 0.0,
        std_thickness: 0.0,
        mean_density: 0.0,
        std_density: 0.0,
        mean_rigidity: 0.0,
        std_rigidity: 0.0,
        oceanic_cell_ratio: 0.0,
        continental_cell_ratio: 0.0,
        mean_thickness_oceanic: 0.0,
        mean_thickness_continental: 0.0,
        mean_rigidity_oceanic: 0.0,
        mean_rigidity_continental: 0.0,
    }
}

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
    bitmap_words >= 4 && range_count >= 4
}

fn build_sparse_patch<T>(before: &[T], after: &[T]) -> Option<SparsePatch<T>>
where
    T: Copy + PartialEq,
{
    if before.len() != after.len() {
        return None;
    }
    let mut indices = Vec::new();
    let mut values = Vec::new();
    for (index, (before_value, after_value)) in before.iter().zip(after.iter()).enumerate() {
        if before_value != after_value {
            indices.push(index as u32);
            values.push(*before_value);
        }
    }
    (!indices.is_empty()).then_some(SparsePatch { indices, values })
}

fn build_map_before_value_patch<K, V>(
    before: &HashMap<K, V>,
    after: &HashMap<K, V>,
) -> Option<MapBeforeValuePatch<K, V>>
where
    K: Clone + Eq + Hash,
    V: Clone + PartialEq,
{
    let mut entries = Vec::new();
    for (key, before_value) in before {
        match after.get(key) {
            Some(after_value) if after_value == before_value => {}
            _ => entries.push((key.clone(), Some(before_value.clone()))),
        }
    }
    for key in after.keys() {
        if !before.contains_key(key) {
            entries.push((key.clone(), None));
        }
    }
    (!entries.is_empty()).then_some(MapBeforeValuePatch { entries })
}

fn build_compact_river_downstream_patch(
    before: &[SmallVec<[(u32, f32); 4]>],
    after: &[SmallVec<[(u32, f32); 4]>],
) -> Option<CompactRiverDownstreamPatch> {
    if before.len() != after.len() {
        return None;
    }
    let mut cell_indices = Vec::new();
    let mut route_offsets = Vec::new();
    let mut route_cells = Vec::new();
    let mut route_weights = Vec::new();
    route_offsets.push(0);

    for (cell_index, (before_routes, after_routes)) in before.iter().zip(after.iter()).enumerate() {
        if before_routes == after_routes {
            continue;
        }
        cell_indices.push(cell_index as u32);
        for (next_cell, weight) in before_routes.iter().copied() {
            route_cells.push(next_cell);
            route_weights.push(weight);
        }
        route_offsets.push(route_cells.len() as u32);
    }

    (!cell_indices.is_empty()).then_some(CompactRiverDownstreamPatch {
        cell_indices,
        route_offsets,
        route_cells,
        route_weights,
    })
}

fn apply_sparse_patch<T>(values: &mut [T], patch: &SparsePatch<T>)
where
    T: Copy,
{
    for (index, value) in patch.indices.iter().zip(patch.values.iter()) {
        if let Some(slot) = values.get_mut(*index as usize) {
            *slot = *value;
        }
    }
}

fn apply_map_before_value_patch<K, V>(values: &mut HashMap<K, V>, patch: &MapBeforeValuePatch<K, V>)
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    for (key, before_value) in &patch.entries {
        match before_value {
            Some(value) => {
                values.insert(key.clone(), value.clone());
            }
            None => {
                values.remove(key);
            }
        }
    }
}

fn apply_compact_river_downstream_patch(
    values: &mut [SmallVec<[(u32, f32); 4]>],
    patch: &CompactRiverDownstreamPatch,
) {
    for (patch_index, cell_index) in patch.cell_indices.iter().copied().enumerate() {
        let start = patch.route_offsets.get(patch_index).copied().unwrap_or(0) as usize;
        let end = patch
            .route_offsets
            .get(patch_index + 1)
            .copied()
            .unwrap_or(start as u32) as usize;
        let mut routes = SmallVec::<[(u32, f32); 4]>::new();
        for route_index in start..end {
            let next_cell = patch.route_cells.get(route_index).copied().unwrap_or(0);
            let weight = patch.route_weights.get(route_index).copied().unwrap_or(0.0);
            routes.push((next_cell, weight));
        }
        if let Some(slot) = values.get_mut(cell_index as usize) {
            *slot = routes;
        }
    }
}

fn vec_bytes<T>(values: &[T]) -> usize {
    values.len() * size_of::<T>()
}

fn river_downstream_bytes(values: &[SmallVec<[(u32, f32); 4]>]) -> usize {
    values
        .iter()
        .map(|routes| routes.len() * size_of::<(u32, f32)>())
        .sum()
}

fn estimate_world_core_bytes(core: &world::WorldCore) -> usize {
    let geology = &core.cells.geology;
    let climate = &core.cells.climate;
    let glaciology = &core.cells.glaciology;
    let hydrology = &core.cells.hydrology;
    let ecology = &core.cells.ecology;
    let domesticates = &core.cells.domesticates;
    let subsistence = &core.cells.subsistence;
    let population = &core.cells.population;
    let settlement = &core.cells.settlement;
    let polity = &core.cells.polity;
    let conflict = &core.cells.conflict;

    vec_bytes(&geology.height)
        + vec_bytes(&geology.lake_depth)
        + vec_bytes(&geology.plate_id)
        + vec_bytes(&geology.volcanism)
        + vec_bytes(&geology.vertex_buoyancy)
        + vec_bytes(&geology.geology_internal)
        + vec_bytes(&geology.boundary_condition)
        + vec_bytes(&climate.temperature)
        + vec_bytes(&climate.precipitation)
        + vec_bytes(&climate.evapotranspiration)
        + vec_bytes(&climate.runoff)
        + vec_bytes(&climate.aridity)
        + vec_bytes(&climate.ocean_temperature)
        + vec_bytes(&climate.precipitable_water)
        + vec_bytes(&climate.cloud_water)
        + vec_bytes(&climate.wind_u)
        + vec_bytes(&climate.wind_v)
        + vec_bytes(&climate.moisture_flux_u)
        + vec_bytes(&climate.moisture_flux_v)
        + vec_bytes(&glaciology.ice_thickness)
        + vec_bytes(&glaciology.ice_load)
        + vec_bytes(&glaciology.accumulation)
        + vec_bytes(&glaciology.ablation)
        + vec_bytes(&glaciology.isostatic_adjustment)
        + vec_bytes(&glaciology.applied_isostatic_adjustment)
        + vec_bytes(&glaciology.glacial_erosion_rate)
        + vec_bytes(&glaciology.glacial_melt_runoff)
        + river_downstream_bytes(&hydrology.river_downstream)
        + vec_bytes(&hydrology.river_next)
        + vec_bytes(&hydrology.river_flow)
        + vec_bytes(&hydrology.erosion_rate)
        + vec_bytes(&hydrology.deposition_rate)
        + vec_bytes(&hydrology.river_transport_cost)
        + vec_bytes(&hydrology.is_lake)
        + vec_bytes(&hydrology.sink_id)
        + vec_bytes(&hydrology.sink_route_next)
        + vec_bytes(&hydrology.sink_member_offsets)
        + vec_bytes(&hydrology.sink_member_cells)
        + vec_bytes(&hydrology.sink_spill_cell)
        + vec_bytes(&hydrology.sink_spill_to)
        + vec_bytes(&hydrology.sink_spill_level)
        + vec_bytes(&hydrology.sink_capacity_total)
        + vec_bytes(&hydrology.sink_capacity_remaining)
        + vec_bytes(&hydrology.sink_storage_water)
        + vec_bytes(&hydrology.sink_storage_sediment)
        + vec_bytes(&hydrology.sink_overflow_active)
        + vec_bytes(&ecology.biome)
        + vec_bytes(&ecology.tree_cover)
        + vec_bytes(&ecology.ground_cover)
        + vec_bytes(&ecology.disturbance)
        + vec_bytes(&ecology.soil_fertility)
        + vec_bytes(&ecology.ecology_internal)
        + vec_bytes(&domesticates.crop_available)
        + vec_bytes(&domesticates.crop_adoption)
        + vec_bytes(&domesticates.livestock_available)
        + vec_bytes(&domesticates.livestock_adoption)
        + vec_bytes(&subsistence.subsistence_mix)
        + vec_bytes(&subsistence.food_energy_mean)
        + vec_bytes(&subsistence.food_energy_variance)
        + vec_bytes(&subsistence.buffer_capacity)
        + vec_bytes(&subsistence.mobility_capacity)
        + vec_bytes(&subsistence.land_use_intensity)
        + vec_bytes(&population.population)
        + vec_bytes(&population.birth_rate)
        + vec_bytes(&population.death_rate)
        + vec_bytes(&settlement.urbanization)
        + vec_bytes(&polity.polity_id)
        + vec_bytes(&conflict.conflict_intensity)
        + vec_bytes(&conflict.occupier_id)
        + size_of::<world::EntityState>()
        + size_of::<world::WorldRelations>()
        + size_of::<world::ClockState>()
        + size_of::<world::WorldControlState>()
}

fn estimate_sparse_patch_bytes<T>(patch: &SparsePatch<T>) -> usize {
    vec_bytes(&patch.indices) + vec_bytes(&patch.values)
}

fn estimate_polity_group_bytes(group: &PolityGroup) -> usize {
    size_of::<PolityGroup>() + vec_bytes(&group.members)
}

fn estimate_map_before_value_patch_bytes<K, V, F>(
    patch: &MapBeforeValuePatch<K, V>,
    estimate_value_bytes: F,
) -> usize
where
    F: Fn(&V) -> usize,
{
    patch
        .entries
        .iter()
        .map(|(_key, value)| {
            size_of::<K>()
                + size_of::<Option<V>>()
                + value.as_ref().map(&estimate_value_bytes).unwrap_or(0)
        })
        .sum()
}

fn nearest_checkpoint_tick(
    checkpoints: &BTreeMap<u64, CheckpointSnapshot>,
    target_tick: u64,
) -> Option<u64> {
    checkpoints
        .keys()
        .copied()
        .min_by_key(|tick| tick.abs_diff(target_tick))
}

fn prunable_checkpoint_tick_for_seek_value(
    checkpoints: &BTreeMap<u64, CheckpointSnapshot>,
    current_tick: u64,
) -> Option<u64> {
    let ticks = checkpoints.keys().copied().collect::<Vec<_>>();
    if ticks.len() <= 2 {
        return None;
    }

    let first = *ticks.first()?;
    let last = *ticks.last()?;
    let nearest_current = nearest_checkpoint_tick(checkpoints, current_tick);
    let mut best: Option<(u64, u64, u64)> = None;

    for window in ticks.windows(3) {
        let prev = window[0];
        let tick = window[1];
        let next = window[2];
        if tick == first || tick == last || Some(tick) == nearest_current {
            continue;
        }

        let left_gap = tick.saturating_sub(prev);
        let right_gap = next.saturating_sub(tick);
        let redundancy = left_gap.min(right_gap);
        let merged_gap = next.saturating_sub(prev);
        let candidate = (redundancy, merged_gap, tick);
        if best.map(|current| candidate < current).unwrap_or(true) {
            best = Some(candidate);
        }
    }

    best.map(|(_, _, tick)| tick)
}

fn prunable_undo_tick_for_rewind_value(
    undo_logs: &BTreeMap<u64, TickUndoLog>,
    current_tick: u64,
    future_prune_grace_ticks: u64,
) -> Option<u64> {
    if undo_logs.is_empty() {
        return None;
    }

    if undo_logs.len() == 1 {
        return undo_logs.keys().next().copied();
    }

    let protect_current = undo_logs.contains_key(&current_tick);
    undo_logs
        .keys()
        .copied()
        .filter(|tick| !protect_current || *tick != current_tick)
        .max_by_key(|tick| {
            let future_beyond_grace = *tick > current_tick.saturating_add(future_prune_grace_ticks);
            (
                u8::from(future_beyond_grace),
                tick.abs_diff(current_tick),
                current_tick.saturating_sub(*tick),
                *tick,
            )
        })
}

fn estimate_compact_river_downstream_patch_bytes(patch: &CompactRiverDownstreamPatch) -> usize {
    vec_bytes(&patch.cell_indices)
        + vec_bytes(&patch.route_offsets)
        + vec_bytes(&patch.route_cells)
        + vec_bytes(&patch.route_weights)
}

#[derive(Clone, Copy)]
pub(crate) enum ChangedField {
    Geology,
    Climate,
    Glaciology,
    Hydrology,
    Ecology,
    Domesticates,
    Subsistence,
    Population,
    Settlement,
    Polity,
    Conflict,
    Entities,
    Relations,
    Clock,
    Control,
    HydrologyDynamics,
    GeologyDynamics,
    AppliedInterventionSeq,
}

impl ChangedField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Geology => "geology",
            Self::Climate => "climate",
            Self::Glaciology => "glaciology",
            Self::Hydrology => "hydrology",
            Self::Ecology => "ecology",
            Self::Domesticates => "domesticates",
            Self::Subsistence => "subsistence",
            Self::Population => "population",
            Self::Settlement => "settlement",
            Self::Polity => "polity",
            Self::Conflict => "conflict",
            Self::Entities => "entities",
            Self::Relations => "relations",
            Self::Clock => "clock",
            Self::Control => "control",
            Self::HydrologyDynamics => "hydrology_dynamics",
            Self::GeologyDynamics => "geology_dynamics",
            Self::AppliedInterventionSeq => "applied_intervention_seq",
        }
    }
}

pub(crate) const ALL_CHANGED_FIELDS: [ChangedField; 18] = [
    ChangedField::Geology,
    ChangedField::Climate,
    ChangedField::Glaciology,
    ChangedField::Hydrology,
    ChangedField::Ecology,
    ChangedField::Domesticates,
    ChangedField::Subsistence,
    ChangedField::Population,
    ChangedField::Settlement,
    ChangedField::Polity,
    ChangedField::Conflict,
    ChangedField::Entities,
    ChangedField::Relations,
    ChangedField::Clock,
    ChangedField::Control,
    ChangedField::HydrologyDynamics,
    ChangedField::GeologyDynamics,
    ChangedField::AppliedInterventionSeq,
];

fn push_changed_field(changed_fields: &mut Vec<ChangedField>, field: ChangedField) {
    changed_fields.push(field);
}

fn record_runtime_optional_change_if_different<T>(
    changed_fields: &mut Vec<ChangedField>,
    field: ChangedField,
    before: T,
    after: T,
) -> Option<T>
where
    T: PartialEq,
{
    if before != after {
        push_changed_field(changed_fields, field);
        Some(before)
    } else {
        None
    }
}

#[derive(Default)]
struct RuntimeAuxChanges {
    hydrology_dynamics_before: Option<Option<ErosionAutomatonState>>,
    geology_dynamics_before: Option<Option<world::GeologyDynamicsState>>,
    applied_intervention_seq_before: Option<u64>,
}

fn build_core_change_set_from_world_diff(
    before_core: &world::WorldCore,
    after_core: &world::WorldCore,
    changed_fields: &mut Vec<ChangedField>,
) -> WorldCoreChangeSet {
    let mut core_change_set = WorldCoreChangeSet::default();

    if let Some(geology_undo) =
        GeologyUndoState::from_diff(&before_core.cells.geology, &after_core.cells.geology)
    {
        push_changed_field(changed_fields, ChangedField::Geology);
        core_change_set.geology = Some(geology_undo);
    }
    if let Some(climate_undo) =
        ClimateUndoState::from_diff(&before_core.cells.climate, &after_core.cells.climate)
    {
        push_changed_field(changed_fields, ChangedField::Climate);
        core_change_set.climate = Some(climate_undo);
    }
    if let Some(glaciology_undo) =
        GlaciologyUndoState::from_diff(&before_core.cells.glaciology, &after_core.cells.glaciology)
    {
        push_changed_field(changed_fields, ChangedField::Glaciology);
        core_change_set.glaciology = Some(glaciology_undo);
    }
    if let Some(hydrology_undo) =
        HydrologyUndoState::from_diff(&before_core.cells.hydrology, &after_core.cells.hydrology)
    {
        push_changed_field(changed_fields, ChangedField::Hydrology);
        core_change_set.hydrology = Some(hydrology_undo);
    }
    if let Some(ecology_undo) =
        EcologyUndoState::from_diff(&before_core.cells.ecology, &after_core.cells.ecology)
    {
        push_changed_field(changed_fields, ChangedField::Ecology);
        core_change_set.ecology = Some(ecology_undo);
    }
    if let Some(domesticates_undo) = DomesticatesUndoState::from_diff(
        &before_core.cells.domesticates,
        &after_core.cells.domesticates,
    ) {
        push_changed_field(changed_fields, ChangedField::Domesticates);
        core_change_set.domesticates = Some(domesticates_undo);
    }
    if let Some(subsistence_undo) = SubsistenceUndoState::from_diff(
        &before_core.cells.subsistence,
        &after_core.cells.subsistence,
    ) {
        push_changed_field(changed_fields, ChangedField::Subsistence);
        core_change_set.subsistence = Some(subsistence_undo);
    }
    if let Some(population_undo) =
        PopulationUndoState::from_diff(&before_core.cells.population, &after_core.cells.population)
    {
        push_changed_field(changed_fields, ChangedField::Population);
        core_change_set.population = Some(population_undo);
    }
    if let Some(settlement_undo) =
        SettlementUndoState::from_diff(&before_core.cells.settlement, &after_core.cells.settlement)
    {
        push_changed_field(changed_fields, ChangedField::Settlement);
        core_change_set.settlement = Some(settlement_undo);
    }
    if let Some(polity_undo) =
        PolityUndoState::from_diff(&before_core.cells.polity, &after_core.cells.polity)
    {
        push_changed_field(changed_fields, ChangedField::Polity);
        core_change_set.polity = Some(polity_undo);
    }
    if let Some(conflict_undo) =
        ConflictUndoState::from_diff(&before_core.cells.conflict, &after_core.cells.conflict)
    {
        push_changed_field(changed_fields, ChangedField::Conflict);
        core_change_set.conflict = Some(conflict_undo);
    }
    if let Some(entity_undo) =
        EntityUndoState::from_diff(&before_core.entities, &after_core.entities)
    {
        push_changed_field(changed_fields, ChangedField::Entities);
        core_change_set.entities = Some(entity_undo);
    }
    if let Some(relations_undo) =
        RelationsUndoState::from_diff(&before_core.relations, &after_core.relations)
    {
        push_changed_field(changed_fields, ChangedField::Relations);
        core_change_set.relations = Some(relations_undo);
    }
    if let Some(clock_undo) = ClockUndoState::from_diff(&before_core.clock, &after_core.clock) {
        push_changed_field(changed_fields, ChangedField::Clock);
        core_change_set.clock = Some(clock_undo);
    }
    if let Some(control_undo) =
        ControlUndoState::from_diff(&before_core.control, &after_core.control)
    {
        push_changed_field(changed_fields, ChangedField::Control);
        core_change_set.control = Some(control_undo);
    }

    core_change_set
}

fn build_runtime_aux_changes(
    snapshot_before_tick: &CheckpointSnapshot,
    managed: &ManagedWorld,
    changed_fields: &mut Vec<ChangedField>,
) -> RuntimeAuxChanges {
    RuntimeAuxChanges {
        hydrology_dynamics_before: record_runtime_optional_change_if_different(
            changed_fields,
            ChangedField::HydrologyDynamics,
            snapshot_before_tick.hydrology_dynamics.clone(),
            managed.hydrology_dynamics.clone(),
        ),
        geology_dynamics_before: record_runtime_optional_change_if_different(
            changed_fields,
            ChangedField::GeologyDynamics,
            snapshot_before_tick.geology_dynamics.clone(),
            managed.geology_dynamics.clone(),
        ),
        applied_intervention_seq_before: record_runtime_optional_change_if_different(
            changed_fields,
            ChangedField::AppliedInterventionSeq,
            snapshot_before_tick.applied_intervention_seq,
            managed.applied_intervention_seq,
        ),
    }
}

macro_rules! apply_sparse_fields {
    ($target:expr, $undo:expr, [$($field:ident),+ $(,)?]) => {
        $(
            if let Some(patch) = &$undo.$field {
                apply_sparse_patch(&mut $target.$field, patch);
            }
        )+
    };
}

macro_rules! any_patch {
    ($($patch:expr),+ $(,)?) => {
        false $(|| $patch.is_some())+
    };
}

#[derive(Clone)]
pub(crate) struct RangeDelta {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone)]
pub(crate) struct F32FieldTracker {
    pub shadow: Vec<f32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub dirty_bitmap: Vec<u32>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(crate) struct I32FieldTracker {
    pub shadow: Vec<i32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub dirty_bitmap: Vec<u32>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(crate) struct U32FieldTracker {
    pub shadow: Vec<u32>,
    pub dirty_ranges: Vec<RangeDelta>,
    pub dirty_bitmap: Vec<u32>,
    pub force_full: bool,
}

#[derive(Clone)]
pub(crate) struct TimelineViewCache {
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
    pub biome: I32FieldTracker,
    pub river_transport_cost: F32FieldTracker,
    pub crop_adoption_wheat: F32FieldTracker,
    pub crop_adoption_rice: F32FieldTracker,
    pub crop_adoption_maize: F32FieldTracker,
    pub crop_adoption_millet: F32FieldTracker,
    pub crop_adoption_potato: F32FieldTracker,
    pub crop_adoption_cassava: F32FieldTracker,
    pub crop_adoption_sorghum: F32FieldTracker,
    pub crop_adoption_yam: F32FieldTracker,
    pub crop_available_wheat: F32FieldTracker,
    pub crop_available_rice: F32FieldTracker,
    pub crop_available_maize: F32FieldTracker,
    pub crop_available_millet: F32FieldTracker,
    pub crop_available_potato: F32FieldTracker,
    pub crop_available_cassava: F32FieldTracker,
    pub crop_available_sorghum: F32FieldTracker,
    pub crop_available_yam: F32FieldTracker,
    pub livestock_adoption_cattle: F32FieldTracker,
    pub livestock_adoption_horse: F32FieldTracker,
    pub livestock_adoption_sheep: F32FieldTracker,
    pub livestock_adoption_pig: F32FieldTracker,
    pub livestock_adoption_camel: F32FieldTracker,
    pub livestock_available_cattle: F32FieldTracker,
    pub livestock_available_horse: F32FieldTracker,
    pub livestock_available_sheep: F32FieldTracker,
    pub livestock_available_pig: F32FieldTracker,
    pub livestock_available_camel: F32FieldTracker,
}

#[allow(dead_code)]
pub(crate) type WorldTransportCache = TimelineViewCache;

#[derive(Clone)]
pub(crate) struct ManagedWorld {
    pub world: world::World,
    pub hydrology_dynamics: Option<ErosionAutomatonState>,
    pub geology_dynamics: Option<world::GeologyDynamicsState>,
    pub feedback: world::FeedbackQueue,
    pub simulation_rate: f32,
    pub verification_mode: VerificationMode,
    pub reduced_metrics: Option<world::WorldMetrics>,
    pub scientific_benchmark_samples: Vec<ScientificBenchmarkSample>,
    pub geology_params: GeologyParams,
    pub transport_cache: WorldTransportCache,
    pub exec_state: ManagedWorldExecState,
    pub applied_intervention_seq: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ScientificBenchmarkSample {
    pub tick: u64,
    pub era: String,
    pub metrics: world::WorldMetrics,
}

#[derive(Clone)]
pub(crate) struct CheckpointSnapshot {
    pub core: world::WorldCore,
    pub hydrology_dynamics: Option<ErosionAutomatonState>,
    pub geology_dynamics: Option<world::GeologyDynamicsState>,
    pub applied_intervention_seq: u64,
}

#[allow(dead_code)]
pub(crate) type WorldHistorySnapshot = CheckpointSnapshot;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum InterventionCommand {
    SetSimulationRate { value: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct InterventionEvent {
    pub tick: u64,
    pub sequence: u64,
    pub command: InterventionCommand,
}

#[derive(Clone, Default)]
pub(crate) struct TimelineArchive {
    pub checkpoints: BTreeMap<u64, CheckpointSnapshot>,
    pub interventions: Vec<InterventionEvent>,
    pub next_intervention_seq: u64,
}

#[derive(Clone)]
pub(crate) struct TimelineRetentionPolicy {
    pub checkpoint_interval: u64,
    pub checkpoint_limit: usize,
    pub undo_log_limit: usize,
    pub undo_future_prune_grace_ticks: u64,
    pub max_estimated_bytes: Option<usize>,
}

impl Default for TimelineRetentionPolicy {
    fn default() -> Self {
        Self {
            checkpoint_interval: CHECKPOINT_SNAPSHOT_INTERVAL,
            checkpoint_limit: DEFAULT_CHECKPOINT_LIMIT,
            undo_log_limit: DEFAULT_UNDO_LOG_LIMIT,
            undo_future_prune_grace_ticks: DEFAULT_UNDO_FUTURE_PRUNE_GRACE_TICKS,
            max_estimated_bytes: None,
        }
    }
}

impl TimelineRetentionPolicy {
    pub fn from_config(config: Option<&TimelineConfig>) -> Self {
        let mut policy = Self::default();
        if let Some(config) = config {
            if let Some(interval) = config.checkpoint_interval {
                policy.checkpoint_interval = interval.max(1);
            }
            if let Some(limit) = config.checkpoint_limit {
                policy.checkpoint_limit = limit.max(1);
            }
            if let Some(limit) = config.undo_log_limit {
                policy.undo_log_limit = limit.max(1);
            }
            if let Some(grace_ticks) = config.undo_future_prune_grace_ticks {
                policy.undo_future_prune_grace_ticks = grace_ticks;
            }
            if let Some(max_estimated_bytes) = config.max_estimated_bytes {
                policy.max_estimated_bytes = Some(max_estimated_bytes.max(1));
            }
        }
        policy
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct TimelineCursor {
    pub tick: u64,
    pub head_tick: u64,
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TickUndoLog {
    pub tick: u64,
    pub pending_snapshot_before_tick: Option<CheckpointSnapshot>,
    pub core_change_set: WorldCoreChangeSet,
    pub hydrology_dynamics_before: Option<Option<ErosionAutomatonState>>,
    pub geology_dynamics_before: Option<Option<world::GeologyDynamicsState>>,
    pub applied_intervention_seq_before: Option<u64>,
    pub changed_fields: Vec<String>,
}

impl TickUndoLog {
    pub fn estimated_bytes(&self) -> usize {
        self.pending_snapshot_before_tick
            .as_ref()
            .map(CheckpointSnapshot::estimated_bytes)
            .unwrap_or(0)
            + self.core_change_set.estimated_bytes()
            + self
                .hydrology_dynamics_before
                .as_ref()
                .map(|_| size_of::<Option<ErosionAutomatonState>>())
                .unwrap_or(0)
            + self
                .geology_dynamics_before
                .as_ref()
                .map(|_| size_of::<Option<world::GeologyDynamicsState>>())
                .unwrap_or(0)
            + self
                .applied_intervention_seq_before
                .as_ref()
                .map(|_| size_of::<u64>())
                .unwrap_or(0)
            + self.changed_fields.iter().map(String::len).sum::<usize>()
    }
}

#[derive(Clone, Default)]
pub(crate) struct SparsePatch<T> {
    pub indices: Vec<u32>,
    pub values: Vec<T>,
}

pub(crate) type SparseF32Patch = SparsePatch<f32>;
pub(crate) type SparseI32Patch = SparsePatch<i32>;
pub(crate) type SparseU32Patch = SparsePatch<u32>;
pub(crate) type SparseU8Patch = SparsePatch<u8>;
pub(crate) type SparseBoolPatch = SparsePatch<bool>;
pub(crate) type SparseBiomePatch = SparsePatch<world::Biome>;

#[derive(Clone, Default)]
pub(crate) struct CompactRiverDownstreamPatch {
    pub cell_indices: Vec<u32>,
    pub route_offsets: Vec<u32>,
    pub route_cells: Vec<u32>,
    pub route_weights: Vec<f32>,
}

impl CheckpointSnapshot {
    pub fn estimated_bytes(&self) -> usize {
        estimate_world_core_bytes(&self.core)
            + self
                .hydrology_dynamics
                .as_ref()
                .map(|_| size_of::<ErosionAutomatonState>())
                .unwrap_or(0)
            + self
                .geology_dynamics
                .as_ref()
                .map(|_| size_of::<world::GeologyDynamicsState>())
                .unwrap_or(0)
            + size_of::<u64>()
    }
}

impl CompactRiverDownstreamPatch {
    pub fn estimated_bytes(&self) -> usize {
        estimate_compact_river_downstream_patch_bytes(self)
    }
}

impl GeologyUndoState {
    pub fn from_diff(before: &world::GeologyState, after: &world::GeologyState) -> Option<Self> {
        let height_patch = build_sparse_patch(&before.height, &after.height);
        let non_height_changed = before.lake_depth != after.lake_depth
            || before.plate_id != after.plate_id
            || before.volcanism != after.volcanism
            || before.vertex_buoyancy != after.vertex_buoyancy
            || before.geology_internal != after.geology_internal
            || before.boundary_condition != after.boundary_condition
            || before.smoothing_limited_cells_ratio != after.smoothing_limited_cells_ratio
            || before.mean_smoothing_factor != after.mean_smoothing_factor
            || before.zero_mean_adjusted_cells_ratio != after.zero_mean_adjusted_cells_ratio
            || before.zero_mean_mean_abs_correction != after.zero_mean_mean_abs_correction
            || before.zero_mean_std_delta != after.zero_mean_std_delta;
        if non_height_changed || height_patch.is_some() {
            Some(if non_height_changed {
                Self {
                    full: Some(before.clone()),
                    height: None,
                }
            } else {
                Self {
                    full: None,
                    height: height_patch,
                }
            })
        } else {
            None
        }
    }

    pub fn apply_to(&self, target: &mut world::GeologyState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else if let Some(height) = &self.height {
            apply_sparse_patch(&mut target.height, height);
        }
    }
}

impl ClimateUndoState {
    pub fn from_diff(before: &world::ClimateState, after: &world::ClimateState) -> Option<Self> {
        if before == after {
            return None;
        }
        let temperature = build_sparse_patch(&before.temperature, &after.temperature);
        let precipitation = build_sparse_patch(&before.precipitation, &after.precipitation);
        let evapotranspiration =
            build_sparse_patch(&before.evapotranspiration, &after.evapotranspiration);
        let runoff = build_sparse_patch(&before.runoff, &after.runoff);
        let aridity = build_sparse_patch(&before.aridity, &after.aridity);
        let ocean_temperature =
            build_sparse_patch(&before.ocean_temperature, &after.ocean_temperature);
        let precipitable_water =
            build_sparse_patch(&before.precipitable_water, &after.precipitable_water);
        let cloud_water = build_sparse_patch(&before.cloud_water, &after.cloud_water);
        let wind_u = build_sparse_patch(&before.wind_u, &after.wind_u);
        let wind_v = build_sparse_patch(&before.wind_v, &after.wind_v);
        let moisture_flux_u = build_sparse_patch(&before.moisture_flux_u, &after.moisture_flux_u);
        let moisture_flux_v = build_sparse_patch(&before.moisture_flux_v, &after.moisture_flux_v);
        let has_sparse_patch = any_patch!(
            temperature,
            precipitation,
            evapotranspiration,
            runoff,
            aridity,
            ocean_temperature,
            precipitable_water,
            cloud_water,
            wind_u,
            wind_v,
            moisture_flux_u,
            moisture_flux_v,
        );
        Some(if has_sparse_patch {
            Self {
                full: None,
                temperature,
                precipitation,
                evapotranspiration,
                runoff,
                aridity,
                ocean_temperature,
                precipitable_water,
                cloud_water,
                wind_u,
                wind_v,
                moisture_flux_u,
                moisture_flux_v,
            }
        } else {
            Self {
                full: Some(before.clone()),
                temperature: None,
                precipitation: None,
                evapotranspiration: None,
                runoff: None,
                aridity: None,
                ocean_temperature: None,
                precipitable_water: None,
                cloud_water: None,
                wind_u: None,
                wind_v: None,
                moisture_flux_u: None,
                moisture_flux_v: None,
            }
        })
    }

    pub fn apply_to(&self, target: &mut world::ClimateState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(
                target,
                self,
                [
                    temperature,
                    precipitation,
                    evapotranspiration,
                    runoff,
                    aridity,
                    ocean_temperature,
                    precipitable_water,
                    cloud_water,
                    wind_u,
                    wind_v,
                    moisture_flux_u,
                    moisture_flux_v,
                ]
            );
        }
    }
}

impl GlaciologyUndoState {
    pub fn from_diff(
        before: &world::GlaciologyState,
        after: &world::GlaciologyState,
    ) -> Option<Self> {
        if before == after {
            return None;
        }
        let ice_thickness = build_sparse_patch(&before.ice_thickness, &after.ice_thickness);
        let ice_load = build_sparse_patch(&before.ice_load, &after.ice_load);
        let accumulation = build_sparse_patch(&before.accumulation, &after.accumulation);
        let ablation = build_sparse_patch(&before.ablation, &after.ablation);
        let isostatic_adjustment =
            build_sparse_patch(&before.isostatic_adjustment, &after.isostatic_adjustment);
        let applied_isostatic_adjustment = build_sparse_patch(
            &before.applied_isostatic_adjustment,
            &after.applied_isostatic_adjustment,
        );
        let glacial_erosion_rate =
            build_sparse_patch(&before.glacial_erosion_rate, &after.glacial_erosion_rate);
        let glacial_melt_runoff =
            build_sparse_patch(&before.glacial_melt_runoff, &after.glacial_melt_runoff);
        let has_sparse_patch = any_patch!(
            ice_thickness,
            ice_load,
            accumulation,
            ablation,
            isostatic_adjustment,
            applied_isostatic_adjustment,
            glacial_erosion_rate,
            glacial_melt_runoff,
        );
        Some(if has_sparse_patch {
            Self {
                full: None,
                ice_thickness,
                ice_load,
                accumulation,
                ablation,
                isostatic_adjustment,
                applied_isostatic_adjustment,
                glacial_erosion_rate,
                glacial_melt_runoff,
            }
        } else {
            Self {
                full: Some(before.clone()),
                ice_thickness: None,
                ice_load: None,
                accumulation: None,
                ablation: None,
                isostatic_adjustment: None,
                applied_isostatic_adjustment: None,
                glacial_erosion_rate: None,
                glacial_melt_runoff: None,
            }
        })
    }

    pub fn apply_to(&self, target: &mut world::GlaciologyState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(
                target,
                self,
                [
                    ice_thickness,
                    ice_load,
                    accumulation,
                    ablation,
                    isostatic_adjustment,
                    applied_isostatic_adjustment,
                    glacial_erosion_rate,
                    glacial_melt_runoff,
                ]
            );
        }
    }
}

impl HydrologyUndoState {
    pub fn from_diff(
        before: &world::HydrologyState,
        after: &world::HydrologyState,
    ) -> Option<Self> {
        let river_downstream =
            build_compact_river_downstream_patch(&before.river_downstream, &after.river_downstream);
        let river_flow = build_sparse_patch(&before.river_flow, &after.river_flow);
        let river_next = build_sparse_patch(&before.river_next, &after.river_next);
        let erosion_rate = build_sparse_patch(&before.erosion_rate, &after.erosion_rate);
        let deposition_rate = build_sparse_patch(&before.deposition_rate, &after.deposition_rate);
        let river_transport_cost =
            build_sparse_patch(&before.river_transport_cost, &after.river_transport_cost);
        let is_lake = build_sparse_patch(&before.is_lake, &after.is_lake);
        let sink_id = build_sparse_patch(&before.sink_id, &after.sink_id);
        let sink_route_next = build_sparse_patch(&before.sink_route_next, &after.sink_route_next);
        let sink_member_offsets =
            build_sparse_patch(&before.sink_member_offsets, &after.sink_member_offsets);
        let sink_member_cells =
            build_sparse_patch(&before.sink_member_cells, &after.sink_member_cells);
        let sink_spill_cell = build_sparse_patch(&before.sink_spill_cell, &after.sink_spill_cell);
        let sink_spill_to = build_sparse_patch(&before.sink_spill_to, &after.sink_spill_to);
        let sink_spill_level =
            build_sparse_patch(&before.sink_spill_level, &after.sink_spill_level);
        let sink_capacity_total =
            build_sparse_patch(&before.sink_capacity_total, &after.sink_capacity_total);
        let sink_capacity_remaining = build_sparse_patch(
            &before.sink_capacity_remaining,
            &after.sink_capacity_remaining,
        );
        let sink_storage_water =
            build_sparse_patch(&before.sink_storage_water, &after.sink_storage_water);
        let sink_storage_sediment =
            build_sparse_patch(&before.sink_storage_sediment, &after.sink_storage_sediment);
        let sink_overflow_active =
            build_sparse_patch(&before.sink_overflow_active, &after.sink_overflow_active);
        let has_sparse_patch = any_patch!(
            river_downstream,
            river_flow,
            river_next,
            erosion_rate,
            deposition_rate,
            river_transport_cost,
            is_lake,
            sink_id,
            sink_route_next,
            sink_member_offsets,
            sink_member_cells,
            sink_spill_cell,
            sink_spill_to,
            sink_spill_level,
            sink_capacity_total,
            sink_capacity_remaining,
            sink_storage_water,
            sink_storage_sediment,
            sink_overflow_active,
        );
        if !has_sparse_patch {
            return None;
        }
        Some(Self {
            full: None,
            river_downstream,
            river_flow,
            river_next,
            erosion_rate,
            deposition_rate,
            river_transport_cost,
            is_lake,
            sink_id,
            sink_route_next,
            sink_member_offsets,
            sink_member_cells,
            sink_spill_cell,
            sink_spill_to,
            sink_spill_level,
            sink_capacity_total,
            sink_capacity_remaining,
            sink_storage_water,
            sink_storage_sediment,
            sink_overflow_active,
        })
    }

    pub fn apply_to(&self, target: &mut world::HydrologyState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            if let Some(patch) = &self.river_downstream {
                apply_compact_river_downstream_patch(&mut target.river_downstream, patch);
            }
            apply_sparse_fields!(
                target,
                self,
                [
                    river_flow,
                    river_next,
                    erosion_rate,
                    deposition_rate,
                    river_transport_cost,
                    is_lake,
                    sink_id,
                    sink_route_next,
                    sink_member_offsets,
                    sink_member_cells,
                    sink_spill_cell,
                    sink_spill_to,
                    sink_spill_level,
                    sink_capacity_total,
                    sink_capacity_remaining,
                    sink_storage_water,
                    sink_storage_sediment,
                    sink_overflow_active,
                ]
            );
        }
    }
}

impl EcologyUndoState {
    pub fn from_diff(before: &world::EcologyState, after: &world::EcologyState) -> Option<Self> {
        let biome = build_sparse_patch(&before.biome, &after.biome);
        let tree_cover = build_sparse_patch(&before.tree_cover, &after.tree_cover);
        let ground_cover = build_sparse_patch(&before.ground_cover, &after.ground_cover);
        let disturbance = build_sparse_patch(&before.disturbance, &after.disturbance);
        let soil_fertility = build_sparse_patch(&before.soil_fertility, &after.soil_fertility);
        let non_selected_changed = before.ecology_internal != after.ecology_internal;
        let has_sparse_patch =
            any_patch!(biome, tree_cover, ground_cover, disturbance, soil_fertility,);
        if !non_selected_changed && !has_sparse_patch {
            return None;
        }
        Some(if non_selected_changed {
            Self {
                full: Some(before.clone()),
                biome: None,
                tree_cover: None,
                ground_cover: None,
                disturbance: None,
                soil_fertility: None,
            }
        } else {
            Self {
                full: None,
                biome,
                tree_cover,
                ground_cover,
                disturbance,
                soil_fertility,
            }
        })
    }

    pub fn apply_to(&self, target: &mut world::EcologyState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(
                target,
                self,
                [biome, tree_cover, ground_cover, disturbance, soil_fertility,]
            );
        }
    }
}

impl DomesticatesUndoState {
    pub fn from_diff(
        before: &world::DomesticatesState,
        after: &world::DomesticatesState,
    ) -> Option<Self> {
        if before == after {
            return None;
        }
        let crop_available = build_sparse_patch(&before.crop_available, &after.crop_available);
        let crop_adoption = build_sparse_patch(&before.crop_adoption, &after.crop_adoption);
        let livestock_available =
            build_sparse_patch(&before.livestock_available, &after.livestock_available);
        let livestock_adoption =
            build_sparse_patch(&before.livestock_adoption, &after.livestock_adoption);
        let domesticates_internal =
            build_sparse_patch(&before.domesticates_internal, &after.domesticates_internal);
        let has_sparse_patch = any_patch!(
            crop_available,
            crop_adoption,
            livestock_available,
            livestock_adoption,
            domesticates_internal,
        );
        if !has_sparse_patch {
            return None;
        }
        Some(Self {
            full: None,
            crop_available,
            crop_adoption,
            livestock_available,
            livestock_adoption,
            domesticates_internal,
        })
    }

    pub fn apply_to(&self, target: &mut world::DomesticatesState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(
                target,
                self,
                [
                    crop_available,
                    crop_adoption,
                    livestock_available,
                    livestock_adoption,
                    domesticates_internal,
                ]
            );
        }
    }
}

impl SubsistenceUndoState {
    pub fn from_diff(
        before: &world::SubsistenceState,
        after: &world::SubsistenceState,
    ) -> Option<Self> {
        if before == after {
            return None;
        }
        let subsistence_mix = build_sparse_patch(&before.subsistence_mix, &after.subsistence_mix);
        let food_energy_mean =
            build_sparse_patch(&before.food_energy_mean, &after.food_energy_mean);
        let food_energy_variance =
            build_sparse_patch(&before.food_energy_variance, &after.food_energy_variance);
        let buffer_capacity = build_sparse_patch(&before.buffer_capacity, &after.buffer_capacity);
        let mobility_capacity =
            build_sparse_patch(&before.mobility_capacity, &after.mobility_capacity);
        let land_use_intensity =
            build_sparse_patch(&before.land_use_intensity, &after.land_use_intensity);
        let has_sparse_patch = any_patch!(
            subsistence_mix,
            food_energy_mean,
            food_energy_variance,
            buffer_capacity,
            mobility_capacity,
            land_use_intensity,
        );
        if !has_sparse_patch {
            return None;
        }
        Some(Self {
            full: None,
            subsistence_mix,
            food_energy_mean,
            food_energy_variance,
            buffer_capacity,
            mobility_capacity,
            land_use_intensity,
        })
    }

    pub fn apply_to(&self, target: &mut world::SubsistenceState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(
                target,
                self,
                [
                    subsistence_mix,
                    food_energy_mean,
                    food_energy_variance,
                    buffer_capacity,
                    mobility_capacity,
                    land_use_intensity,
                ]
            );
        }
    }
}

impl PopulationUndoState {
    pub fn from_diff(
        before: &world::PopulationState,
        after: &world::PopulationState,
    ) -> Option<Self> {
        if before == after {
            return None;
        }
        let population = build_sparse_patch(&before.population, &after.population);
        let birth_rate = build_sparse_patch(&before.birth_rate, &after.birth_rate);
        let death_rate = build_sparse_patch(&before.death_rate, &after.death_rate);
        let has_sparse_patch = any_patch!(population, birth_rate, death_rate,);
        if !has_sparse_patch {
            return None;
        }
        Some(Self {
            full: None,
            population,
            birth_rate,
            death_rate,
        })
    }

    pub fn apply_to(&self, target: &mut world::PopulationState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(target, self, [population, birth_rate, death_rate,]);
        }
    }
}

impl SettlementUndoState {
    pub fn from_diff(
        before: &world::SettlementState,
        after: &world::SettlementState,
    ) -> Option<Self> {
        let urbanization = build_sparse_patch(&before.urbanization, &after.urbanization);
        urbanization.map(|urbanization| Self {
            full: None,
            urbanization: Some(urbanization),
        })
    }

    pub fn apply_to(&self, target: &mut world::SettlementState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(target, self, [urbanization,]);
        }
    }
}

impl PolityUndoState {
    pub fn from_diff(before: &world::PolityState, after: &world::PolityState) -> Option<Self> {
        let polity_id = build_sparse_patch(&before.polity_id, &after.polity_id);
        polity_id.map(|polity_id| Self {
            full: None,
            polity_id: Some(polity_id),
        })
    }

    pub fn apply_to(&self, target: &mut world::PolityState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(target, self, [polity_id,]);
        }
    }
}

impl ConflictUndoState {
    pub fn from_diff(before: &world::ConflictState, after: &world::ConflictState) -> Option<Self> {
        if before == after {
            return None;
        }
        let conflict_intensity =
            build_sparse_patch(&before.conflict_intensity, &after.conflict_intensity);
        let occupier_id = build_sparse_patch(&before.occupier_id, &after.occupier_id);
        let has_sparse_patch = any_patch!(conflict_intensity, occupier_id,);
        if !has_sparse_patch {
            return None;
        }
        Some(Self {
            full: None,
            conflict_intensity,
            occupier_id,
        })
    }

    pub fn apply_to(&self, target: &mut world::ConflictState) {
        if let Some(full) = &self.full {
            *target = full.clone();
        } else {
            apply_sparse_fields!(target, self, [conflict_intensity, occupier_id,]);
        }
    }
}

impl ClockUndoState {
    pub fn from_diff(before: &world::ClockState, after: &world::ClockState) -> Option<Self> {
        if before == after {
            return None;
        }
        Some(Self {
            tick: (before.tick != after.tick).then_some(before.tick),
            epoch: (before.epoch != after.epoch).then_some(before.epoch),
            real_years_per_tick: (before.real_years_per_tick != after.real_years_per_tick)
                .then_some(before.real_years_per_tick),
            runtime_tick_ms: (before.runtime_tick_ms != after.runtime_tick_ms)
                .then_some(before.runtime_tick_ms),
            budgets: (before.budgets != after.budgets).then_some(before.budgets),
            transition: (before.transition != after.transition)
                .then_some(before.transition.clone()),
        })
    }

    pub fn apply_to(&self, target: &mut world::ClockState) {
        if let Some(value) = self.tick {
            target.tick = value;
        }
        if let Some(value) = self.epoch {
            target.epoch = value;
        }
        if let Some(value) = self.real_years_per_tick {
            target.real_years_per_tick = value;
        }
        if let Some(value) = self.runtime_tick_ms {
            target.runtime_tick_ms = value;
        }
        if let Some(value) = self.budgets {
            target.budgets = value;
        }
        if let Some(value) = &self.transition {
            target.transition = value.clone();
        }
    }
}

impl ControlUndoState {
    pub fn from_diff(
        before: &world::WorldControlState,
        after: &world::WorldControlState,
    ) -> Option<Self> {
        if before == after {
            return None;
        }
        if before.geology_params != after.geology_params {
            return Some(Self {
                full: Some(before.clone()),
                sea_level_offset: None,
                erosion_thickness_coupling: None,
                deposition_thickness_coupling: None,
                ocean_water_inventory: None,
                ocean_water_inventory_baseline: None,
                ice_inventory: None,
                marine_sediment_mass: None,
                global_sediment_export: None,
                solid_earth_mass_proxy: None,
                solid_earth_mass_proxy_baseline: None,
            });
        }
        Some(Self {
            full: None,
            sea_level_offset: (before.sea_level_offset != after.sea_level_offset)
                .then_some(before.sea_level_offset),
            erosion_thickness_coupling: (before.erosion_thickness_coupling
                != after.erosion_thickness_coupling)
                .then_some(before.erosion_thickness_coupling),
            deposition_thickness_coupling: (before.deposition_thickness_coupling
                != after.deposition_thickness_coupling)
                .then_some(before.deposition_thickness_coupling),
            ocean_water_inventory: (before.ocean_water_inventory != after.ocean_water_inventory)
                .then_some(before.ocean_water_inventory),
            ocean_water_inventory_baseline: (before.ocean_water_inventory_baseline
                != after.ocean_water_inventory_baseline)
                .then_some(before.ocean_water_inventory_baseline),
            ice_inventory: (before.ice_inventory != after.ice_inventory)
                .then_some(before.ice_inventory),
            marine_sediment_mass: (before.marine_sediment_mass != after.marine_sediment_mass)
                .then_some(before.marine_sediment_mass),
            global_sediment_export: (before.global_sediment_export != after.global_sediment_export)
                .then_some(before.global_sediment_export),
            solid_earth_mass_proxy: (before.solid_earth_mass_proxy != after.solid_earth_mass_proxy)
                .then_some(before.solid_earth_mass_proxy),
            solid_earth_mass_proxy_baseline: (before.solid_earth_mass_proxy_baseline
                != after.solid_earth_mass_proxy_baseline)
                .then_some(before.solid_earth_mass_proxy_baseline),
        })
    }

    pub fn apply_to(&self, target: &mut world::WorldControlState) {
        if let Some(full) = &self.full {
            *target = full.clone();
            return;
        }
        if let Some(value) = self.sea_level_offset {
            target.sea_level_offset = value;
        }
        if let Some(value) = self.erosion_thickness_coupling {
            target.erosion_thickness_coupling = value;
        }
        if let Some(value) = self.deposition_thickness_coupling {
            target.deposition_thickness_coupling = value;
        }
        if let Some(value) = self.ocean_water_inventory {
            target.ocean_water_inventory = value;
        }
        if let Some(value) = self.ocean_water_inventory_baseline {
            target.ocean_water_inventory_baseline = value;
        }
        if let Some(value) = self.ice_inventory {
            target.ice_inventory = value;
        }
        if let Some(value) = self.marine_sediment_mass {
            target.marine_sediment_mass = value;
        }
        if let Some(value) = self.global_sediment_export {
            target.global_sediment_export = value;
        }
        if let Some(value) = self.solid_earth_mass_proxy {
            target.solid_earth_mass_proxy = value;
        }
        if let Some(value) = self.solid_earth_mass_proxy_baseline {
            target.solid_earth_mass_proxy_baseline = value;
        }
    }
}

impl EntityUndoState {
    pub fn from_diff(before: &world::EntityState, after: &world::EntityState) -> Option<Self> {
        let before_polities = before
            .iter_polities()
            .cloned()
            .map(|record| (record.id, record))
            .collect::<BTreeMap<_, _>>();
        let after_polities = after
            .iter_polities()
            .cloned()
            .map(|record| (record.id, record))
            .collect::<BTreeMap<_, _>>();
        let before_settlements = before
            .iter_settlements()
            .cloned()
            .map(|record| (record.id, record))
            .collect::<BTreeMap<_, _>>();
        let after_settlements = after
            .iter_settlements()
            .cloned()
            .map(|record| (record.id, record))
            .collect::<BTreeMap<_, _>>();
        let before_regions = before
            .iter_regions()
            .cloned()
            .map(|record| (record.id, record))
            .collect::<BTreeMap<_, _>>();
        let after_regions = after
            .iter_regions()
            .cloned()
            .map(|record| (record.id, record))
            .collect::<BTreeMap<_, _>>();

        let mut undo = Self::default();

        for (id, before_record) in &before_polities {
            match after_polities.get(id) {
                Some(after_record) if after_record == before_record => {}
                _ => undo.polity_upserts.push(before_record.clone()),
            }
        }
        for id in after_polities.keys() {
            if !before_polities.contains_key(id) {
                undo.polity_removals.push(*id);
            }
        }

        for (id, before_record) in &before_settlements {
            match after_settlements.get(id) {
                Some(after_record) if after_record == before_record => {}
                _ => undo.settlement_upserts.push(before_record.clone()),
            }
        }
        for id in after_settlements.keys() {
            if !before_settlements.contains_key(id) {
                undo.settlement_removals.push(*id);
            }
        }

        for (id, before_record) in &before_regions {
            match after_regions.get(id) {
                Some(after_record) if after_record == before_record => {}
                _ => undo.region_upserts.push(before_record.clone()),
            }
        }
        for id in after_regions.keys() {
            if !before_regions.contains_key(id) {
                undo.region_removals.push(*id);
            }
        }

        if undo.polity_upserts.is_empty()
            && undo.polity_removals.is_empty()
            && undo.settlement_upserts.is_empty()
            && undo.settlement_removals.is_empty()
            && undo.region_upserts.is_empty()
            && undo.region_removals.is_empty()
        {
            None
        } else {
            Some(undo)
        }
    }

    pub fn apply_to(&self, target: &mut world::EntityState) {
        for id in &self.polity_removals {
            let _ = target.remove_polity(*id);
        }
        for record in &self.polity_upserts {
            let _ = target.remove_polity(record.id);
            let _ = target.create_polity(record.clone());
        }
        for id in &self.settlement_removals {
            let _ = target.remove_settlement(*id);
        }
        for record in &self.settlement_upserts {
            let _ = target.remove_settlement(record.id);
            let _ = target.create_settlement(record.clone());
        }
        for id in &self.region_removals {
            let _ = target.remove_region(*id);
        }
        for record in &self.region_upserts {
            let _ = target.remove_region(record.id);
            let _ = target.create_region(record.clone());
        }
    }

    pub fn estimated_bytes(&self) -> usize {
        self.polity_upserts
            .iter()
            .map(|record| size_of::<world::PolityRecord>() + vec_bytes(&record.cells_cache))
            .sum::<usize>()
            + self.polity_removals.len() * size_of::<world::PolityId>()
            + self.settlement_upserts.len() * size_of::<world::SettlementRecord>()
            + self.settlement_removals.len() * size_of::<world::SettlementId>()
            + self
                .region_upserts
                .iter()
                .map(|record| size_of::<world::RegionRecord>() + vec_bytes(&record.cells))
                .sum::<usize>()
            + self.region_removals.len() * size_of::<world::RegionId>()
    }
}

impl PolityGroupsUndoState {
    pub fn from_diff(before: &[PolityGroup], after: &[PolityGroup]) -> Option<Self> {
        if before == after {
            return None;
        }

        let before_groups = before
            .iter()
            .cloned()
            .map(|group| (group.id, group))
            .collect::<BTreeMap<_, _>>();
        let after_groups = after
            .iter()
            .cloned()
            .map(|group| (group.id, group))
            .collect::<BTreeMap<_, _>>();

        let mut undo = Self {
            order_before: before.iter().map(|group| group.id).collect(),
            ..Self::default()
        };

        for (id, before_group) in &before_groups {
            match after_groups.get(id) {
                Some(after_group) if after_group == before_group => {}
                _ => undo.upserts.push(before_group.clone()),
            }
        }
        for id in after_groups.keys() {
            if !before_groups.contains_key(id) {
                undo.removals.push(*id);
            }
        }

        Some(undo)
    }

    pub fn apply_to(&self, target: &mut Vec<PolityGroup>) {
        target.retain(|group| !self.removals.contains(&group.id));
        for group in &self.upserts {
            if let Some(slot) = target.iter_mut().find(|existing| existing.id == group.id) {
                *slot = group.clone();
            } else {
                target.push(group.clone());
            }
        }

        let mut groups_by_id = target
            .drain(..)
            .map(|group| (group.id, group))
            .collect::<BTreeMap<_, _>>();
        let mut reordered = Vec::with_capacity(self.order_before.len());
        for id in &self.order_before {
            if let Some(group) = groups_by_id.remove(id) {
                reordered.push(group);
            }
        }
        reordered.extend(groups_by_id.into_values());
        *target = reordered;
    }

    pub fn estimated_bytes(&self) -> usize {
        self.upserts
            .iter()
            .map(estimate_polity_group_bytes)
            .sum::<usize>()
            + self.removals.len() * size_of::<world::PolityGroupId>()
            + self.order_before.len() * size_of::<world::PolityGroupId>()
    }
}

impl RelationsUndoState {
    pub fn from_diff(
        before: &world::WorldRelations,
        after: &world::WorldRelations,
    ) -> Option<Self> {
        let polity_relations =
            build_map_before_value_patch(&before.polity_relations, &after.polity_relations);
        let polity_groups =
            PolityGroupsUndoState::from_diff(&before.polity_groups, &after.polity_groups);
        let plate_relations =
            build_map_before_value_patch(&before.plate_relations, &after.plate_relations);
        if polity_relations.is_none() && polity_groups.is_none() && plate_relations.is_none() {
            None
        } else {
            Some(Self {
                polity_relations,
                polity_groups,
                plate_relations,
            })
        }
    }

    pub fn apply_to(&self, target: &mut world::WorldRelations) {
        if let Some(patch) = &self.polity_relations {
            apply_map_before_value_patch(&mut target.polity_relations, patch);
        }
        if let Some(groups) = &self.polity_groups {
            groups.apply_to(&mut target.polity_groups);
        }
        if let Some(patch) = &self.plate_relations {
            apply_map_before_value_patch(&mut target.plate_relations, patch);
        }
    }

    pub fn estimated_bytes(&self) -> usize {
        self.polity_relations
            .as_ref()
            .map(|patch| {
                estimate_map_before_value_patch_bytes(patch, |_| size_of::<PolityRelation>())
            })
            .unwrap_or(0)
            + self
                .polity_groups
                .as_ref()
                .map(PolityGroupsUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .plate_relations
                .as_ref()
                .map(|patch| {
                    estimate_map_before_value_patch_bytes(patch, |_| size_of::<PlateRelation>())
                })
                .unwrap_or(0)
    }
}

macro_rules! estimate_optional_sparse_patch_bytes {
    ($state:expr, [$($field:ident),+ $(,)?]) => {
        0usize $(+ $state.$field.as_ref().map(estimate_sparse_patch_bytes).unwrap_or(0))+
    };
}

#[derive(Clone, Default)]
pub(crate) struct GeologyUndoState {
    pub full: Option<world::GeologyState>,
    pub height: Option<SparseF32Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct ClimateUndoState {
    pub full: Option<world::ClimateState>,
    pub temperature: Option<SparseF32Patch>,
    pub precipitation: Option<SparseF32Patch>,
    pub evapotranspiration: Option<SparseF32Patch>,
    pub runoff: Option<SparseF32Patch>,
    pub aridity: Option<SparseF32Patch>,
    pub ocean_temperature: Option<SparseF32Patch>,
    pub precipitable_water: Option<SparseF32Patch>,
    pub cloud_water: Option<SparseF32Patch>,
    pub wind_u: Option<SparseF32Patch>,
    pub wind_v: Option<SparseF32Patch>,
    pub moisture_flux_u: Option<SparseF32Patch>,
    pub moisture_flux_v: Option<SparseF32Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct GlaciologyUndoState {
    pub full: Option<world::GlaciologyState>,
    pub ice_thickness: Option<SparseF32Patch>,
    pub ice_load: Option<SparseF32Patch>,
    pub accumulation: Option<SparseF32Patch>,
    pub ablation: Option<SparseF32Patch>,
    pub isostatic_adjustment: Option<SparseF32Patch>,
    pub applied_isostatic_adjustment: Option<SparseF32Patch>,
    pub glacial_erosion_rate: Option<SparseF32Patch>,
    pub glacial_melt_runoff: Option<SparseF32Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct HydrologyUndoState {
    pub full: Option<world::HydrologyState>,
    pub river_downstream: Option<CompactRiverDownstreamPatch>,
    pub river_flow: Option<SparseF32Patch>,
    pub river_next: Option<SparseI32Patch>,
    pub erosion_rate: Option<SparseF32Patch>,
    pub deposition_rate: Option<SparseF32Patch>,
    pub river_transport_cost: Option<SparseF32Patch>,
    pub is_lake: Option<SparseBoolPatch>,
    pub sink_id: Option<SparseI32Patch>,
    pub sink_route_next: Option<SparseI32Patch>,
    pub sink_member_offsets: Option<SparseU32Patch>,
    pub sink_member_cells: Option<SparseU32Patch>,
    pub sink_spill_cell: Option<SparseI32Patch>,
    pub sink_spill_to: Option<SparseI32Patch>,
    pub sink_spill_level: Option<SparseF32Patch>,
    pub sink_capacity_total: Option<SparseF32Patch>,
    pub sink_capacity_remaining: Option<SparseF32Patch>,
    pub sink_storage_water: Option<SparseF32Patch>,
    pub sink_storage_sediment: Option<SparseF32Patch>,
    pub sink_overflow_active: Option<SparseU8Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct EcologyUndoState {
    pub full: Option<world::EcologyState>,
    pub biome: Option<SparseBiomePatch>,
    pub tree_cover: Option<SparseF32Patch>,
    pub ground_cover: Option<SparseF32Patch>,
    pub disturbance: Option<SparseF32Patch>,
    pub soil_fertility: Option<SparseF32Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct DomesticatesUndoState {
    pub full: Option<world::DomesticatesState>,
    pub crop_available: Option<SparseU8Patch>,
    pub crop_adoption: Option<SparsePatch<[f32; world::N_CROPS]>>,
    pub livestock_available: Option<SparseU8Patch>,
    pub livestock_adoption: Option<SparsePatch<[f32; world::N_LIVESTOCK]>>,
    pub domesticates_internal: Option<SparsePatch<world::DomesticatesInternal>>,
}

#[derive(Clone, Default)]
pub(crate) struct SubsistenceUndoState {
    pub full: Option<world::SubsistenceState>,
    pub subsistence_mix: Option<SparsePatch<world::SubsistenceMix>>,
    pub food_energy_mean: Option<SparseF32Patch>,
    pub food_energy_variance: Option<SparseF32Patch>,
    pub buffer_capacity: Option<SparseF32Patch>,
    pub mobility_capacity: Option<SparseF32Patch>,
    pub land_use_intensity: Option<SparseF32Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct PopulationUndoState {
    pub full: Option<world::PopulationState>,
    pub population: Option<SparseF32Patch>,
    pub birth_rate: Option<SparseF32Patch>,
    pub death_rate: Option<SparseF32Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct SettlementUndoState {
    pub full: Option<world::SettlementState>,
    pub urbanization: Option<SparseF32Patch>,
}

#[derive(Clone, Default)]
pub(crate) struct PolityUndoState {
    pub full: Option<world::PolityState>,
    pub polity_id: Option<SparsePatch<Option<world::PolityId>>>,
}

#[derive(Clone, Default)]
pub(crate) struct ConflictUndoState {
    pub full: Option<world::ConflictState>,
    pub conflict_intensity: Option<SparseF32Patch>,
    pub occupier_id: Option<SparsePatch<Option<world::PolityId>>>,
}

#[derive(Clone, Default)]
pub(crate) struct ClockUndoState {
    pub tick: Option<u64>,
    pub epoch: Option<crate::sim::world::EraKind>,
    pub real_years_per_tick: Option<f32>,
    pub runtime_tick_ms: Option<u32>,
    pub budgets: Option<crate::sim::world::SubsystemBudgets>,
    pub transition: Option<crate::sim::world::TransitionState>,
}

#[derive(Clone, Default)]
pub(crate) struct ControlUndoState {
    pub full: Option<world::WorldControlState>,
    pub sea_level_offset: Option<f32>,
    pub erosion_thickness_coupling: Option<f32>,
    pub deposition_thickness_coupling: Option<f32>,
    pub ocean_water_inventory: Option<f32>,
    pub ocean_water_inventory_baseline: Option<f32>,
    pub ice_inventory: Option<f32>,
    pub marine_sediment_mass: Option<f32>,
    pub global_sediment_export: Option<f32>,
    pub solid_earth_mass_proxy: Option<f32>,
    pub solid_earth_mass_proxy_baseline: Option<f32>,
}

#[derive(Clone, Default)]
pub(crate) struct EntityUndoState {
    pub polity_upserts: Vec<world::PolityRecord>,
    pub polity_removals: Vec<world::PolityId>,
    pub settlement_upserts: Vec<world::SettlementRecord>,
    pub settlement_removals: Vec<world::SettlementId>,
    pub region_upserts: Vec<world::RegionRecord>,
    pub region_removals: Vec<world::RegionId>,
}

#[derive(Clone, Default)]
pub(crate) struct MapBeforeValuePatch<K, V> {
    pub entries: Vec<(K, Option<V>)>,
}

#[derive(Clone, Default)]
pub(crate) struct PolityGroupsUndoState {
    pub upserts: Vec<PolityGroup>,
    pub removals: Vec<world::PolityGroupId>,
    pub order_before: Vec<world::PolityGroupId>,
}

#[derive(Clone, Default)]
pub(crate) struct RelationsUndoState {
    pub polity_relations:
        Option<MapBeforeValuePatch<(world::PolityId, world::PolityId), PolityRelation>>,
    pub polity_groups: Option<PolityGroupsUndoState>,
    pub plate_relations: Option<MapBeforeValuePatch<(PlateId, PlateId), PlateRelation>>,
}

#[derive(Clone, Default)]
pub(crate) struct WorldCoreChangeSet {
    pub geology: Option<GeologyUndoState>,
    pub climate: Option<ClimateUndoState>,
    pub glaciology: Option<GlaciologyUndoState>,
    pub hydrology: Option<HydrologyUndoState>,
    pub ecology: Option<EcologyUndoState>,
    pub domesticates: Option<DomesticatesUndoState>,
    pub subsistence: Option<SubsistenceUndoState>,
    pub population: Option<PopulationUndoState>,
    pub settlement: Option<SettlementUndoState>,
    pub polity: Option<PolityUndoState>,
    pub conflict: Option<ConflictUndoState>,
    pub entities: Option<EntityUndoState>,
    pub relations: Option<RelationsUndoState>,
    pub clock: Option<ClockUndoState>,
    pub control: Option<ControlUndoState>,
}

impl GeologyUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| vec_bytes(&full.height))
            .unwrap_or(0)
            + self
                .height
                .as_ref()
                .map(estimate_sparse_patch_bytes)
                .unwrap_or(0)
    }
}

impl ClimateUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| {
                vec_bytes(&full.temperature)
                    + vec_bytes(&full.precipitation)
                    + vec_bytes(&full.evapotranspiration)
                    + vec_bytes(&full.runoff)
                    + vec_bytes(&full.aridity)
                    + vec_bytes(&full.ocean_temperature)
                    + vec_bytes(&full.precipitable_water)
                    + vec_bytes(&full.cloud_water)
                    + vec_bytes(&full.wind_u)
                    + vec_bytes(&full.wind_v)
                    + vec_bytes(&full.moisture_flux_u)
                    + vec_bytes(&full.moisture_flux_v)
            })
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(
                self,
                [
                    temperature,
                    precipitation,
                    evapotranspiration,
                    runoff,
                    aridity,
                    ocean_temperature,
                    precipitable_water,
                    cloud_water,
                    wind_u,
                    wind_v,
                    moisture_flux_u,
                    moisture_flux_v,
                ]
            )
    }
}

impl GlaciologyUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| {
                vec_bytes(&full.ice_thickness)
                    + vec_bytes(&full.ice_load)
                    + vec_bytes(&full.accumulation)
                    + vec_bytes(&full.ablation)
                    + vec_bytes(&full.isostatic_adjustment)
                    + vec_bytes(&full.applied_isostatic_adjustment)
                    + vec_bytes(&full.glacial_erosion_rate)
                    + vec_bytes(&full.glacial_melt_runoff)
            })
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(
                self,
                [
                    ice_thickness,
                    ice_load,
                    accumulation,
                    ablation,
                    isostatic_adjustment,
                    applied_isostatic_adjustment,
                    glacial_erosion_rate,
                    glacial_melt_runoff,
                ]
            )
    }
}

impl HydrologyUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| {
                river_downstream_bytes(&full.river_downstream)
                    + vec_bytes(&full.river_next)
                    + vec_bytes(&full.river_flow)
                    + vec_bytes(&full.erosion_rate)
                    + vec_bytes(&full.deposition_rate)
                    + vec_bytes(&full.river_transport_cost)
                    + vec_bytes(&full.is_lake)
                    + vec_bytes(&full.sink_id)
                    + vec_bytes(&full.sink_route_next)
                    + vec_bytes(&full.sink_member_offsets)
                    + vec_bytes(&full.sink_member_cells)
                    + vec_bytes(&full.sink_spill_cell)
                    + vec_bytes(&full.sink_spill_to)
                    + vec_bytes(&full.sink_spill_level)
                    + vec_bytes(&full.sink_capacity_total)
                    + vec_bytes(&full.sink_capacity_remaining)
                    + vec_bytes(&full.sink_storage_water)
                    + vec_bytes(&full.sink_storage_sediment)
                    + vec_bytes(&full.sink_overflow_active)
            })
            .unwrap_or(0)
            + self
                .river_downstream
                .as_ref()
                .map(CompactRiverDownstreamPatch::estimated_bytes)
                .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(
                self,
                [
                    river_flow,
                    river_next,
                    erosion_rate,
                    deposition_rate,
                    river_transport_cost,
                    is_lake,
                    sink_id,
                    sink_route_next,
                    sink_member_offsets,
                    sink_member_cells,
                    sink_spill_cell,
                    sink_spill_to,
                    sink_spill_level,
                    sink_capacity_total,
                    sink_capacity_remaining,
                    sink_storage_water,
                    sink_storage_sediment,
                    sink_overflow_active,
                ]
            )
    }
}

impl EcologyUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| {
                vec_bytes(&full.biome)
                    + vec_bytes(&full.tree_cover)
                    + vec_bytes(&full.ground_cover)
                    + vec_bytes(&full.disturbance)
                    + vec_bytes(&full.soil_fertility)
                    + vec_bytes(&full.ecology_internal)
            })
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(
                self,
                [biome, tree_cover, ground_cover, disturbance, soil_fertility,]
            )
    }
}

impl DomesticatesUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| {
                vec_bytes(&full.crop_available)
                    + vec_bytes(&full.crop_adoption)
                    + vec_bytes(&full.livestock_available)
                    + vec_bytes(&full.livestock_adoption)
                    + vec_bytes(&full.domesticates_internal)
            })
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(
                self,
                [
                    crop_available,
                    crop_adoption,
                    livestock_available,
                    livestock_adoption,
                    domesticates_internal,
                ]
            )
    }
}

impl SubsistenceUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| {
                vec_bytes(&full.subsistence_mix)
                    + vec_bytes(&full.food_energy_mean)
                    + vec_bytes(&full.food_energy_variance)
                    + vec_bytes(&full.buffer_capacity)
                    + vec_bytes(&full.mobility_capacity)
                    + vec_bytes(&full.land_use_intensity)
            })
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(
                self,
                [
                    subsistence_mix,
                    food_energy_mean,
                    food_energy_variance,
                    buffer_capacity,
                    mobility_capacity,
                    land_use_intensity,
                ]
            )
    }
}

impl PopulationUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| {
                vec_bytes(&full.population)
                    + vec_bytes(&full.birth_rate)
                    + vec_bytes(&full.death_rate)
            })
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(self, [population, birth_rate, death_rate,])
    }
}

impl SettlementUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| vec_bytes(&full.urbanization))
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(self, [urbanization,])
    }
}

impl PolityUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| vec_bytes(&full.polity_id))
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(self, [polity_id,])
    }
}

impl ConflictUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|full| vec_bytes(&full.conflict_intensity) + vec_bytes(&full.occupier_id))
            .unwrap_or(0)
            + estimate_optional_sparse_patch_bytes!(self, [conflict_intensity, occupier_id,])
    }
}

impl ClockUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.tick.as_ref().map(|_| size_of::<u64>()).unwrap_or(0)
            + self
                .epoch
                .as_ref()
                .map(|_| size_of::<world::EraKind>())
                .unwrap_or(0)
            + self
                .real_years_per_tick
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .runtime_tick_ms
                .as_ref()
                .map(|_| size_of::<u32>())
                .unwrap_or(0)
            + self
                .budgets
                .as_ref()
                .map(|_| size_of::<world::SubsystemBudgets>())
                .unwrap_or(0)
            + self
                .transition
                .as_ref()
                .map(|_| size_of::<world::TransitionState>())
                .unwrap_or(0)
    }
}

impl ControlUndoState {
    pub fn estimated_bytes(&self) -> usize {
        self.full
            .as_ref()
            .map(|_| size_of::<world::WorldControlState>())
            .unwrap_or(0)
            + self
                .sea_level_offset
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .erosion_thickness_coupling
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .deposition_thickness_coupling
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .ocean_water_inventory
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .ocean_water_inventory_baseline
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .ice_inventory
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .marine_sediment_mass
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .global_sediment_export
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .solid_earth_mass_proxy
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
            + self
                .solid_earth_mass_proxy_baseline
                .as_ref()
                .map(|_| size_of::<f32>())
                .unwrap_or(0)
    }
}

impl WorldCoreChangeSet {
    pub fn estimated_bytes(&self) -> usize {
        self.geology
            .as_ref()
            .map(GeologyUndoState::estimated_bytes)
            .unwrap_or(0)
            + self
                .climate
                .as_ref()
                .map(ClimateUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .glaciology
                .as_ref()
                .map(GlaciologyUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .hydrology
                .as_ref()
                .map(HydrologyUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .ecology
                .as_ref()
                .map(EcologyUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .domesticates
                .as_ref()
                .map(DomesticatesUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .subsistence
                .as_ref()
                .map(SubsistenceUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .population
                .as_ref()
                .map(PopulationUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .settlement
                .as_ref()
                .map(SettlementUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .polity
                .as_ref()
                .map(PolityUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .conflict
                .as_ref()
                .map(ConflictUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .entities
                .as_ref()
                .map(EntityUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .relations
                .as_ref()
                .map(RelationsUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .clock
                .as_ref()
                .map(ClockUndoState::estimated_bytes)
                .unwrap_or(0)
            + self
                .control
                .as_ref()
                .map(ControlUndoState::estimated_bytes)
                .unwrap_or(0)
    }
}

#[derive(Clone, Default)]
pub(crate) struct TimelineRuntime {
    pub archive: TimelineArchive,
    pub undo_logs: BTreeMap<u64, TickUndoLog>,
    pub cursor: TimelineCursor,
    pub retention: TimelineRetentionPolicy,
}

#[allow(dead_code)]
pub(crate) type WorldArchive = TimelineArchive;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManagedWorldExecState {
    pub next_phase: ExecWorldPhase,
    pub remaining_steps: u32,
}

impl Default for ManagedWorldExecState {
    fn default() -> Self {
        Self {
            next_phase: first_phase(),
            remaining_steps: 0,
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

    pub fn observe_with<F>(&mut self, len: usize, mut value_at: F)
    where
        F: FnMut(usize) -> f32,
    {
        if self.shadow.len() != len {
            self.shadow.clear();
            self.shadow.reserve(len);
            for index in 0..len {
                self.shadow.push(value_at(index));
            }
            self.force_full = true;
            self.dirty_ranges.clear();
            self.dirty_bitmap = vec![0; bitmap_word_len(len)];
            return;
        }

        let mut changed = 0usize;
        let mut range_start: Option<usize> = None;
        for index in 0..len {
            let value = value_at(index);
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
            self.merge_dirty_range(start, len);
        }
        if changed > 0 && (changed as f32) >= (len as f32) * DELTA_FULL_THRESHOLD_RATIO {
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

    pub fn observe_with<F>(&mut self, len: usize, mut value_at: F)
    where
        F: FnMut(usize) -> i32,
    {
        if self.shadow.len() != len {
            self.shadow.clear();
            self.shadow.reserve(len);
            for index in 0..len {
                self.shadow.push(value_at(index));
            }
            self.force_full = true;
            self.dirty_ranges.clear();
            self.dirty_bitmap = vec![0; bitmap_word_len(len)];
            return;
        }

        let mut changed = 0usize;
        let mut range_start: Option<usize> = None;
        for index in 0..len {
            let value = value_at(index);
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
            self.merge_dirty_range(start, len);
        }
        if changed > 0 && (changed as f32) >= (len as f32) * DELTA_FULL_THRESHOLD_RATIO {
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

    pub fn observe_with<F>(&mut self, len: usize, mut value_at: F)
    where
        F: FnMut(usize) -> u32,
    {
        if self.shadow.len() != len {
            self.shadow.clear();
            self.shadow.reserve(len);
            for index in 0..len {
                self.shadow.push(value_at(index));
            }
            self.force_full = true;
            self.dirty_ranges.clear();
            self.dirty_bitmap = vec![0; bitmap_word_len(len)];
            return;
        }

        let mut changed = 0usize;
        let mut range_start: Option<usize> = None;
        for index in 0..len {
            let value = value_at(index);
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
            self.merge_dirty_range(start, len);
        }
        if changed > 0 && (changed as f32) >= (len as f32) * DELTA_FULL_THRESHOLD_RATIO {
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

fn collect_crop_adoption_for_kind(world: &world::World, kind_index: usize) -> Vec<f32> {
    world
        .state
        .domesticates
        .crop_adoption
        .iter()
        .map(|values| values.get(kind_index).copied().unwrap_or(0.0))
        .collect()
}

fn collect_crop_available_for_kind(world: &world::World, kind_index: usize) -> Vec<f32> {
    let mask = 1u8 << kind_index;
    world
        .state
        .domesticates
        .crop_available
        .iter()
        .map(|value| if (*value & mask) != 0 { 1.0 } else { 0.0 })
        .collect()
}

fn collect_livestock_adoption_for_kind(world: &world::World, kind_index: usize) -> Vec<f32> {
    world
        .state
        .domesticates
        .livestock_adoption
        .iter()
        .map(|values| values.get(kind_index).copied().unwrap_or(0.0))
        .collect()
}

fn collect_livestock_available_for_kind(world: &world::World, kind_index: usize) -> Vec<f32> {
    let mask = 1u8 << kind_index;
    world
        .state
        .domesticates
        .livestock_available
        .iter()
        .map(|value| if (*value & mask) != 0 { 1.0 } else { 0.0 })
        .collect()
}

fn collect_biome_codes(world: &world::World) -> Vec<i32> {
    world
        .state
        .ecology
        .biome
        .iter()
        .copied()
        .map(|biome| match biome {
            world::Biome::TropicalForest => 0,
            world::Biome::Savanna => 1,
            world::Biome::Desert => 2,
            world::Biome::Grassland => 3,
            world::Biome::TemperateForest => 4,
            world::Biome::BorealForest => 5,
            world::Biome::Tundra => 6,
            world::Biome::Wetland => 7,
            world::Biome::Alpine => 8,
        })
        .collect()
}

fn biome_to_code(biome: world::Biome) -> i32 {
    match biome {
        world::Biome::TropicalForest => 0,
        world::Biome::Savanna => 1,
        world::Biome::Desert => 2,
        world::Biome::Grassland => 3,
        world::Biome::TemperateForest => 4,
        world::Biome::BorealForest => 5,
        world::Biome::Tundra => 6,
        world::Biome::Wetland => 7,
        world::Biome::Alpine => 8,
    }
}

impl TimelineViewCache {
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
            erosion_rate: F32FieldTracker::new(&world.state.hydrology.erosion_rate),
            deposition_rate: F32FieldTracker::new(&world.state.hydrology.deposition_rate),
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
            biome: I32FieldTracker::new(&collect_biome_codes(world)),
            river_transport_cost: F32FieldTracker::new(&world.state.hydrology.river_transport_cost),
            crop_adoption_wheat: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 0)),
            crop_adoption_rice: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 1)),
            crop_adoption_maize: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 2)),
            crop_adoption_millet: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 3)),
            crop_adoption_potato: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 4)),
            crop_adoption_cassava: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 5)),
            crop_adoption_sorghum: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 6)),
            crop_adoption_yam: F32FieldTracker::new(&collect_crop_adoption_for_kind(world, 7)),
            crop_available_wheat: F32FieldTracker::new(&collect_crop_available_for_kind(world, 0)),
            crop_available_rice: F32FieldTracker::new(&collect_crop_available_for_kind(world, 1)),
            crop_available_maize: F32FieldTracker::new(&collect_crop_available_for_kind(world, 2)),
            crop_available_millet: F32FieldTracker::new(&collect_crop_available_for_kind(world, 3)),
            crop_available_potato: F32FieldTracker::new(&collect_crop_available_for_kind(world, 4)),
            crop_available_cassava: F32FieldTracker::new(&collect_crop_available_for_kind(
                world, 5,
            )),
            crop_available_sorghum: F32FieldTracker::new(&collect_crop_available_for_kind(
                world, 6,
            )),
            crop_available_yam: F32FieldTracker::new(&collect_crop_available_for_kind(world, 7)),
            livestock_adoption_cattle: F32FieldTracker::new(&collect_livestock_adoption_for_kind(
                world, 0,
            )),
            livestock_adoption_horse: F32FieldTracker::new(&collect_livestock_adoption_for_kind(
                world, 1,
            )),
            livestock_adoption_sheep: F32FieldTracker::new(&collect_livestock_adoption_for_kind(
                world, 2,
            )),
            livestock_adoption_pig: F32FieldTracker::new(&collect_livestock_adoption_for_kind(
                world, 3,
            )),
            livestock_adoption_camel: F32FieldTracker::new(&collect_livestock_adoption_for_kind(
                world, 4,
            )),
            livestock_available_cattle: F32FieldTracker::new(
                &collect_livestock_available_for_kind(world, 0),
            ),
            livestock_available_horse: F32FieldTracker::new(&collect_livestock_available_for_kind(
                world, 1,
            )),
            livestock_available_sheep: F32FieldTracker::new(&collect_livestock_available_for_kind(
                world, 2,
            )),
            livestock_available_pig: F32FieldTracker::new(&collect_livestock_available_for_kind(
                world, 3,
            )),
            livestock_available_camel: F32FieldTracker::new(&collect_livestock_available_for_kind(
                world, 4,
            )),
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
        self.plate_id
            .observe_with(world.state.geology.plate_id.len(), |index| {
                world.state.geology.plate_id[index].as_u32()
            });
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
        self.erosion_rate
            .observe(&world.state.hydrology.erosion_rate);
        self.deposition_rate
            .observe(&world.state.hydrology.deposition_rate);
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
        self.biome
            .observe_with(world.state.ecology.biome.len(), |index| {
                biome_to_code(world.state.ecology.biome[index])
            });
        self.river_transport_cost
            .observe(&world.state.hydrology.river_transport_cost);
        let crop_len = world.state.domesticates.crop_adoption.len();
        self.crop_adoption_wheat.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][0]
        });
        self.crop_adoption_rice.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][1]
        });
        self.crop_adoption_maize.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][2]
        });
        self.crop_adoption_millet.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][3]
        });
        self.crop_adoption_potato.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][4]
        });
        self.crop_adoption_cassava.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][5]
        });
        self.crop_adoption_sorghum.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][6]
        });
        self.crop_adoption_yam.observe_with(crop_len, |index| {
            world.state.domesticates.crop_adoption[index][7]
        });

        let crop_available_len = world.state.domesticates.crop_available.len();
        self.crop_available_wheat
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 0)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.crop_available_rice
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 1)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.crop_available_maize
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 2)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.crop_available_millet
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 3)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.crop_available_potato
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 4)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.crop_available_cassava
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 5)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.crop_available_sorghum
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 6)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.crop_available_yam
            .observe_with(crop_available_len, |index| {
                if (world.state.domesticates.crop_available[index] & (1u8 << 7)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });

        let livestock_len = world.state.domesticates.livestock_adoption.len();
        self.livestock_adoption_cattle
            .observe_with(livestock_len, |index| {
                world.state.domesticates.livestock_adoption[index][0]
            });
        self.livestock_adoption_horse
            .observe_with(livestock_len, |index| {
                world.state.domesticates.livestock_adoption[index][1]
            });
        self.livestock_adoption_sheep
            .observe_with(livestock_len, |index| {
                world.state.domesticates.livestock_adoption[index][2]
            });
        self.livestock_adoption_pig
            .observe_with(livestock_len, |index| {
                world.state.domesticates.livestock_adoption[index][3]
            });
        self.livestock_adoption_camel
            .observe_with(livestock_len, |index| {
                world.state.domesticates.livestock_adoption[index][4]
            });

        let livestock_available_len = world.state.domesticates.livestock_available.len();
        self.livestock_available_cattle
            .observe_with(livestock_available_len, |index| {
                if (world.state.domesticates.livestock_available[index] & (1u8 << 0)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.livestock_available_horse
            .observe_with(livestock_available_len, |index| {
                if (world.state.domesticates.livestock_available[index] & (1u8 << 1)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.livestock_available_sheep
            .observe_with(livestock_available_len, |index| {
                if (world.state.domesticates.livestock_available[index] & (1u8 << 2)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.livestock_available_pig
            .observe_with(livestock_available_len, |index| {
                if (world.state.domesticates.livestock_available[index] & (1u8 << 3)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
        self.livestock_available_camel
            .observe_with(livestock_available_len, |index| {
                if (world.state.domesticates.livestock_available[index] & (1u8 << 4)) != 0 {
                    1.0
                } else {
                    0.0
                }
            });
    }

    pub fn observe_world_selected<F>(
        &mut self,
        world: &world::World,
        geology_dynamics: Option<&world::GeologyDynamicsState>,
        mut include_field: F,
    ) where
        F: FnMut(&str) -> bool,
    {
        if include_field("height") {
            self.height.observe(&world.state.geology.height);
        }
        if include_field("lake_depth") {
            self.lake_depth.observe(&world.state.geology.lake_depth);
        }
        if include_field("volcanism") {
            self.volcanism.observe(&world.state.geology.volcanism);
        }
        if include_field("vertex_buoyancy") {
            self.vertex_buoyancy
                .observe(&world.state.geology.vertex_buoyancy);
        }
        if include_field("plate_id") {
            self.plate_id
                .observe_with(world.state.geology.plate_id.len(), |index| {
                    world.state.geology.plate_id[index].as_u32()
                });
        }
        if include_field("river_flux") {
            self.river_flux.observe(&world.state.hydrology.river_flow);
        }
        if include_field("river_next") {
            self.river_next.observe(&world.state.hydrology.river_next);
        }
        if include_field("mantle_heat") {
            let mantle_heat = geology_dynamics
                .map(|dynamics| dynamics.mantle_heat.as_slice())
                .filter(|values| values.len() == world.state.geology.height.len());
            if let Some(values) = mantle_heat {
                self.mantle_heat.observe(values);
            } else {
                let fallback = vec![0.5; world.state.geology.height.len()];
                self.mantle_heat.observe(&fallback);
            }
        }
        if include_field("erosion_rate") {
            self.erosion_rate
                .observe(&world.state.hydrology.erosion_rate);
        }
        if include_field("deposition_rate") {
            self.deposition_rate
                .observe(&world.state.hydrology.deposition_rate);
        }
        if include_field("temperature") {
            self.temperature.observe(&world.state.climate.temperature);
        }
        if include_field("precipitation") {
            self.precipitation
                .observe(&world.state.climate.precipitation);
        }
        if include_field("evapotranspiration") {
            self.evapotranspiration
                .observe(&world.state.climate.evapotranspiration);
        }
        if include_field("aridity") {
            self.aridity.observe(&world.state.climate.aridity);
        }
        if include_field("runoff") {
            self.runoff.observe(&world.state.climate.runoff);
        }
        if include_field("ice_pressure") {
            if world.state.glaciology.ice_load.len() == world.state.geology.height.len() {
                self.ice_pressure.observe(&world.state.glaciology.ice_load);
            } else {
                let fallback = vec![0.0; world.state.geology.height.len()];
                self.ice_pressure.observe(&fallback);
            }
        }
        if include_field("ocean_temperature") {
            self.ocean_temperature
                .observe(&world.state.climate.ocean_temperature);
        }
        if include_field("wind_u") {
            self.wind_u.observe(&world.state.climate.wind_u);
        }
        if include_field("wind_v") {
            self.wind_v.observe(&world.state.climate.wind_v);
        }
        if include_field("moisture_flux_u") {
            self.moisture_flux_u
                .observe(&world.state.climate.moisture_flux_u);
        }
        if include_field("moisture_flux_v") {
            self.moisture_flux_v
                .observe(&world.state.climate.moisture_flux_v);
        }
        if include_field("biome") {
            self.biome
                .observe_with(world.state.ecology.biome.len(), |index| {
                    biome_to_code(world.state.ecology.biome[index])
                });
        }
        if include_field("river_transport_cost") {
            self.river_transport_cost
                .observe(&world.state.hydrology.river_transport_cost);
        }

        let include_any_crop_adoption = include_field("crop_adoption_wheat")
            || include_field("crop_adoption_rice")
            || include_field("crop_adoption_maize")
            || include_field("crop_adoption_millet")
            || include_field("crop_adoption_potato")
            || include_field("crop_adoption_cassava")
            || include_field("crop_adoption_sorghum")
            || include_field("crop_adoption_yam");
        if include_any_crop_adoption {
            let crop_len = world.state.domesticates.crop_adoption.len();
            if include_field("crop_adoption_wheat") {
                self.crop_adoption_wheat.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][0]
                });
            }
            if include_field("crop_adoption_rice") {
                self.crop_adoption_rice.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][1]
                });
            }
            if include_field("crop_adoption_maize") {
                self.crop_adoption_maize.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][2]
                });
            }
            if include_field("crop_adoption_millet") {
                self.crop_adoption_millet.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][3]
                });
            }
            if include_field("crop_adoption_potato") {
                self.crop_adoption_potato.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][4]
                });
            }
            if include_field("crop_adoption_cassava") {
                self.crop_adoption_cassava.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][5]
                });
            }
            if include_field("crop_adoption_sorghum") {
                self.crop_adoption_sorghum.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][6]
                });
            }
            if include_field("crop_adoption_yam") {
                self.crop_adoption_yam.observe_with(crop_len, |index| {
                    world.state.domesticates.crop_adoption[index][7]
                });
            }
        }

        let include_any_crop_available = include_field("crop_available_wheat")
            || include_field("crop_available_rice")
            || include_field("crop_available_maize")
            || include_field("crop_available_millet")
            || include_field("crop_available_potato")
            || include_field("crop_available_cassava")
            || include_field("crop_available_sorghum")
            || include_field("crop_available_yam");
        if include_any_crop_available {
            let crop_available_len = world.state.domesticates.crop_available.len();
            if include_field("crop_available_wheat") {
                self.crop_available_wheat
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 0)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("crop_available_rice") {
                self.crop_available_rice
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 1)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("crop_available_maize") {
                self.crop_available_maize
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 2)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("crop_available_millet") {
                self.crop_available_millet
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 3)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("crop_available_potato") {
                self.crop_available_potato
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 4)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("crop_available_cassava") {
                self.crop_available_cassava
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 5)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("crop_available_sorghum") {
                self.crop_available_sorghum
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 6)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("crop_available_yam") {
                self.crop_available_yam
                    .observe_with(crop_available_len, |index| {
                        if (world.state.domesticates.crop_available[index] & (1u8 << 7)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
        }

        let include_any_livestock_adoption = include_field("livestock_adoption_cattle")
            || include_field("livestock_adoption_horse")
            || include_field("livestock_adoption_sheep")
            || include_field("livestock_adoption_pig")
            || include_field("livestock_adoption_camel");
        if include_any_livestock_adoption {
            let livestock_len = world.state.domesticates.livestock_adoption.len();
            if include_field("livestock_adoption_cattle") {
                self.livestock_adoption_cattle
                    .observe_with(livestock_len, |index| {
                        world.state.domesticates.livestock_adoption[index][0]
                    });
            }
            if include_field("livestock_adoption_horse") {
                self.livestock_adoption_horse
                    .observe_with(livestock_len, |index| {
                        world.state.domesticates.livestock_adoption[index][1]
                    });
            }
            if include_field("livestock_adoption_sheep") {
                self.livestock_adoption_sheep
                    .observe_with(livestock_len, |index| {
                        world.state.domesticates.livestock_adoption[index][2]
                    });
            }
            if include_field("livestock_adoption_pig") {
                self.livestock_adoption_pig
                    .observe_with(livestock_len, |index| {
                        world.state.domesticates.livestock_adoption[index][3]
                    });
            }
            if include_field("livestock_adoption_camel") {
                self.livestock_adoption_camel
                    .observe_with(livestock_len, |index| {
                        world.state.domesticates.livestock_adoption[index][4]
                    });
            }
        }

        let include_any_livestock_available = include_field("livestock_available_cattle")
            || include_field("livestock_available_horse")
            || include_field("livestock_available_sheep")
            || include_field("livestock_available_pig")
            || include_field("livestock_available_camel");
        if include_any_livestock_available {
            let livestock_available_len = world.state.domesticates.livestock_available.len();
            if include_field("livestock_available_cattle") {
                self.livestock_available_cattle
                    .observe_with(livestock_available_len, |index| {
                        if (world.state.domesticates.livestock_available[index] & (1u8 << 0)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("livestock_available_horse") {
                self.livestock_available_horse
                    .observe_with(livestock_available_len, |index| {
                        if (world.state.domesticates.livestock_available[index] & (1u8 << 1)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("livestock_available_sheep") {
                self.livestock_available_sheep
                    .observe_with(livestock_available_len, |index| {
                        if (world.state.domesticates.livestock_available[index] & (1u8 << 2)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("livestock_available_pig") {
                self.livestock_available_pig
                    .observe_with(livestock_available_len, |index| {
                        if (world.state.domesticates.livestock_available[index] & (1u8 << 3)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
            if include_field("livestock_available_camel") {
                self.livestock_available_camel
                    .observe_with(livestock_available_len, |index| {
                        if (world.state.domesticates.livestock_available[index] & (1u8 << 4)) != 0 {
                            1.0
                        } else {
                            0.0
                        }
                    });
            }
        }
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
        if include_field("biome") {
            if let Some(delta) = self.biome.take_delta("biome") {
                deltas.push(delta);
            }
        } else {
            self.biome.discard_pending();
        }
        if include_field("river_transport_cost") {
            if let Some(delta) = self.river_transport_cost.take_delta("river_transport_cost") {
                deltas.push(delta);
            }
        } else {
            self.river_transport_cost.discard_pending();
        }
        if include_field("crop_adoption_wheat") {
            if let Some(delta) = self.crop_adoption_wheat.take_delta("crop_adoption_wheat") {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_wheat.discard_pending();
        }
        if include_field("crop_adoption_rice") {
            if let Some(delta) = self.crop_adoption_rice.take_delta("crop_adoption_rice") {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_rice.discard_pending();
        }
        if include_field("crop_adoption_maize") {
            if let Some(delta) = self.crop_adoption_maize.take_delta("crop_adoption_maize") {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_maize.discard_pending();
        }
        if include_field("crop_adoption_millet") {
            if let Some(delta) = self.crop_adoption_millet.take_delta("crop_adoption_millet") {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_millet.discard_pending();
        }
        if include_field("crop_adoption_potato") {
            if let Some(delta) = self.crop_adoption_potato.take_delta("crop_adoption_potato") {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_potato.discard_pending();
        }
        if include_field("crop_adoption_cassava") {
            if let Some(delta) = self
                .crop_adoption_cassava
                .take_delta("crop_adoption_cassava")
            {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_cassava.discard_pending();
        }
        if include_field("crop_adoption_sorghum") {
            if let Some(delta) = self
                .crop_adoption_sorghum
                .take_delta("crop_adoption_sorghum")
            {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_sorghum.discard_pending();
        }
        if include_field("crop_adoption_yam") {
            if let Some(delta) = self.crop_adoption_yam.take_delta("crop_adoption_yam") {
                deltas.push(delta);
            }
        } else {
            self.crop_adoption_yam.discard_pending();
        }
        if include_field("crop_available_wheat") {
            if let Some(delta) = self.crop_available_wheat.take_delta("crop_available_wheat") {
                deltas.push(delta);
            }
        } else {
            self.crop_available_wheat.discard_pending();
        }
        if include_field("crop_available_rice") {
            if let Some(delta) = self.crop_available_rice.take_delta("crop_available_rice") {
                deltas.push(delta);
            }
        } else {
            self.crop_available_rice.discard_pending();
        }
        if include_field("crop_available_maize") {
            if let Some(delta) = self.crop_available_maize.take_delta("crop_available_maize") {
                deltas.push(delta);
            }
        } else {
            self.crop_available_maize.discard_pending();
        }
        if include_field("crop_available_millet") {
            if let Some(delta) = self
                .crop_available_millet
                .take_delta("crop_available_millet")
            {
                deltas.push(delta);
            }
        } else {
            self.crop_available_millet.discard_pending();
        }
        if include_field("crop_available_potato") {
            if let Some(delta) = self
                .crop_available_potato
                .take_delta("crop_available_potato")
            {
                deltas.push(delta);
            }
        } else {
            self.crop_available_potato.discard_pending();
        }
        if include_field("crop_available_cassava") {
            if let Some(delta) = self
                .crop_available_cassava
                .take_delta("crop_available_cassava")
            {
                deltas.push(delta);
            }
        } else {
            self.crop_available_cassava.discard_pending();
        }
        if include_field("crop_available_sorghum") {
            if let Some(delta) = self
                .crop_available_sorghum
                .take_delta("crop_available_sorghum")
            {
                deltas.push(delta);
            }
        } else {
            self.crop_available_sorghum.discard_pending();
        }
        if include_field("crop_available_yam") {
            if let Some(delta) = self.crop_available_yam.take_delta("crop_available_yam") {
                deltas.push(delta);
            }
        } else {
            self.crop_available_yam.discard_pending();
        }
        if include_field("livestock_adoption_cattle") {
            if let Some(delta) = self
                .livestock_adoption_cattle
                .take_delta("livestock_adoption_cattle")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_adoption_cattle.discard_pending();
        }
        if include_field("livestock_adoption_horse") {
            if let Some(delta) = self
                .livestock_adoption_horse
                .take_delta("livestock_adoption_horse")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_adoption_horse.discard_pending();
        }
        if include_field("livestock_adoption_sheep") {
            if let Some(delta) = self
                .livestock_adoption_sheep
                .take_delta("livestock_adoption_sheep")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_adoption_sheep.discard_pending();
        }
        if include_field("livestock_adoption_pig") {
            if let Some(delta) = self
                .livestock_adoption_pig
                .take_delta("livestock_adoption_pig")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_adoption_pig.discard_pending();
        }
        if include_field("livestock_adoption_camel") {
            if let Some(delta) = self
                .livestock_adoption_camel
                .take_delta("livestock_adoption_camel")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_adoption_camel.discard_pending();
        }
        if include_field("livestock_available_cattle") {
            if let Some(delta) = self
                .livestock_available_cattle
                .take_delta("livestock_available_cattle")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_available_cattle.discard_pending();
        }
        if include_field("livestock_available_horse") {
            if let Some(delta) = self
                .livestock_available_horse
                .take_delta("livestock_available_horse")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_available_horse.discard_pending();
        }
        if include_field("livestock_available_sheep") {
            if let Some(delta) = self
                .livestock_available_sheep
                .take_delta("livestock_available_sheep")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_available_sheep.discard_pending();
        }
        if include_field("livestock_available_pig") {
            if let Some(delta) = self
                .livestock_available_pig
                .take_delta("livestock_available_pig")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_available_pig.discard_pending();
        }
        if include_field("livestock_available_camel") {
            if let Some(delta) = self
                .livestock_available_camel
                .take_delta("livestock_available_camel")
            {
                deltas.push(delta);
            }
        } else {
            self.livestock_available_camel.discard_pending();
        }
        deltas
    }
}

impl ManagedWorld {
    pub fn refresh_reduced_metrics(&mut self) {
        if self.verification_mode != VerificationMode::HeadlessMetrics {
            self.reduced_metrics = None;
            return;
        }
        let cells = self.world.cell_store();
        let metrics = reduce_metrics_for_headless(
            cells.height,
            cells.river_flow,
            self.world.sea_level_offset(),
        );
        self.reduced_metrics = Some(to_world_metrics(metrics));
    }

    pub fn current_metrics(&self) -> world::WorldMetrics {
        if let Some(mut metrics) = self.reduced_metrics {
            metrics.global_sediment_export = self.world.control.global_sediment_export.max(0.0);
            metrics.marine_sediment_mass = self.world.control.marine_sediment_mass.max(0.0);
            metrics.solid_earth_mass_proxy = self.world.control.solid_earth_mass_proxy;
            metrics.solid_earth_mass_proxy_drift = self.world.control.solid_earth_mass_proxy
                - self.world.control.solid_earth_mass_proxy_baseline;
            metrics.ocean_water_inventory = self.world.control.ocean_water_inventory.max(0.0);
            metrics.ocean_water_inventory_drift = self.world.control.ocean_water_inventory
                - self.world.control.ocean_water_inventory_baseline;
            metrics.ice_inventory = self.world.control.ice_inventory.max(0.0);
            return metrics;
        }
        self.world.metrics()
    }

    pub fn matched_geology_dynamics(&self) -> Option<&world::GeologyDynamicsState> {
        self.geology_dynamics
            .as_ref()
            .filter(|state| state.vertex_states.len() == self.world.state.geology.height.len())
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

    pub fn checkpoint_snapshot(&self) -> CheckpointSnapshot {
        CheckpointSnapshot {
            core: self.world.core_owned(),
            hydrology_dynamics: self.hydrology_dynamics.clone(),
            geology_dynamics: self.geology_dynamics.clone(),
            applied_intervention_seq: self.applied_intervention_seq,
        }
    }

    #[allow(dead_code)]
    pub fn snapshot_world(&self) -> WorldHistorySnapshot {
        self.checkpoint_snapshot()
    }

    pub fn reset_exec_state(&mut self) {
        self.exec_state = ManagedWorldExecState::default();
    }

    pub fn observe_after_world_change(&mut self) {
        // Delta observation is deferred to get_world_delta(include_fields) so that
        // we can avoid scanning untouched fields each tick.
    }

    pub fn push_scientific_benchmark_sample(&mut self) {
        let sample = ScientificBenchmarkSample {
            tick: self.world.clock.tick,
            era: self.world.clock.epoch.as_key().to_string(),
            metrics: self.current_metrics(),
        };
        self.scientific_benchmark_samples.push(sample);
        if self.scientific_benchmark_samples.len() > SCIENTIFIC_BENCHMARK_SAMPLE_LIMIT {
            let overflow = self
                .scientific_benchmark_samples
                .len()
                .saturating_sub(SCIENTIFIC_BENCHMARK_SAMPLE_LIMIT);
            self.scientific_benchmark_samples.drain(0..overflow);
        }
    }

    pub fn exec_is_busy(&self) -> bool {
        self.exec_state.remaining_steps > 0
    }
}

impl TimelineArchive {
    pub fn new() -> Self {
        Self {
            checkpoints: BTreeMap::new(),
            interventions: Vec::new(),
            next_intervention_seq: 0,
        }
    }

    pub fn insert_checkpoint(&mut self, tick: u64, snapshot: CheckpointSnapshot) {
        self.checkpoints.insert(tick, snapshot);
    }

    #[allow(dead_code)]
    pub fn insert_snapshot(&mut self, tick: u64, snapshot: WorldHistorySnapshot) {
        self.insert_checkpoint(tick, snapshot);
    }

    pub fn save_checkpoint_if_needed(
        &mut self,
        managed: &ManagedWorld,
        retention: &TimelineRetentionPolicy,
    ) {
        if !managed
            .world
            .clock
            .tick
            .is_multiple_of(retention.checkpoint_interval)
        {
            return;
        }
        self.insert_checkpoint(managed.world.clock.tick, managed.checkpoint_snapshot());
        while self.checkpoints.len() > retention.checkpoint_limit {
            if let Some(prunable) =
                prunable_checkpoint_tick_for_seek_value(&self.checkpoints, managed.world.clock.tick)
            {
                self.checkpoints.remove(&prunable);
            } else {
                break;
            }
        }
    }

    #[allow(dead_code)]
    pub fn save_snapshot_if_needed(
        &mut self,
        managed: &ManagedWorld,
        retention: &TimelineRetentionPolicy,
    ) {
        self.save_checkpoint_if_needed(managed, retention);
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
        }
        managed.applied_intervention_seq = event.sequence.saturating_add(1);
    }
}

impl TimelineRuntime {
    pub fn new(retention: TimelineRetentionPolicy) -> Self {
        Self {
            archive: TimelineArchive::new(),
            undo_logs: BTreeMap::new(),
            cursor: TimelineCursor {
                tick: 0,
                head_tick: 0,
            },
            retention,
        }
    }

    pub fn archive(&self) -> &TimelineArchive {
        &self.archive
    }

    pub fn archive_mut(&mut self) -> &mut TimelineArchive {
        &mut self.archive
    }

    #[allow(dead_code)]
    pub fn clone_archive(&self) -> TimelineArchive {
        self.archive.clone()
    }

    pub fn current_tick(&self) -> u64 {
        self.cursor.tick
    }

    pub fn head_tick(&self) -> u64 {
        self.cursor.head_tick
    }

    pub fn observe_tick(&mut self, tick: u64) {
        self.cursor.tick = tick;
        self.cursor.head_tick = self.cursor.head_tick.max(tick);
    }

    #[allow(dead_code)]
    pub fn set_cursor_tick(&mut self, tick: u64) {
        self.cursor.tick = tick;
    }

    pub fn checkpoint_estimated_bytes(&self) -> usize {
        self.archive
            .checkpoints
            .values()
            .map(CheckpointSnapshot::estimated_bytes)
            .sum()
    }

    pub fn undo_log_estimated_bytes(&self) -> usize {
        self.undo_logs
            .values()
            .map(TickUndoLog::estimated_bytes)
            .sum()
    }

    pub fn total_estimated_bytes(&self) -> usize {
        self.checkpoint_estimated_bytes() + self.undo_log_estimated_bytes()
    }

    fn prunable_checkpoint_tick(&self) -> Option<u64> {
        prunable_checkpoint_tick_for_seek_value(&self.archive.checkpoints, self.current_tick())
    }

    fn prunable_undo_tick(&self) -> Option<u64> {
        prunable_undo_tick_for_rewind_value(
            &self.undo_logs,
            self.current_tick(),
            self.retention.undo_future_prune_grace_ticks,
        )
    }

    pub fn prune_to_retention_budget(&mut self) {
        if let Some(max_estimated_bytes) = self.retention.max_estimated_bytes {
            while self.total_estimated_bytes() > max_estimated_bytes {
                let prunable_checkpoint_tick = self.prunable_checkpoint_tick();
                let prunable_undo_tick = self.prunable_undo_tick();
                match (prunable_checkpoint_tick, prunable_undo_tick) {
                    (Some(checkpoint_tick), Some(undo_tick)) => {
                        let checkpoint_bytes = self
                            .archive
                            .checkpoints
                            .get(&checkpoint_tick)
                            .map(CheckpointSnapshot::estimated_bytes)
                            .unwrap_or(0);
                        let undo_bytes = self
                            .undo_logs
                            .get(&undo_tick)
                            .map(TickUndoLog::estimated_bytes)
                            .unwrap_or(0);
                        if checkpoint_bytes > undo_bytes {
                            self.archive.checkpoints.remove(&checkpoint_tick);
                        } else {
                            self.undo_logs.remove(&undo_tick);
                        }
                    }
                    (Some(checkpoint_tick), None) => {
                        self.archive.checkpoints.remove(&checkpoint_tick);
                    }
                    (None, Some(undo_tick)) => {
                        self.undo_logs.remove(&undo_tick);
                    }
                    (None, None) => break,
                }
            }
        }
    }

    pub fn begin_tick_undo_log(&mut self, tick: u64, snapshot_before_tick: CheckpointSnapshot) {
        self.undo_logs.entry(tick).or_insert_with(|| TickUndoLog {
            tick,
            pending_snapshot_before_tick: Some(snapshot_before_tick),
            core_change_set: WorldCoreChangeSet::default(),
            hydrology_dynamics_before: None,
            geology_dynamics_before: None,
            applied_intervention_seq_before: None,
            changed_fields: Vec::new(),
        });
        while self.undo_logs.len() > self.retention.undo_log_limit {
            if let Some(prunable) = self.prunable_undo_tick() {
                self.undo_logs.remove(&prunable);
            } else {
                break;
            }
        }
    }

    pub fn finalize_tick_undo_log(&mut self, tick: u64, managed: &ManagedWorld) {
        let Some(log) = self.undo_logs.get_mut(&tick) else {
            return;
        };
        let Some(snapshot_before_tick) = log.pending_snapshot_before_tick.take() else {
            return;
        };

        let before_core = &snapshot_before_tick.core;
        let after_core = managed.world.core_owned();
        let mut changed_fields = Vec::new();
        let core_change_set =
            build_core_change_set_from_world_diff(before_core, &after_core, &mut changed_fields);
        let runtime_aux_changes =
            build_runtime_aux_changes(&snapshot_before_tick, managed, &mut changed_fields);

        log.core_change_set = core_change_set;
        log.hydrology_dynamics_before = runtime_aux_changes.hydrology_dynamics_before;
        log.geology_dynamics_before = runtime_aux_changes.geology_dynamics_before;
        log.applied_intervention_seq_before = runtime_aux_changes.applied_intervention_seq_before;
        log.changed_fields = changed_fields
            .into_iter()
            .map(|field| field.as_str().to_string())
            .collect();
        self.observe_tick(tick);
        self.prune_to_retention_budget();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        ClockUndoState, ConflictUndoState, ControlUndoState, DomesticatesUndoState,
        EntityUndoState, F32FieldTracker, I32FieldTracker, ManagedWorld, ManagedWorldExecState,
        PolityGroupsUndoState, PolityUndoState, PopulationUndoState, RelationsUndoState,
        SettlementUndoState, SubsistenceUndoState, U32FieldTracker, WorldTransportCache,
    };
    use crate::sim::geology_types::{GeologyInternal, PlateId};
    use crate::sim::polity::types::PolityGroup;
    use crate::sim::world;
    use verification_runtime::{VerificationMode, SCIENTIFIC_BENCHMARK_SAMPLE_LIMIT};

    fn test_checkpoint_snapshot() -> super::CheckpointSnapshot {
        let world = world::World::new(
            world::WorldMesh {
                positions: vec![[0.0, 0.0, 1.0]],
                nbr_offsets: vec![0, 0],
                nbrs: vec![],
            },
            world::GeologyState {
                height: vec![0.2],
                lake_depth: vec![0.0],
                plate_id: vec![PlateId(0)],
                plate_emergence_regime: Default::default(),
                plate_emergence_fallback: Default::default(),
                initial_plate_kinematics: Vec::new(),
                volcanism: vec![0.0],
                vertex_buoyancy: vec![0.0],
                geology_internal: vec![GeologyInternal::default()],
                boundary_condition: vec![0.0],
                smoothing_limited_cells_ratio: 0.0,
                mean_smoothing_factor: 1.0,
                zero_mean_adjusted_cells_ratio: 0.0,
                zero_mean_mean_abs_correction: 0.0,
                zero_mean_std_delta: 0.0,
            },
        );

        super::CheckpointSnapshot {
            core: world.core_owned(),
            hydrology_dynamics: None,
            geology_dynamics: None,
            applied_intervention_seq: 0,
        }
    }

    fn test_tick_undo_log(tick: u64) -> super::TickUndoLog {
        super::TickUndoLog {
            tick,
            pending_snapshot_before_tick: Some(test_checkpoint_snapshot()),
            core_change_set: super::WorldCoreChangeSet::default(),
            hydrology_dynamics_before: None,
            geology_dynamics_before: None,
            applied_intervention_seq_before: None,
            changed_fields: Vec::new(),
        }
    }

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
        let observed = [0, 8, 2, 3, 9, 5];
        tracker.observe_with(observed.len(), |index| observed[index]);

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
            biome: I32FieldTracker::new(&[0, 0]),
            river_transport_cost: F32FieldTracker::new(&[0.2, 0.2]),
            crop_adoption_wheat: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_rice: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_maize: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_millet: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_potato: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_cassava: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_sorghum: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_yam: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_wheat: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_rice: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_maize: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_millet: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_potato: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_cassava: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_sorghum: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_yam: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_cattle: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_horse: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_sheep: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_pig: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_camel: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_cattle: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_horse: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_sheep: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_pig: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_camel: F32FieldTracker::new(&[0.0, 0.0]),
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
            biome: I32FieldTracker::new(&[0, 0]),
            river_transport_cost: F32FieldTracker::new(&[0.2, 0.2]),
            crop_adoption_wheat: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_rice: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_maize: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_millet: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_potato: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_cassava: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_sorghum: F32FieldTracker::new(&[0.0, 0.0]),
            crop_adoption_yam: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_wheat: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_rice: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_maize: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_millet: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_potato: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_cassava: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_sorghum: F32FieldTracker::new(&[0.0, 0.0]),
            crop_available_yam: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_cattle: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_horse: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_sheep: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_pig: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_adoption_camel: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_cattle: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_horse: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_sheep: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_pig: F32FieldTracker::new(&[0.0, 0.0]),
            livestock_available_camel: F32FieldTracker::new(&[0.0, 0.0]),
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
            plate_emergence_regime: Default::default(),
            plate_emergence_fallback: Default::default(),
            initial_plate_kinematics: Vec::new(),
            volcanism: vec![0.0],
            vertex_buoyancy: vec![0.0],
            geology_internal: vec![GeologyInternal::default()],
            boundary_condition: vec![0.0],
            smoothing_limited_cells_ratio: 0.0,
            mean_smoothing_factor: 1.0,
            zero_mean_adjusted_cells_ratio: 0.0,
            zero_mean_mean_abs_correction: 0.0,
            zero_mean_std_delta: 0.0,
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
            verification_mode: VerificationMode::Interactive,
            reduced_metrics: None,
            scientific_benchmark_samples: Vec::new(),
            geology_params: crate::GeologyParams::default(),
            transport_cache: WorldTransportCache::from_world(&sim_world, None),
            exec_state: ManagedWorldExecState::default(),
            applied_intervention_seq: 0,
        };

        let snapshot = managed.snapshot_world();
        assert_eq!(snapshot.core.cells.geology.height, vec![0.2]);
        assert_eq!(snapshot.core.clock.tick, sim_world.clock.tick);
        assert_eq!(
            snapshot.core.entities.iter_polities().count(),
            sim_world.entities.iter_polities().count()
        );
    }

    #[test]
    fn scientific_benchmark_samples_keep_recent_window() {
        let geology = world::GeologyState {
            height: vec![0.2],
            lake_depth: vec![0.0],
            plate_id: vec![PlateId(0)],
            plate_emergence_regime: Default::default(),
            plate_emergence_fallback: Default::default(),
            initial_plate_kinematics: Vec::new(),
            volcanism: vec![0.0],
            vertex_buoyancy: vec![0.0],
            geology_internal: vec![GeologyInternal::default()],
            boundary_condition: vec![0.0],
            smoothing_limited_cells_ratio: 0.0,
            mean_smoothing_factor: 1.0,
            zero_mean_adjusted_cells_ratio: 0.0,
            zero_mean_mean_abs_correction: 0.0,
            zero_mean_std_delta: 0.0,
        };
        let mesh = world::WorldMesh {
            positions: vec![[0.0, 0.0, 1.0]],
            nbr_offsets: vec![0, 0],
            nbrs: vec![],
        };
        let sim_world = world::World::new(mesh, geology);
        let mut managed = ManagedWorld {
            world: sim_world,
            hydrology_dynamics: None,
            geology_dynamics: None,
            feedback: world::FeedbackQueue::new(1),
            simulation_rate: 1.0,
            verification_mode: VerificationMode::ScientificBenchmark,
            reduced_metrics: None,
            scientific_benchmark_samples: Vec::new(),
            geology_params: crate::GeologyParams::default(),
            transport_cache: WorldTransportCache::from_world(
                &world::World::new(
                    world::WorldMesh {
                        positions: vec![[0.0, 0.0, 1.0]],
                        nbr_offsets: vec![0, 0],
                        nbrs: vec![],
                    },
                    world::GeologyState {
                        height: vec![0.2],
                        lake_depth: vec![0.0],
                        plate_id: vec![PlateId(0)],
                        plate_emergence_regime: Default::default(),
                        plate_emergence_fallback: Default::default(),
                        initial_plate_kinematics: Vec::new(),
                        volcanism: vec![0.0],
                        vertex_buoyancy: vec![0.0],
                        geology_internal: vec![GeologyInternal::default()],
                        boundary_condition: vec![0.0],
                        smoothing_limited_cells_ratio: 0.0,
                        mean_smoothing_factor: 1.0,
                        zero_mean_adjusted_cells_ratio: 0.0,
                        zero_mean_mean_abs_correction: 0.0,
                        zero_mean_std_delta: 0.0,
                    },
                ),
                None,
            ),
            exec_state: ManagedWorldExecState::default(),
            applied_intervention_seq: 0,
        };

        for tick in 0..(SCIENTIFIC_BENCHMARK_SAMPLE_LIMIT + 8) {
            managed.world.clock.tick = tick as u64;
            managed.push_scientific_benchmark_sample();
        }

        assert_eq!(
            managed.scientific_benchmark_samples.len(),
            SCIENTIFIC_BENCHMARK_SAMPLE_LIMIT
        );
        assert_eq!(
            managed
                .scientific_benchmark_samples
                .first()
                .map(|sample| sample.tick),
            Some(8)
        );
    }

    #[test]
    fn domesticates_undo_uses_sparse_patches_for_public_arrays() {
        let before = world::DomesticatesState {
            crop_available: vec![0, 1],
            crop_adoption: vec![[0.0; world::N_CROPS], [0.0; world::N_CROPS]],
            livestock_available: vec![0, 0],
            livestock_adoption: vec![[0.0; world::N_LIVESTOCK], [0.0; world::N_LIVESTOCK]],
            domesticates_internal: vec![world::DomesticatesInternal::default(); 2],
        };
        let mut after = before.clone();
        after.crop_adoption[1][0] = 0.5;
        after.livestock_available[0] = 1;

        let undo = DomesticatesUndoState::from_diff(&before, &after).expect("domesticates undo");
        assert!(undo.full.is_none());
        assert!(undo.crop_adoption.is_some());
        assert!(undo.livestock_available.is_some());
    }

    #[test]
    fn domesticates_undo_uses_sparse_patch_for_internal_state_changes() {
        let before = world::DomesticatesState {
            crop_available: vec![0],
            crop_adoption: vec![[0.0; world::N_CROPS]],
            livestock_available: vec![0],
            livestock_adoption: vec![[0.0; world::N_LIVESTOCK]],
            domesticates_internal: vec![world::DomesticatesInternal::default()],
        };
        let mut after = before.clone();
        after.domesticates_internal[0].diffusion_memory = 1.0;

        let undo = DomesticatesUndoState::from_diff(&before, &after).expect("domesticates undo");
        assert!(undo.full.is_none());
        assert!(undo.domesticates_internal.is_some());
    }

    #[test]
    fn subsistence_undo_uses_sparse_patches_for_mix_and_scalar_fields() {
        let before = world::SubsistenceState {
            subsistence_mix: vec![world::SubsistenceMix::default(); 2],
            food_energy_mean: vec![0.0, 0.0],
            food_energy_variance: vec![1.0, 1.0],
            buffer_capacity: vec![0.0, 0.0],
            mobility_capacity: vec![0.0, 0.0],
            land_use_intensity: vec![0.0, 0.0],
        };
        let mut after = before.clone();
        after.subsistence_mix[0].cultivation = 0.25;
        after.food_energy_mean[1] = 0.7;

        let undo = SubsistenceUndoState::from_diff(&before, &after).expect("subsistence undo");
        assert!(undo.full.is_none());
        assert!(undo.subsistence_mix.is_some());
        assert!(undo.food_energy_mean.is_some());
    }

    #[test]
    fn population_undo_uses_sparse_patches_for_all_fields() {
        let before = world::PopulationState {
            population: vec![0.0, 0.0],
            birth_rate: vec![0.0, 0.0],
            death_rate: vec![0.0, 0.0],
        };
        let mut after = before.clone();
        after.population[0] = 10.0;
        after.birth_rate[1] = 0.2;

        let undo = PopulationUndoState::from_diff(&before, &after).expect("population undo");
        assert!(undo.population.is_some());
        assert!(undo.birth_rate.is_some());
        assert!(undo.full.is_none());
    }

    #[test]
    fn settlement_undo_uses_sparse_patch() {
        let before = world::SettlementState {
            urbanization: vec![0.0, 0.0],
        };
        let mut after = before.clone();
        after.urbanization[1] = 0.5;

        let undo = SettlementUndoState::from_diff(&before, &after).expect("settlement undo");
        assert!(undo.urbanization.is_some());
        assert!(undo.full.is_none());
    }

    #[test]
    fn polity_undo_uses_sparse_patch_for_optional_ids() {
        let before = world::PolityState {
            polity_id: vec![None, None],
        };
        let mut after = before.clone();
        after.polity_id[1] = Some(world::PolityId(3));

        let undo = PolityUndoState::from_diff(&before, &after).expect("polity undo");
        assert!(undo.polity_id.is_some());
        assert!(undo.full.is_none());
    }

    #[test]
    fn conflict_undo_uses_sparse_patches_for_intensity_and_occupier() {
        let before = world::ConflictState {
            conflict_intensity: vec![0.0, 0.0],
            occupier_id: vec![None, None],
        };
        let mut after = before.clone();
        after.conflict_intensity[0] = 0.8;
        after.occupier_id[1] = Some(world::PolityId(2));

        let undo = ConflictUndoState::from_diff(&before, &after).expect("conflict undo");
        assert!(undo.conflict_intensity.is_some());
        assert!(undo.occupier_id.is_some());
        assert!(undo.full.is_none());
    }

    #[test]
    fn clock_undo_tracks_changed_fields_without_full_clone() {
        let before = world::ClockState {
            tick: 1,
            epoch: world::EraKind::Crust,
            real_years_per_tick: 10.0,
            runtime_tick_ms: 20,
            budgets: world::SubsystemBudgets::default(),
            transition: world::TransitionState {
                era_enter_tick: 0,
                stable_ticks_in_era: 0,
                last_land_ratio: 0.0,
                ema_geology_activity: 0.0,
                ema_climate_activity: 0.0,
                ema_ecology_activity: 0.0,
                ema_civilization_activity: 0.0,
            },
        };
        let mut after = before.clone();
        after.tick = 2;
        after.runtime_tick_ms = 25;

        let undo = ClockUndoState::from_diff(&before, &after).expect("clock undo");
        assert_eq!(undo.tick, Some(1));
        assert_eq!(undo.runtime_tick_ms, Some(20));
    }

    #[test]
    fn control_undo_uses_scalar_fields_when_params_are_unchanged() {
        let before = world::WorldControlState {
            geology_params: crate::sim::geology_types::GeologyParams::default(),
            sea_level_offset: 0.0,
            erosion_thickness_coupling: 1.0,
            deposition_thickness_coupling: 1.0,
            ocean_water_inventory: 0.0,
            ocean_water_inventory_baseline: 0.0,
            ice_inventory: 0.0,
            marine_sediment_mass: 0.0,
            global_sediment_export: 0.0,
            solid_earth_mass_proxy: 0.0,
            solid_earth_mass_proxy_baseline: 0.0,
        };
        let mut after = before.clone();
        after.sea_level_offset = 0.5;

        let undo = ControlUndoState::from_diff(&before, &after).expect("control undo");
        assert!(undo.full.is_none());
        assert_eq!(undo.sea_level_offset, Some(0.0));
    }

    #[test]
    fn entity_undo_tracks_record_upserts_and_removals() {
        let mut before = world::EntityState::default();
        let _ = before.create_polity(world::PolityRecord {
            id: world::PolityId(1),
            capital_cell: world::CellId(1),
            legitimacy: 0.4,
            centralization: 0.3,
            military_tech: 0.2,
            cells_cache: vec![world::CellId(1)],
        });
        let mut after = before.clone();
        let _ = after.remove_polity(world::PolityId(1));
        let _ = after.create_polity(world::PolityRecord {
            id: world::PolityId(2),
            capital_cell: world::CellId(2),
            legitimacy: 0.8,
            centralization: 0.7,
            military_tech: 0.6,
            cells_cache: vec![world::CellId(2)],
        });

        let undo = EntityUndoState::from_diff(&before, &after).expect("entity undo");
        assert_eq!(undo.polity_upserts.len(), 1);
        assert_eq!(undo.polity_removals, vec![world::PolityId(2)]);
    }

    #[test]
    fn relations_undo_tracks_map_and_group_changes() {
        let mut before = world::WorldRelations::default();
        before.polity_groups.push(PolityGroup {
            id: world::PolityGroupId(1),
            kind: crate::sim::polity::types::GroupKind::CulturalSphere,
            members: vec![world::PolityId(1)],
            leader: Some(world::PolityId(1)),
        });
        let mut after = before.clone();
        after.polity_groups.clear();

        let undo = RelationsUndoState::from_diff(&before, &after).expect("relations undo");
        assert!(undo.polity_groups.is_some());
        undo.apply_to(&mut after);
        assert_eq!(after, before);
    }

    #[test]
    fn polity_groups_undo_restores_group_order_and_payload() {
        let before = vec![
            PolityGroup {
                id: world::PolityGroupId(1),
                kind: crate::sim::polity::types::GroupKind::CulturalSphere,
                members: vec![world::PolityId(1)],
                leader: Some(world::PolityId(1)),
            },
            PolityGroup {
                id: world::PolityGroupId(2),
                kind: crate::sim::polity::types::GroupKind::MilitaryAlliance,
                members: vec![world::PolityId(2), world::PolityId(3)],
                leader: Some(world::PolityId(2)),
            },
        ];
        let after = vec![
            PolityGroup {
                id: world::PolityGroupId(2),
                kind: crate::sim::polity::types::GroupKind::EconomicZone,
                members: vec![world::PolityId(3)],
                leader: Some(world::PolityId(3)),
            },
            PolityGroup {
                id: world::PolityGroupId(3),
                kind: crate::sim::polity::types::GroupKind::CulturalSphere,
                members: vec![world::PolityId(4)],
                leader: None,
            },
        ];

        let undo = PolityGroupsUndoState::from_diff(&before, &after).expect("groups undo");
        let mut restored = after.clone();
        undo.apply_to(&mut restored);

        assert_eq!(restored, before);
    }

    #[test]
    fn entity_undo_estimated_bytes_grow_with_variable_payload() {
        let small = EntityUndoState {
            polity_upserts: vec![world::PolityRecord {
                id: world::PolityId(1),
                capital_cell: world::CellId(1),
                legitimacy: 0.5,
                centralization: 0.5,
                military_tech: 0.5,
                cells_cache: vec![world::CellId(1)],
            }],
            ..EntityUndoState::default()
        };
        let large = EntityUndoState {
            polity_upserts: vec![world::PolityRecord {
                id: world::PolityId(1),
                capital_cell: world::CellId(1),
                legitimacy: 0.5,
                centralization: 0.5,
                military_tech: 0.5,
                cells_cache: vec![
                    world::CellId(1),
                    world::CellId(2),
                    world::CellId(3),
                    world::CellId(4),
                ],
            }],
            ..EntityUndoState::default()
        };

        assert!(large.estimated_bytes() > small.estimated_bytes());
    }

    #[test]
    fn relations_undo_estimated_bytes_grow_with_group_members() {
        let small = RelationsUndoState {
            polity_groups: Some(PolityGroupsUndoState {
                upserts: vec![PolityGroup {
                    id: world::PolityGroupId(1),
                    kind: crate::sim::polity::types::GroupKind::CulturalSphere,
                    members: vec![world::PolityId(1)],
                    leader: Some(world::PolityId(1)),
                }],
                order_before: vec![world::PolityGroupId(1)],
                ..PolityGroupsUndoState::default()
            }),
            ..RelationsUndoState::default()
        };
        let large = RelationsUndoState {
            polity_groups: Some(PolityGroupsUndoState {
                upserts: vec![PolityGroup {
                    id: world::PolityGroupId(1),
                    kind: crate::sim::polity::types::GroupKind::CulturalSphere,
                    members: vec![
                        world::PolityId(1),
                        world::PolityId(2),
                        world::PolityId(3),
                        world::PolityId(4),
                    ],
                    leader: Some(world::PolityId(1)),
                }],
                order_before: vec![world::PolityGroupId(1)],
                ..PolityGroupsUndoState::default()
            }),
            ..RelationsUndoState::default()
        };

        assert!(large.estimated_bytes() > small.estimated_bytes());
    }

    #[test]
    fn prunable_checkpoint_prefers_redundant_middle_checkpoint() {
        let snapshot = test_checkpoint_snapshot();
        let mut checkpoints = BTreeMap::new();
        checkpoints.insert(0, snapshot.clone());
        checkpoints.insert(10, snapshot.clone());
        checkpoints.insert(11, snapshot.clone());
        checkpoints.insert(20, snapshot);

        let prunable = super::prunable_checkpoint_tick_for_seek_value(&checkpoints, 20);
        assert_eq!(prunable, Some(11));
    }

    #[test]
    fn prunable_checkpoint_protects_current_nearest_checkpoint() {
        let snapshot = test_checkpoint_snapshot();
        let mut checkpoints = BTreeMap::new();
        checkpoints.insert(0, snapshot.clone());
        checkpoints.insert(50, snapshot.clone());
        checkpoints.insert(60, snapshot.clone());
        checkpoints.insert(100, snapshot);

        let prunable = super::prunable_checkpoint_tick_for_seek_value(&checkpoints, 52);
        assert_eq!(prunable, Some(60));
    }

    #[test]
    fn prunable_undo_prefers_future_ticks_over_rewind_window() {
        let mut undo_logs = BTreeMap::new();
        undo_logs.insert(40, test_tick_undo_log(40));
        undo_logs.insert(49, test_tick_undo_log(49));
        undo_logs.insert(50, test_tick_undo_log(50));
        undo_logs.insert(51, test_tick_undo_log(51));

        let prunable = super::prunable_undo_tick_for_rewind_value(&undo_logs, 50, 0);
        assert_eq!(prunable, Some(51));
    }

    #[test]
    fn prunable_undo_protects_current_tick_log_when_alternatives_exist() {
        let mut undo_logs = BTreeMap::new();
        undo_logs.insert(20, test_tick_undo_log(20));
        undo_logs.insert(21, test_tick_undo_log(21));
        undo_logs.insert(22, test_tick_undo_log(22));
        undo_logs.insert(23, test_tick_undo_log(23));

        let prunable = super::prunable_undo_tick_for_rewind_value(&undo_logs, 22, 0);
        assert_eq!(prunable, Some(23));
    }

    #[test]
    fn prunable_undo_respects_future_prune_grace_ticks() {
        let mut undo_logs = BTreeMap::new();
        undo_logs.insert(20, test_tick_undo_log(20));
        undo_logs.insert(50, test_tick_undo_log(50));
        undo_logs.insert(51, test_tick_undo_log(51));
        undo_logs.insert(52, test_tick_undo_log(52));

        let without_grace = super::prunable_undo_tick_for_rewind_value(&undo_logs, 50, 0);
        assert_eq!(without_grace, Some(52));

        let with_grace = super::prunable_undo_tick_for_rewind_value(&undo_logs, 50, 4);
        assert_eq!(with_grace, Some(20));
    }
}
