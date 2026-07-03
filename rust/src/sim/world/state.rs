use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::entity_state::{EntityState, EntityStateError};
use super::exec::{
    ClockState, ComponentPatch, EntityBundle, EntityRef, ExecScratchState, TargetRef,
};
use crate::sim::geology_types::{
    CrustType, GeologyInternal, InitialPlateKinematics, PlateEmergenceFallbackKind, PlateId,
    PlateRelation, StressTensor, TectonicRegime,
};
use crate::sim::polity::types::{PolityGroup, PolityRelation};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    #[serde(flatten)]
    pub metadata: WorldMetadata,
    pub state: WorldState,
    #[serde(default, skip_serializing_if = "WorldProjectionState::is_empty")]
    pub projections: WorldProjectionState,
    pub entities: EntityStore,
    pub clock: ClockState,
    pub control: WorldControlState,
    #[serde(default, skip_serializing_if = "ExecScratchState::is_empty")]
    pub exec_scratch: ExecScratchState,
    #[serde(default, flatten)]
    pub relations: WorldRelations,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldMetadata {
    pub mesh: WorldMesh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldRelations {
    #[serde(default)]
    pub polity_relations: HashMap<(PolityId, PolityId), PolityRelation>,
    #[serde(default)]
    pub polity_groups: Vec<PolityGroup>,
    #[serde(default)]
    pub plate_relations: HashMap<(PlateId, PlateId), PlateRelation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldControlState {
    pub geology_params: crate::sim::geology_types::GeologyParams,
    pub sea_level_offset: f32,
    pub erosion_thickness_coupling: f32,
    pub deposition_thickness_coupling: f32,
    #[serde(default)]
    pub ocean_water_inventory: f32,
    #[serde(default)]
    pub ocean_water_inventory_baseline: f32,
    #[serde(default)]
    pub ice_inventory: f32,
    #[serde(default)]
    pub marine_sediment_mass: f32,
    #[serde(default)]
    pub global_sediment_export: f32,
    #[serde(default)]
    pub solid_earth_mass_proxy: f32,
    #[serde(default)]
    pub solid_earth_mass_proxy_baseline: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ArchiveState {
    #[serde(default)]
    pub history_ticks: BTreeMap<u64, String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Ord, PartialOrd,
)]
#[serde(transparent)]
pub struct CellId(pub u32);

impl CellId {
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Ord, PartialOrd,
)]
#[serde(transparent)]
pub struct PolityId(pub u32);

impl PolityId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Ord, PartialOrd,
)]
#[serde(transparent)]
pub struct SettlementId(pub u32);

impl SettlementId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Ord, PartialOrd,
)]
#[serde(transparent)]
pub struct RegionId(pub u32);

impl RegionId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, Ord, PartialOrd,
)]
#[serde(transparent)]
pub struct PolityGroupId(pub u32);

impl PolityGroupId {
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolityComponent {
    pub polity_id: PolityId,
    pub capital_cell: CellId,
    pub legitimacy: f32,
    pub centralization: f32,
    pub military_tech: f32,
    pub cells_cache: Vec<CellId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementComponent {
    pub settlement_id: SettlementId,
    pub cell: CellId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionComponent {
    pub region_id: RegionId,
    pub cells: Vec<CellId>,
}

impl EntityState {
    pub fn from_components(
        polity_components: Vec<PolityComponent>,
        settlement_components: Vec<SettlementComponent>,
        region_components: Vec<RegionComponent>,
    ) -> Self {
        let mut store = EntityState::default();
        for component in polity_components {
            let _ = store.create_polity(component.into());
        }
        for component in settlement_components {
            let _ = store.create_settlement(component.into());
        }
        for component in region_components {
            let _ = store.create_region(component.into());
        }
        store
    }

    pub fn polity_components(&self) -> Vec<PolityComponent> {
        let mut components = self
            .iter_polities()
            .cloned()
            .map(PolityComponent::from)
            .collect::<Vec<_>>();
        components.sort_by_key(|component| component.polity_id);
        components
    }

    pub fn settlement_components(&self) -> Vec<SettlementComponent> {
        let mut components = self
            .iter_settlements()
            .cloned()
            .map(SettlementComponent::from)
            .collect::<Vec<_>>();
        components.sort_by_key(|component| component.settlement_id);
        components
    }

    pub fn region_components(&self) -> Vec<RegionComponent> {
        let mut components = self
            .iter_regions()
            .cloned()
            .map(RegionComponent::from)
            .collect::<Vec<_>>();
        components.sort_by_key(|component| component.region_id);
        components
    }

    pub fn replace_polities(&mut self, components: Vec<PolityComponent>) {
        let settlements = self.settlement_components();
        let regions = self.region_components();
        *self = EntityState::from_components(components, settlements, regions);
    }

    pub fn replace_settlements(&mut self, components: Vec<SettlementComponent>) {
        let polities = self.polity_components();
        let regions = self.region_components();
        *self = EntityState::from_components(polities, components, regions);
    }

    pub fn replace_regions(&mut self, components: Vec<RegionComponent>) {
        let polities = self.polity_components();
        let settlements = self.settlement_components();
        *self = EntityState::from_components(polities, settlements, components);
    }

    pub fn apply_entity_bundle(&mut self, bundle: EntityBundle) -> Result<(), EntityStateError> {
        match bundle {
            EntityBundle::Polity(component) => {
                self.create_polity(component.into())?;
            }
            EntityBundle::Settlement(component) => {
                self.create_settlement(component.into())?;
            }
            EntityBundle::Region(component) => {
                self.create_region(component.into())?;
            }
        }
        Ok(())
    }

    pub fn destroy_entity(&mut self, entity: &EntityRef) {
        match entity {
            EntityRef::Polity(id) => {
                self.remove_polity(*id);
            }
            EntityRef::Settlement(id) => {
                self.remove_settlement(*id);
            }
            EntityRef::Region(id) => {
                self.remove_region(*id);
            }
        }
    }

    pub fn mutate_entity(
        &mut self,
        target_ref: &TargetRef,
        entity: &EntityRef,
        patch: ComponentPatch,
    ) {
        match (target_ref, entity, patch) {
            (
                TargetRef::Polity(_),
                EntityRef::Polity(id),
                ComponentPatch::Polity {
                    capital_cell,
                    legitimacy,
                    centralization,
                    military_tech,
                    cells_cache,
                },
            ) => {
                if let Some(record) = self.get_polity_mut(*id) {
                    if let Some(value) = capital_cell {
                        record.capital_cell = value;
                    }
                    if let Some(value) = legitimacy {
                        record.legitimacy = value;
                    }
                    if let Some(value) = centralization {
                        record.centralization = value;
                    }
                    if let Some(value) = military_tech {
                        record.military_tech = value;
                    }
                    if let Some(value) = cells_cache {
                        record.cells_cache = value;
                    }
                }
            }
            (
                TargetRef::Settlement(_),
                EntityRef::Settlement(id),
                ComponentPatch::Settlement { cell },
            ) => {
                if let Some(record) = self.get_settlement_mut(*id) {
                    if let Some(value) = cell {
                        record.cell = value;
                    }
                }
            }
            (TargetRef::Region(_), EntityRef::Region(id), ComponentPatch::Region { cells }) => {
                if let Some(record) = self.get_region_mut(*id) {
                    if let Some(value) = cells {
                        record.cells = value;
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldMesh {
    pub positions: Vec<[f32; 3]>,
    pub nbr_offsets: Vec<u32>,
    pub nbrs: Vec<u32>,
}

impl WorldMesh {
    pub fn position(&self, index: usize) -> Option<[f32; 3]> {
        self.positions.get(index).copied()
    }

    pub fn cell_neighbors(&self, index: usize) -> &[u32] {
        let start = self.nbr_offsets.get(index).copied().unwrap_or(0) as usize;
        let end = self
            .nbr_offsets
            .get(index + 1)
            .copied()
            .unwrap_or(start as u32) as usize;
        self.nbrs.get(start..end).unwrap_or(&[])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldState {
    pub geology: GeologyState,
    pub climate: ClimateState,
    #[serde(default)]
    pub glaciology: GlaciologyState,
    pub hydrology: HydrologyState,
    pub ecology: EcologyState,
    pub domesticates: DomesticatesState,
    pub subsistence: SubsistenceState,
    pub population: PopulationState,
    pub settlement: SettlementState,
    pub polity: PolityState,
    pub conflict: ConflictState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldProjectionState {
    pub terrain: TerrainState,
}

impl WorldProjectionState {
    pub fn is_empty(&self) -> bool {
        self.terrain.latitude.is_empty()
            && self.terrain.distance_from_ocean.is_empty()
            && self.terrain.coast_side.is_empty()
            && self.terrain.is_coastal.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TerrainState {
    pub latitude: Vec<f32>,
    pub distance_from_ocean: Vec<f32>,
    pub coast_side: Vec<CoastSide>,
    pub is_coastal: Vec<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CoastSide {
    East,
    West,
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeologyState {
    pub height: Vec<f32>,
    #[serde(default)]
    pub lake_depth: Vec<f32>,
    pub plate_id: Vec<PlateId>,
    #[serde(default)]
    pub plate_emergence_regime: TectonicRegime,
    #[serde(default)]
    pub plate_emergence_fallback: PlateEmergenceFallbackKind,
    #[serde(default)]
    pub initial_plate_kinematics: Vec<InitialPlateKinematics>,
    #[serde(default)]
    pub volcanism: Vec<f32>,
    #[serde(default)]
    pub vertex_buoyancy: Vec<f32>,
    #[serde(default)]
    pub geology_internal: Vec<GeologyInternal>,
    pub boundary_condition: Vec<f32>,
    #[serde(default)]
    pub smoothing_limited_cells_ratio: f32,
    #[serde(default)]
    pub mean_smoothing_factor: f32,
    #[serde(default)]
    pub zero_mean_adjusted_cells_ratio: f32,
    #[serde(default)]
    pub zero_mean_mean_abs_correction: f32,
    #[serde(default)]
    pub zero_mean_std_delta: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClimateState {
    pub temperature: Vec<f32>,
    pub precipitation: Vec<f32>,
    pub evapotranspiration: Vec<f32>,
    pub runoff: Vec<f32>,
    pub aridity: Vec<f32>,
    pub ocean_temperature: Vec<f32>,
    #[serde(default)]
    pub precipitable_water: Vec<f32>,
    #[serde(default)]
    pub cloud_water: Vec<f32>,
    #[serde(default)]
    pub wind_u: Vec<f32>,
    #[serde(default)]
    pub wind_v: Vec<f32>,
    #[serde(default)]
    pub moisture_flux_u: Vec<f32>,
    #[serde(default)]
    pub moisture_flux_v: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GlaciologyState {
    #[serde(default)]
    pub ice_thickness: Vec<f32>,
    #[serde(default)]
    pub ice_load: Vec<f32>,
    #[serde(default)]
    pub accumulation: Vec<f32>,
    #[serde(default)]
    pub ablation: Vec<f32>,
    #[serde(default)]
    pub isostatic_adjustment: Vec<f32>,
    #[serde(default)]
    pub applied_isostatic_adjustment: Vec<f32>,
    #[serde(default)]
    pub glacial_erosion_rate: Vec<f32>,
    #[serde(default)]
    pub glacial_melt_runoff: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydrologyState {
    pub river_downstream: Vec<SmallVec<[(u32, f32); 4]>>,
    #[serde(default)]
    pub river_next: Vec<i32>,
    pub river_flow: Vec<f32>,
    #[serde(default)]
    pub erosion_rate: Vec<f32>,
    #[serde(default)]
    pub deposition_rate: Vec<f32>,
    pub river_transport_cost: Vec<f32>,
    #[serde(default)]
    pub surface_water_access: Vec<f32>,
    #[serde(default)]
    pub is_lake: Vec<bool>,
    #[serde(default)]
    pub sink_id: Vec<i32>,
    #[serde(default)]
    pub sink_route_next: Vec<i32>,
    #[serde(default)]
    pub sink_member_offsets: Vec<u32>,
    #[serde(default)]
    pub sink_member_cells: Vec<u32>,
    #[serde(default)]
    pub sink_spill_cell: Vec<i32>,
    #[serde(default)]
    pub sink_spill_to: Vec<i32>,
    #[serde(default)]
    pub sink_spill_level: Vec<f32>,
    #[serde(default)]
    pub sink_capacity_total: Vec<f32>,
    #[serde(default)]
    pub sink_capacity_remaining: Vec<f32>,
    #[serde(default)]
    pub sink_storage_water: Vec<f32>,
    #[serde(default)]
    pub sink_storage_sediment: Vec<f32>,
    #[serde(default)]
    pub sink_overflow_active: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcologyState {
    pub biome: Vec<Biome>,
    pub tree_cover: Vec<f32>,
    pub ground_cover: Vec<f32>,
    pub disturbance: Vec<f32>,
    pub soil_fertility: Vec<f32>,
    #[serde(default)]
    pub ecology_internal: Vec<EcologyInternal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EcologyInternal {
    pub recovery_memory: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Biome {
    TropicalForest,
    Savanna,
    Desert,
    Grassland,
    #[default]
    TemperateForest,
    BorealForest,
    Tundra,
    Wetland,
    Alpine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomesticatesState {
    pub crop_available: Vec<CropBitmap>,
    pub crop_adoption: Vec<[f32; N_CROPS]>,
    pub livestock_available: Vec<LivestockBitmap>,
    pub livestock_adoption: Vec<[f32; N_LIVESTOCK]>,
    #[serde(default)]
    pub domesticates_internal: Vec<DomesticatesInternal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct DomesticatesInternal {
    #[serde(default)]
    pub diffusion_memory: f32,
    #[serde(default)]
    pub origin_initialized: bool,
    #[serde(default)]
    pub origin_seed_crop: [f32; N_CROPS],
    #[serde(default)]
    pub origin_seed_livestock: [f32; N_LIVESTOCK],
    #[serde(default)]
    pub spread_pressure_crop: [f32; N_CROPS],
    #[serde(default)]
    pub spread_pressure_livestock: [f32; N_LIVESTOCK],
    #[serde(default)]
    pub routed_feedback_crop: [f32; N_CROPS],
    #[serde(default)]
    pub routed_feedback_livestock: [f32; N_LIVESTOCK],
    #[serde(default)]
    pub population_pressure_bonus: f32,
}

pub type CropBitmap = u8;
pub type LivestockBitmap = u8;
pub const N_CROPS: usize = 8;
pub const N_LIVESTOCK: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SubsistenceMix {
    pub gathering: f32,
    pub hunting: f32,
    pub fishing: f32,
    pub cultivation: f32,
    pub herding: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubsistenceState {
    pub subsistence_mix: Vec<SubsistenceMix>,
    #[serde(default)]
    pub food_energy_mean: Vec<f32>,
    #[serde(default)]
    pub food_energy_variance: Vec<f32>,
    #[serde(default)]
    pub buffer_capacity: Vec<f32>,
    #[serde(default)]
    pub mobility_capacity: Vec<f32>,
    #[serde(default)]
    pub land_use_intensity: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopulationState {
    pub population: Vec<f32>,
    pub birth_rate: Vec<f32>,
    pub death_rate: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementState {
    pub urbanization: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolityState {
    pub polity_id: Vec<Option<PolityId>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictState {
    pub conflict_intensity: Vec<f32>,
    pub occupier_id: Vec<Option<PolityId>>,
}

pub struct CellStore<'a> {
    pub neighbors_offsets: &'a [u32],
    pub neighbors: &'a [u32],
    pub height: &'a [f32],
    pub lake_depth: &'a [f32],
    pub plate_id: &'a [PlateId],
    pub erosion_rate: &'a [f32],
    pub deposition_rate: &'a [f32],
    pub volcanism: &'a [f32],
    pub vertex_buoyancy: &'a [f32],
    pub geology_internal: &'a [GeologyInternal],
    pub temperature: &'a [f32],
    pub precipitation: &'a [f32],
    pub evapotranspiration: &'a [f32],
    pub runoff: &'a [f32],
    pub aridity: &'a [f32],
    pub ocean_temperature: &'a [f32],
    pub precipitable_water: &'a [f32],
    pub cloud_water: &'a [f32],
    pub wind_u: &'a [f32],
    pub wind_v: &'a [f32],
    pub moisture_flux_u: &'a [f32],
    pub moisture_flux_v: &'a [f32],
    pub river_downstream: &'a [SmallVec<[(u32, f32); 4]>],
    pub river_next: &'a [i32],
    pub river_flow: &'a [f32],
    pub river_transport_cost: &'a [f32],
    pub is_lake: &'a [bool],
}

pub struct CellStoreMut<'a> {
    pub height: &'a mut Vec<f32>,
    pub lake_depth: &'a mut Vec<f32>,
    pub plate_id: &'a mut Vec<PlateId>,
    pub erosion_rate: &'a mut Vec<f32>,
    pub deposition_rate: &'a mut Vec<f32>,
    pub volcanism: &'a mut Vec<f32>,
    pub vertex_buoyancy: &'a mut Vec<f32>,
    pub geology_internal: &'a mut Vec<GeologyInternal>,
    pub temperature: &'a mut Vec<f32>,
    pub precipitation: &'a mut Vec<f32>,
    pub evapotranspiration: &'a mut Vec<f32>,
    pub runoff: &'a mut Vec<f32>,
    pub aridity: &'a mut Vec<f32>,
    pub ocean_temperature: &'a mut Vec<f32>,
    pub precipitable_water: &'a mut Vec<f32>,
    pub cloud_water: &'a mut Vec<f32>,
    pub wind_u: &'a mut Vec<f32>,
    pub wind_v: &'a mut Vec<f32>,
    pub moisture_flux_u: &'a mut Vec<f32>,
    pub moisture_flux_v: &'a mut Vec<f32>,
    pub river_downstream: &'a mut Vec<SmallVec<[(u32, f32); 4]>>,
    pub river_next: &'a mut Vec<i32>,
    pub river_flow: &'a mut Vec<f32>,
    pub river_transport_cost: &'a mut Vec<f32>,
    pub is_lake: &'a mut Vec<bool>,
}

pub struct CivilizationState<'a> {
    pub population: &'a PopulationState,
    pub settlement: &'a SettlementState,
    pub polity: &'a PolityState,
    pub conflict: &'a ConflictState,
}

pub struct CivilizationStateMut<'a> {
    pub population: &'a mut PopulationState,
    pub settlement: &'a mut SettlementState,
    pub polity: &'a mut PolityState,
    pub conflict: &'a mut ConflictState,
}

pub type EntityStore = EntityState;
pub type RelationsStore = WorldRelations;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldCore {
    pub cells: WorldState,
    pub entities: EntityStore,
    pub relations: RelationsStore,
    pub clock: ClockState,
    pub control: WorldControlState,
}

pub struct WorldCoreView<'a> {
    pub cells: CellStore<'a>,
    pub entities: &'a EntityStore,
    pub relations: &'a RelationsStore,
    pub clock: &'a ClockState,
    pub control: &'a WorldControlState,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CivilizationIndicators {
    pub settled_cells: usize,
    pub total_population: f32,
    pub state_cells: usize,
}

impl World {
    pub fn mesh(&self) -> &WorldMesh {
        &self.metadata.mesh
    }

    pub fn mesh_mut(&mut self) -> &mut WorldMesh {
        &mut self.metadata.mesh
    }

    pub fn clear_projections(&mut self) {
        self.projections = WorldProjectionState::default();
    }

    pub fn clear_runtime_scratch(&mut self) {
        self.exec_scratch = ExecScratchState::default();
    }

    pub fn core_view(&self) -> WorldCoreView<'_> {
        WorldCoreView {
            cells: self.cell_store(),
            entities: &self.entities,
            relations: &self.relations,
            clock: &self.clock,
            control: &self.control,
        }
    }

    pub fn core_owned(&self) -> WorldCore {
        WorldCore {
            cells: self.state.clone(),
            entities: self.entities.clone(),
            relations: self.relations.clone(),
            clock: self.clock.clone(),
            control: self.control.clone(),
        }
    }

    pub fn apply_core(&mut self, core: WorldCore) {
        self.state = core.cells;
        self.entities = core.entities;
        self.relations = core.relations;
        self.clock = core.clock;
        self.control = core.control;
        self.refresh_terrain_state();
    }

    pub fn entity_store(&self) -> &EntityStore {
        &self.entities
    }

    pub fn entity_store_mut(&mut self) -> &mut EntityStore {
        &mut self.entities
    }

    pub fn relations_store(&self) -> &RelationsStore {
        &self.relations
    }

    pub fn relations_store_mut(&mut self) -> &mut RelationsStore {
        &mut self.relations
    }

    pub fn cell_store(&self) -> CellStore<'_> {
        CellStore {
            neighbors_offsets: &self.mesh().nbr_offsets,
            neighbors: &self.mesh().nbrs,
            height: &self.state.geology.height,
            lake_depth: &self.state.geology.lake_depth,
            plate_id: &self.state.geology.plate_id,
            erosion_rate: &self.state.hydrology.erosion_rate,
            deposition_rate: &self.state.hydrology.deposition_rate,
            volcanism: &self.state.geology.volcanism,
            vertex_buoyancy: &self.state.geology.vertex_buoyancy,
            geology_internal: &self.state.geology.geology_internal,
            temperature: &self.state.climate.temperature,
            precipitation: &self.state.climate.precipitation,
            evapotranspiration: &self.state.climate.evapotranspiration,
            runoff: &self.state.climate.runoff,
            aridity: &self.state.climate.aridity,
            ocean_temperature: &self.state.climate.ocean_temperature,
            precipitable_water: &self.state.climate.precipitable_water,
            cloud_water: &self.state.climate.cloud_water,
            wind_u: &self.state.climate.wind_u,
            wind_v: &self.state.climate.wind_v,
            moisture_flux_u: &self.state.climate.moisture_flux_u,
            moisture_flux_v: &self.state.climate.moisture_flux_v,
            river_downstream: &self.state.hydrology.river_downstream,
            river_next: &self.state.hydrology.river_next,
            river_flow: &self.state.hydrology.river_flow,
            river_transport_cost: &self.state.hydrology.river_transport_cost,
            is_lake: &self.state.hydrology.is_lake,
        }
    }

    pub fn cell_store_mut(&mut self) -> CellStoreMut<'_> {
        CellStoreMut {
            height: &mut self.state.geology.height,
            lake_depth: &mut self.state.geology.lake_depth,
            plate_id: &mut self.state.geology.plate_id,
            erosion_rate: &mut self.state.hydrology.erosion_rate,
            deposition_rate: &mut self.state.hydrology.deposition_rate,
            volcanism: &mut self.state.geology.volcanism,
            vertex_buoyancy: &mut self.state.geology.vertex_buoyancy,
            geology_internal: &mut self.state.geology.geology_internal,
            temperature: &mut self.state.climate.temperature,
            precipitation: &mut self.state.climate.precipitation,
            evapotranspiration: &mut self.state.climate.evapotranspiration,
            runoff: &mut self.state.climate.runoff,
            aridity: &mut self.state.climate.aridity,
            ocean_temperature: &mut self.state.climate.ocean_temperature,
            precipitable_water: &mut self.state.climate.precipitable_water,
            cloud_water: &mut self.state.climate.cloud_water,
            wind_u: &mut self.state.climate.wind_u,
            wind_v: &mut self.state.climate.wind_v,
            moisture_flux_u: &mut self.state.climate.moisture_flux_u,
            moisture_flux_v: &mut self.state.climate.moisture_flux_v,
            river_downstream: &mut self.state.hydrology.river_downstream,
            river_next: &mut self.state.hydrology.river_next,
            river_flow: &mut self.state.hydrology.river_flow,
            river_transport_cost: &mut self.state.hydrology.river_transport_cost,
            is_lake: &mut self.state.hydrology.is_lake,
        }
    }

    pub fn with_geology_exec_state<R>(
        &mut self,
        run: impl FnOnce(&mut World, &mut crate::sim::exec::GeologyExecState) -> R,
    ) -> R {
        let mut geology_state = self.exec_scratch.geology_dynamics.take();
        let result = run(self, &mut geology_state);
        self.exec_scratch.geology_dynamics = geology_state;
        result
    }

    pub fn matched_geology_dynamics(&self) -> Option<&GeologyDynamicsState> {
        self.exec_scratch
            .geology_dynamics
            .as_ref()
            .filter(|state| state.vertex_states.len() == self.state.geology.height.len())
    }

    pub fn position(&self, index: usize) -> Option<[f32; 3]> {
        self.mesh().positions.get(index).copied()
    }

    pub fn latitude(&self, index: usize) -> f32 {
        self.projections
            .terrain
            .latitude
            .get(index)
            .copied()
            .unwrap_or(0.0)
    }

    pub fn distance_from_ocean(&self, index: usize) -> f32 {
        self.projections
            .terrain
            .distance_from_ocean
            .get(index)
            .copied()
            .unwrap_or(0.0)
    }

    pub fn coast_side(&self, index: usize) -> CoastSide {
        self.projections
            .terrain
            .coast_side
            .get(index)
            .copied()
            .unwrap_or(CoastSide::None)
    }

    pub fn is_coastal(&self, index: usize) -> bool {
        self.projections
            .terrain
            .is_coastal
            .get(index)
            .copied()
            .unwrap_or(false)
    }

    pub fn coastal_flags(&self) -> &[bool] {
        &self.projections.terrain.is_coastal
    }

    pub fn distance_from_ocean_values(&self) -> &[f32] {
        &self.projections.terrain.distance_from_ocean
    }

    pub fn cell_neighbors(&self, index: usize) -> &[u32] {
        let start = self.mesh().nbr_offsets.get(index).copied().unwrap_or(0) as usize;
        let end = self
            .mesh()
            .nbr_offsets
            .get(index + 1)
            .copied()
            .unwrap_or(start as u32) as usize;
        self.mesh().nbrs.get(start..end).unwrap_or(&[])
    }

    pub fn heights(&self) -> &[f32] {
        &self.state.geology.height
    }

    pub fn sea_level_offset(&self) -> f32 {
        self.control.sea_level_offset
    }

    pub fn surface_elevation(&self, index: usize) -> Option<f32> {
        let height = self.heights().get(index).copied()?;
        let ice = self
            .state
            .glaciology
            .ice_thickness
            .get(index)
            .copied()
            .unwrap_or(0.0);
        Some(height + ice - self.sea_level_offset())
    }

    pub fn is_land_cell(&self, index: usize) -> bool {
        self.surface_elevation(index)
            .map(|surface_elevation| surface_elevation > 0.0)
            .unwrap_or(false)
    }

    pub fn runoff(&self) -> &[f32] {
        &self.state.climate.runoff
    }

    pub fn river_flow(&self) -> &[f32] {
        &self.state.hydrology.river_flow
    }

    pub fn river_next(&self) -> &[i32] {
        &self.state.hydrology.river_next
    }
}

impl WorldState {
    pub fn civilization_state(&self) -> CivilizationState<'_> {
        CivilizationState {
            population: &self.population,
            settlement: &self.settlement,
            polity: &self.polity,
            conflict: &self.conflict,
        }
    }

    pub fn civilization_state_mut(&mut self) -> CivilizationStateMut<'_> {
        CivilizationStateMut {
            population: &mut self.population,
            settlement: &mut self.settlement,
            polity: &mut self.polity,
            conflict: &mut self.conflict,
        }
    }
}

impl<'a> CellStore<'a> {
    pub fn len(&self) -> usize {
        self.height.len()
    }

    pub fn is_empty(&self) -> bool {
        self.height.is_empty()
    }

    pub fn is_land_cell(&self, index: usize, sea_level_offset: f32) -> bool {
        self.height
            .get(index)
            .copied()
            .map(|height| height > sea_level_offset)
            .unwrap_or(false)
    }

    pub fn cell_neighbors(&self, index: usize) -> &[u32] {
        let start = self.neighbors_offsets.get(index).copied().unwrap_or(0) as usize;
        let end = self
            .neighbors_offsets
            .get(index + 1)
            .copied()
            .unwrap_or(start as u32) as usize;
        self.neighbors.get(start..end).unwrap_or(&[])
    }
}

impl<'a> CellStoreMut<'a> {
    pub fn len(&self) -> usize {
        self.height.len()
    }

    pub fn is_empty(&self) -> bool {
        self.height.is_empty()
    }

    pub fn apply_hydrology_view(
        &mut self,
        state: &crate::sim::erosion::ErosionAutomatonState,
    ) -> Result<(), String> {
        let expected = self.len();
        if state.height.len() != expected
            || state.river_flux.len() != expected
            || state.river_next.len() != expected
        {
            return Err("river erosion state length does not match core cell count".to_string());
        }
        self.height.clone_from(&state.height);
        self.river_flow.clone_from(&state.river_flux);
        self.river_next.clone_from(&state.river_next);
        Ok(())
    }
}

impl CivilizationState<'_> {
    pub fn indicators(&self) -> CivilizationIndicators {
        CivilizationIndicators {
            settled_cells: self
                .population
                .population
                .iter()
                .filter(|&&value| value >= 10.0)
                .count(),
            total_population: self.population.population.iter().copied().sum::<f32>(),
            state_cells: self
                .polity
                .polity_id
                .iter()
                .filter(|id| id.is_some())
                .count(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeologyDynamicsState {
    #[serde(default)]
    pub update_index: u64,
    #[serde(default)]
    pub plate_states: Vec<PlateKinematicsState>,
    #[serde(default)]
    pub vertex_states: Vec<VertexCrustState>,
    #[serde(default)]
    pub boundary_state: BoundaryDynamicsState,
    #[serde(default)]
    pub mantle_heat: Vec<f32>,
    #[serde(default)]
    pub cached_metrics: GeologyStepMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlateKinematicsState {
    pub angular_axis: [f32; 3],
    pub angular_speed: f32,
    #[serde(default)]
    pub reference_angular_speed: f32,
    #[serde(default)]
    pub slab_pull_drive: f32,
    #[serde(default)]
    pub ridge_push_drive: f32,
    #[serde(default)]
    pub collision_drag: f32,
    #[serde(default)]
    pub force_target_speed_km_per_myr: f32,
    #[serde(default)]
    pub basal_target_speed_km_per_myr: f32,
    #[serde(default)]
    pub phase_offset: f32,
    pub activity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BoundaryType {
    Ridge,
    Rift,
    Subduction,
    Collision,
    Transform,
    #[default]
    PassiveMargin,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VertexCrustState {
    #[serde(default)]
    pub crust_type: CrustType,
    #[serde(default = "default_thickness")]
    pub thickness: f32,
    #[serde(default = "default_density")]
    pub density: f32,
    #[serde(default)]
    pub age: f32,
    #[serde(default)]
    pub stress: f32,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default = "default_rigidity")]
    pub rigidity: f32,
    #[serde(default)]
    pub arc_volcanism: f32,
    #[serde(default)]
    pub ridge_volcanism: f32,
    #[serde(default)]
    pub hotspot_volcanism: f32,
    #[serde(default)]
    pub backarc_volcanism: f32,
    #[serde(default)]
    pub stress_tensor: StressTensor,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct BoundaryEdgeInternal {
    #[serde(default)]
    pub convergence_memory: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BoundaryDynamicsState {
    #[serde(default = "default_reclassify_interval")]
    pub reclassify_interval_ticks: u32,
    #[serde(default)]
    pub steps_since_reclassify: u32,
    #[serde(default)]
    pub dominant_type: Vec<BoundaryType>,
    #[serde(default)]
    pub activity: Vec<f32>,
    #[serde(default)]
    pub edge_pairs: Vec<[u32; 2]>,
    #[serde(default)]
    pub edge_pairs_plate_hash: u64,
    #[serde(default)]
    pub edge_internal: Vec<BoundaryEdgeInternal>,
    #[serde(default)]
    pub rollback_fraction: Vec<f32>,
    #[serde(default)]
    pub backarc_tension: Vec<f32>,
    #[serde(default)]
    pub slab_convergence_component: Vec<f32>,
    #[serde(default)]
    pub slab_rollback_component: Vec<f32>,
    #[serde(default)]
    pub convergence_component: Vec<f32>,
    #[serde(default)]
    pub divergence_component: Vec<f32>,
    #[serde(default)]
    pub transform_component: Vec<f32>,
    #[serde(default)]
    pub obliquity: Vec<f32>,
    #[serde(default)]
    pub subduction_gate: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct GeologyStepMetrics {
    pub geology_activity: f32,
    pub boundary_activity: f32,
    pub plate_id_churn_rate: f32,
    pub boundary_crossing_substeps: f32,
    pub orphan_cell_count: f32,
    pub single_cell_plate_count: f32,
    pub activity_scale: f32,
    pub runtime_rebuild_applied: f32,
    pub mean_abs_surface_write_delta: f32,
    pub mean_signed_surface_write_delta: f32,
    pub min_surface_write_delta: f32,
    pub max_surface_write_delta: f32,
    pub mean_abs_surface_range_clamp_delta: f32,
    pub mean_abs_surface_raw_delta: f32,
    pub mean_abs_surface_step_delta: f32,
    pub mean_abs_surface_step_clamp_delta: f32,
    pub mean_abs_surface_pre_isostatic_delta: f32,
    pub mean_abs_surface_output_delta: f32,
    pub mean_abs_surface_pre_zero_mean_delta: f32,
    pub mean_abs_surface_zero_mean_delta: f32,
    pub debug_surface_max_delta_index: f32,
    pub debug_surface_max_delta_raw_delta: f32,
    pub debug_surface_max_delta_step_delta: f32,
    pub debug_surface_max_delta_thermal_subsidence: f32,
    pub debug_surface_max_delta_diffusive: f32,
    pub debug_surface_max_delta_uplift: f32,
    pub debug_surface_max_delta_tectonic_subsidence: f32,
    pub debug_surface_max_delta_tensile: f32,
    pub debug_surface_max_delta_stress: f32,
    pub debug_surface_max_delta_height_before: f32,
    pub debug_surface_max_delta_height_after_pre_isostatic: f32,
    pub uplift_rate: f32,
    pub subsidence_rate: f32,
    pub smoothing_limited_cells_ratio: f32,
    pub mean_smoothing_factor: f32,
    pub zero_mean_adjusted_cells_ratio: f32,
    pub zero_mean_mean_abs_correction: f32,
    pub zero_mean_std_delta: f32,
    pub mean_compressive: f32,
    pub mean_tensile: f32,
    pub mean_abs_tectonic_uplift: f32,
    pub mean_abs_volcanic_uplift: f32,
    pub mean_abs_tectonic_subsidence: f32,
    pub mean_abs_thermal_subsidence: f32,
    pub mean_abs_thickness_equilibrium_gap: f32,
    pub mean_abs_isostatic_equilibrium_gap: f32,
    pub mean_abs_isostatic_reference_freeboard: f32,
    pub mean_abs_isostatic_compensated_anomaly: f32,
    pub mean_density_ratio: f32,
    pub mean_abs_diffusive_raw: f32,
    pub mean_abs_diffusive_applied: f32,
    pub mean_abs_diffusive_land_down_raw: f32,
    pub mean_abs_diffusive_land_up_raw: f32,
    pub mean_abs_diffusive_ocean_down_raw: f32,
    pub mean_abs_diffusive_ocean_up_raw: f32,
    pub mean_abs_diffusive_ocean_up_applied: f32,
    pub mean_abs_isostatic_raw: f32,
    pub mean_abs_isostatic_applied: f32,
    pub mean_abs_isostatic_reference_freeboard_applied: f32,
    pub mean_abs_isostatic_compensated_anomaly_applied: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_oceanic: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_continental: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_continental_orogenic: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_continental_stable: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_continental_stable_rift: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_transform: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_margin: f32,
    pub mean_signed_isostatic_reference_freeboard_applied_continental_stable_transform: f32,
    pub mean_signed_isostatic_reference_freeboard_raw_continental_stable_passive_margin: f32,
    pub mean_signed_isostatic_reference_freeboard_raw_continental_stable_transform: f32,
    pub passive_margin_continental_cell_ratio: f32,
    pub mean_passive_margin_isostatic_adjustment_rate: f32,
    pub mean_passive_margin_smoothing_factor: f32,
    pub passive_margin_reference_freeboard_effective_applied_factor: f32,
    pub crust_recentering_shift: f32,
    pub crust_recentering_pre_band_ratio: f32,
    pub crust_recentering_post_band_ratio: f32,
    pub bedrock_zero_level_coastal_band_ratio: f32,
    pub bedrock_freeboard_p10: f32,
    pub bedrock_freeboard_p50: f32,
    pub bedrock_freeboard_p90: f32,
}

fn default_thickness() -> f32 {
    0.6
}

fn default_density() -> f32 {
    0.5
}

fn default_rigidity() -> f32 {
    0.7
}

fn default_reclassify_interval() -> u32 {
    4
}
