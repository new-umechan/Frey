use serde::{Deserialize, Serialize};

use super::exec::ExecState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub mesh: WorldMesh,
    pub state: WorldState,
    pub exec: ExecState,
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
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcologyState {
    pub vegetation: Vec<f32>,
    pub habitability: Vec<f32>,
    pub productivity: Vec<f32>,
    pub riparian_vegetation: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomesticatesState {
    pub crop_available: Vec<u32>,
    pub crop_adopted: Vec<u32>,
    pub livestock_available: Vec<u32>,
    pub livestock_adopted: Vec<u32>,
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
