use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;

use super::exec::{ClockState, FeedbackQueue, RuntimeState};
use crate::sim::geology_types::{CrustType, GeologyInternal, PlateId, PlateRelation, StressTensor};
use crate::sim::polity::types::{PolityGroup, PolityRelation};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub mesh: WorldMesh,
    pub state: WorldState,
    pub entities: EntitiesState,
    pub clock: ClockState,
    pub feedback: FeedbackQueue,
    pub runtime: RuntimeState,
    #[serde(default)]
    pub polity_relations: HashMap<(PolityId, PolityId), PolityRelation>,
    #[serde(default)]
    pub polity_groups: Vec<PolityGroup>,
    #[serde(default)]
    pub plate_relations: HashMap<(PlateId, PlateId), PlateRelation>,
    #[serde(default)]
    pub archive: ArchiveState,
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

pub struct EntitiesState {
    pub polity_components: Vec<PolityComponent>,
    pub settlement_components: Vec<SettlementComponent>,
    pub region_components: Vec<RegionComponent>,
    pub world: hecs::World,
}

#[derive(Serialize, Deserialize)]
struct EntitiesSerde {
    polity_components: Vec<PolityComponent>,
    settlement_components: Vec<SettlementComponent>,
    region_components: Vec<RegionComponent>,
}

impl std::fmt::Debug for EntitiesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntitiesState")
            .field("polity_components", &self.polity_components)
            .field("settlement_components", &self.settlement_components)
            .field("region_components", &self.region_components)
            .finish()
    }
}

impl Clone for EntitiesState {
    fn clone(&self) -> Self {
        Self::from_components(
            self.polity_components.clone(),
            self.settlement_components.clone(),
            self.region_components.clone(),
        )
    }
}

impl PartialEq for EntitiesState {
    fn eq(&self, other: &Self) -> bool {
        self.polity_components == other.polity_components
            && self.settlement_components == other.settlement_components
            && self.region_components == other.region_components
    }
}

impl Default for EntitiesState {
    fn default() -> Self {
        Self::from_components(Vec::new(), Vec::new(), Vec::new())
    }
}

impl Serialize for EntitiesState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        EntitiesSerde {
            polity_components: self.polity_components.clone(),
            settlement_components: self.settlement_components.clone(),
            region_components: self.region_components.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EntitiesState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed = EntitiesSerde::deserialize(deserializer)?;
        Ok(Self::from_components(
            parsed.polity_components,
            parsed.settlement_components,
            parsed.region_components,
        ))
    }
}

impl EntitiesState {
    pub fn from_components(
        polity_components: Vec<PolityComponent>,
        settlement_components: Vec<SettlementComponent>,
        region_components: Vec<RegionComponent>,
    ) -> Self {
        let mut entities = Self {
            polity_components,
            settlement_components,
            region_components,
            world: hecs::World::new(),
        };
        entities.sync_world_from_components();
        entities
    }

    pub fn sync_world_from_components(&mut self) {
        self.world = hecs::World::new();
        for component in &self.polity_components {
            self.world.spawn((component.clone(),));
        }
        for component in &self.settlement_components {
            self.world.spawn((component.clone(),));
        }
        for component in &self.region_components {
            self.world.spawn((component.clone(),));
        }
    }

    pub fn sync_components_from_world(&mut self) {
        self.polity_components.clear();
        self.settlement_components.clear();
        self.region_components.clear();

        for (_, component) in self.world.query::<&PolityComponent>().iter() {
            self.polity_components.push(component.clone());
        }
        for (_, component) in self.world.query::<&SettlementComponent>().iter() {
            self.settlement_components.push(component.clone());
        }
        for (_, component) in self.world.query::<&RegionComponent>().iter() {
            self.region_components.push(component.clone());
        }

        self.polity_components.sort_by_key(|c| c.polity_id);
        self.settlement_components.sort_by_key(|c| c.settlement_id);
        self.region_components.sort_by_key(|c| c.region_id);
    }

    pub fn replace_polities(&mut self, components: Vec<PolityComponent>) {
        self.polity_components = components;
        self.sync_world_from_components();
    }

    pub fn replace_settlements(&mut self, components: Vec<SettlementComponent>) {
        self.settlement_components = components;
        self.sync_world_from_components();
    }

    pub fn replace_regions(&mut self, components: Vec<RegionComponent>) {
        self.region_components = components;
        self.sync_world_from_components();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldMesh {
    pub positions: Vec<[f32; 3]>,
    pub nbr_offsets: Vec<u32>,
    pub nbrs: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldState {
    pub geo: GeoState,
    pub geology: GeologyState,
    pub climate: ClimateState,
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
pub struct GeoState {
    #[serde(alias = "latitude_deg")]
    pub latitude: Vec<f32>,
    #[serde(alias = "distance_from_ocean_km")]
    pub distance_from_ocean: Vec<f32>,
    pub coast_side: Vec<CoastSide>,
    pub is_coastal: Vec<bool>,
    #[serde(default)]
    pub neighbors_offsets: Vec<u32>,
    #[serde(default)]
    pub neighbors: Vec<u32>,
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
    pub erosion_rate: Vec<f32>,
    pub deposition_rate: Vec<f32>,
    #[serde(default)]
    pub volcanism: Vec<f32>,
    #[serde(default)]
    pub vertex_buoyancy: Vec<f32>,
    #[serde(default)]
    pub geology_internal: Vec<GeologyInternal>,
    pub boundary_condition: Vec<f32>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydrologyState {
    pub river_downstream: Vec<SmallVec<[(u32, f32); 3]>>,
    #[serde(default)]
    pub river_next: Vec<i32>,
    pub river_flow: Vec<f32>,
    pub river_transport_cost: Vec<f32>,
    #[serde(default)]
    pub is_lake: Vec<bool>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DomesticatesInternal {
    pub diffusion_memory: f32,
}

pub type CropBitmap = u8;
pub type LivestockBitmap = u8;
pub const N_CROPS: usize = 7;
pub const N_LIVESTOCK: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SubsistenceMix {
    pub gathering: f32,
    pub hunting: f32,
    pub fishing: f32,
    pub farming: f32,
    pub pastoralism: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubsistenceState {
    pub subsistence_mix: Vec<SubsistenceMix>,
    pub food_production: Vec<f32>,
    pub freshwater_access: Vec<f32>,
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
    pub latitude: &'a [f32],
    pub distance_from_ocean: &'a [f32],
    pub coast_side: &'a [CoastSide],
    pub is_coastal: &'a [bool],
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
    pub river_downstream: &'a [SmallVec<[(u32, f32); 3]>],
    pub river_next: &'a [i32],
    pub river_flow: &'a [f32],
    pub river_transport_cost: &'a [f32],
    pub is_lake: &'a [bool],
}

pub struct CellStoreMut<'a> {
    pub latitude: &'a mut Vec<f32>,
    pub distance_from_ocean: &'a mut Vec<f32>,
    pub coast_side: &'a mut Vec<CoastSide>,
    pub is_coastal: &'a mut Vec<bool>,
    pub neighbors_offsets: &'a mut Vec<u32>,
    pub neighbors: &'a mut Vec<u32>,
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
    pub river_downstream: &'a mut Vec<SmallVec<[(u32, f32); 3]>>,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CivilizationIndicators {
    pub settled_cells: usize,
    pub total_population: f32,
    pub state_cells: usize,
}

impl WorldState {
    pub fn cell_store(&self) -> CellStore<'_> {
        CellStore {
            latitude: &self.geo.latitude,
            distance_from_ocean: &self.geo.distance_from_ocean,
            coast_side: &self.geo.coast_side,
            is_coastal: &self.geo.is_coastal,
            neighbors_offsets: &self.geo.neighbors_offsets,
            neighbors: &self.geo.neighbors,
            height: &self.geology.height,
            lake_depth: &self.geology.lake_depth,
            plate_id: &self.geology.plate_id,
            erosion_rate: &self.geology.erosion_rate,
            deposition_rate: &self.geology.deposition_rate,
            volcanism: &self.geology.volcanism,
            vertex_buoyancy: &self.geology.vertex_buoyancy,
            geology_internal: &self.geology.geology_internal,
            temperature: &self.climate.temperature,
            precipitation: &self.climate.precipitation,
            evapotranspiration: &self.climate.evapotranspiration,
            runoff: &self.climate.runoff,
            aridity: &self.climate.aridity,
            ocean_temperature: &self.climate.ocean_temperature,
            precipitable_water: &self.climate.precipitable_water,
            cloud_water: &self.climate.cloud_water,
            wind_u: &self.climate.wind_u,
            wind_v: &self.climate.wind_v,
            moisture_flux_u: &self.climate.moisture_flux_u,
            moisture_flux_v: &self.climate.moisture_flux_v,
            river_downstream: &self.hydrology.river_downstream,
            river_next: &self.hydrology.river_next,
            river_flow: &self.hydrology.river_flow,
            river_transport_cost: &self.hydrology.river_transport_cost,
            is_lake: &self.hydrology.is_lake,
        }
    }

    pub fn cell_store_mut(&mut self) -> CellStoreMut<'_> {
        CellStoreMut {
            latitude: &mut self.geo.latitude,
            distance_from_ocean: &mut self.geo.distance_from_ocean,
            coast_side: &mut self.geo.coast_side,
            is_coastal: &mut self.geo.is_coastal,
            neighbors_offsets: &mut self.geo.neighbors_offsets,
            neighbors: &mut self.geo.neighbors,
            height: &mut self.geology.height,
            lake_depth: &mut self.geology.lake_depth,
            plate_id: &mut self.geology.plate_id,
            erosion_rate: &mut self.geology.erosion_rate,
            deposition_rate: &mut self.geology.deposition_rate,
            volcanism: &mut self.geology.volcanism,
            vertex_buoyancy: &mut self.geology.vertex_buoyancy,
            geology_internal: &mut self.geology.geology_internal,
            temperature: &mut self.climate.temperature,
            precipitation: &mut self.climate.precipitation,
            evapotranspiration: &mut self.climate.evapotranspiration,
            runoff: &mut self.climate.runoff,
            aridity: &mut self.climate.aridity,
            ocean_temperature: &mut self.climate.ocean_temperature,
            precipitable_water: &mut self.climate.precipitable_water,
            cloud_water: &mut self.climate.cloud_water,
            wind_u: &mut self.climate.wind_u,
            wind_v: &mut self.climate.wind_v,
            moisture_flux_u: &mut self.climate.moisture_flux_u,
            moisture_flux_v: &mut self.climate.moisture_flux_v,
            river_downstream: &mut self.hydrology.river_downstream,
            river_next: &mut self.hydrology.river_next,
            river_flow: &mut self.hydrology.river_flow,
            river_transport_cost: &mut self.hydrology.river_transport_cost,
            is_lake: &mut self.hydrology.is_lake,
        }
    }

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
    pub edge_internal: Vec<BoundaryEdgeInternal>,
    #[serde(default)]
    pub rollback_fraction: Vec<f32>,
    #[serde(default)]
    pub backarc_tension: Vec<f32>,
    #[serde(default)]
    pub slab_convergence_component: Vec<f32>,
    #[serde(default)]
    pub slab_rollback_component: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct GeologyStepMetrics {
    pub geology_activity: f32,
    pub boundary_activity: f32,
    pub uplift_rate: f32,
    pub subsidence_rate: f32,
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
