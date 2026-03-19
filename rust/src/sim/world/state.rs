use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::exec::{ClockState, FeedbackQueue, RuntimeState};
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
    pub polity_relations: HashMap<(u32, u32), PolityRelation>,
    #[serde(default)]
    pub polity_groups: Vec<PolityGroup>,
    #[serde(default)]
    pub archive: ArchiveState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ArchiveState {
    #[serde(default)]
    pub history_ticks: BTreeMap<u64, String>,
    #[serde(default)]
    pub snapshots: BTreeMap<String, SnapshotMeta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub tick: u64,
    #[serde(default)]
    pub source_world_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolityComponent {
    pub polity_id: u32,
    pub capital_cell: u32,
    pub stability: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementComponent {
    pub settlement_id: u32,
    pub cell: u32,
    pub size: f32,
    pub urbanization: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionComponent {
    pub region_id: u32,
    pub cells: Vec<u32>,
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
    pub latitude_deg: Vec<f32>,
    pub distance_from_ocean_km: Vec<f32>,
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
    pub plate_id: Vec<u16>,
    pub erosion_rate: Vec<f32>,
    pub deposition_rate: Vec<f32>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydrologyState {
    pub river_path: Vec<i32>,
    pub river_flow: Vec<f32>,
    pub river_transport_cost: Vec<f32>,
    #[serde(default)]
    pub river_upstream: Vec<i32>,
    #[serde(default)]
    pub river_downstream: Vec<i32>,
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
    pub crop_available: Vec<u32>,
    pub crop_adopted: Vec<u32>,
    pub livestock_available: Vec<u32>,
    pub livestock_adopted: Vec<u32>,
    #[serde(default)]
    pub domesticates_internal: Vec<DomesticatesInternal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DomesticatesInternal {
    pub diffusion_memory: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubsistenceState {
    pub subsistence_mix: Vec<f32>,
    pub food_production: Vec<f32>,
    pub land_use: Vec<f32>,
    pub water_withdrawal: Vec<f32>,
    pub dam_pressure: Vec<f32>,
    pub pollution: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopulationState {
    pub population: Vec<f32>,
    pub population_density: Vec<f32>,
    pub migration_pressure: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementState {
    pub settlement_size: Vec<f32>,
    pub urbanization: Vec<f32>,
    pub centrality: Vec<f32>,
    pub residence: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolityState {
    pub polity_id: Vec<u32>,
    pub territory_status: Vec<u8>,
    pub language_group: Vec<u16>,
    pub polity_stability: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictState {
    pub war_state: Vec<u8>,
    pub occupier_id: Vec<u32>,
    pub frontline: Vec<f32>,
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
            state_cells: self.polity.polity_id.iter().filter(|&&id| id > 0).count(),
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
pub enum CrustType {
    #[default]
    Continental,
    Oceanic,
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct StressTensor {
    pub xx: f32,
    pub yy: f32,
    pub xy: f32,
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
    pub stress_tensor: StressTensor,
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
