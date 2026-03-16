use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

use serde::{Deserialize, Serialize};

use super::erosion::ErosionAutomatonState;
use super::geo::{
    add3, dot3, east_direction, edge_distance_km, normalize3, project_to_tangent, sub3,
    EARTH_RADIUS_KM,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EraKind {
    Crust,
    Environment,
    Life,
    Civilization,
    History,
}

impl EraKind {
    pub fn as_key(self) -> &'static str {
        match self {
            EraKind::Crust => "crust",
            EraKind::Environment => "environment",
            EraKind::Life => "life",
            EraKind::Civilization => "civilization",
            EraKind::History => "history",
        }
    }

    pub fn budgets(self) -> SubsystemBudgets {
        match self {
            EraKind::Crust => SubsystemBudgets {
                geology: 4,
                climate: 0,
                ecology: 0,
                civilization: 0,
            },
            EraKind::Environment => SubsystemBudgets {
                geology: 3,
                climate: 3,
                ecology: 1,
                civilization: 0,
            },
            EraKind::Life => SubsystemBudgets {
                geology: 2,
                climate: 3,
                ecology: 4,
                civilization: 1,
            },
            EraKind::Civilization => SubsystemBudgets {
                geology: 1,
                climate: 2,
                ecology: 2,
                civilization: 4,
            },
            EraKind::History => SubsystemBudgets {
                geology: 1,
                climate: 1,
                ecology: 1,
                civilization: 4,
            },
        }
    }

    pub fn real_years_per_tick(self) -> f32 {
        match self {
            EraKind::Crust => 5_000_000.0,
            EraKind::Environment => 10_000.0,
            EraKind::Life => 1_000.0,
            EraKind::Civilization => 100.0,
            EraKind::History => 1.0,
        }
    }

    pub fn runtime_tick_ms(self) -> u32 {
        match self {
            EraKind::Crust => 70,
            EraKind::Environment => 150,
            EraKind::Life => 110,
            EraKind::Civilization => 90,
            EraKind::History => 70,
        }
    }
}

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
    pub ecology: EcologyState,
    pub civilization: CivilizationState,
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
pub struct EcologyState {
    pub vegetation: Vec<f32>,
    pub habitability: Vec<f32>,
    pub productivity: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CivilizationState {
    pub population: Vec<f32>,
    pub state_id: Vec<u32>,
    pub agriculture: Vec<f32>,
    pub water_withdrawal: Vec<f32>,
    pub dam_level: Vec<f32>,
    pub pollution: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecState {
    pub tick: u64,
    pub era: EraKind,
    pub real_years_per_tick: f32,
    pub runtime_tick_ms: u32,
    #[serde(default = "default_target_sea_ratio")]
    pub target_sea_ratio: f32,
    pub budgets: SubsystemBudgets,
    pub feedback_queue: FeedbackQueue,
    pub transition: TransitionState,
    #[serde(default)]
    pub terrain_dynamics: Option<TerrainDynamicsState>,
    #[serde(default)]
    pub river_erosion_state: Option<ErosionAutomatonState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SubsystemBudgets {
    pub geology: u32,
    pub climate: u32,
    pub ecology: u32,
    pub civilization: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackQueue {
    pub active: FeedbackFields,
    pub pending: FeedbackFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackFields {
    pub water_withdrawal: Vec<f32>,
    pub dam_pressure: Vec<f32>,
    pub pollution: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionState {
    pub era_enter_tick: u64,
    pub stable_ticks_in_era: u32,
    pub last_land_ratio: f32,
    pub ema_geology_activity: f32,
    pub ema_climate_activity: f32,
    pub ema_ecology_activity: f32,
    pub ema_civilization_activity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainDynamicsState {
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
    pub cached_metrics: TerrainStepMetrics,
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
pub struct TerrainStepMetrics {
    pub terrain_activity: f32,
    pub boundary_activity: f32,
    pub uplift_rate: f32,
    pub subsidence_rate: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldMetrics {
    pub cell_count: u32,
    pub land_cells: u32,
    pub land_ratio: f32,
    pub mean_height: f32,
    pub height_std_dev: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub mean_river_flux: f32,
    pub max_river_flux: f32,
    pub top10_river_flux_sum: f32,
    pub river_active_cells: u32,
    pub river_fragmentation_ratio: f32,
    pub river_ocean_reach_ratio: f32,
    pub river_mainstem_persistence: f32,
    pub river_flux_concentration: f32,
    pub continent_count: u32,
    pub largest_continent_cells: u32,
}

impl World {
    pub fn new(mesh: WorldMesh, geology: GeologyState) -> Self {
        let cell_count = geology.height.len();
        let (land_ratio, target_sea_ratio) = land_and_sea_ratios(&geology.height);
        let geo = build_geo_state(&mesh, &geology.height);
        let ocean_temperature = geo
            .latitude_deg
            .iter()
            .map(|&latitude_deg| base_ocean_temperature(latitude_deg))
            .collect::<Vec<_>>();
        let era = EraKind::Crust;
        Self {
            mesh,
            state: WorldState {
                geo,
                geology,
                climate: ClimateState {
                    temperature: vec![15.0; cell_count],
                    precipitation: vec![800.0; cell_count],
                    evapotranspiration: vec![400.0; cell_count],
                    runoff: vec![200.0; cell_count],
                    aridity: vec![1.0; cell_count],
                    ocean_temperature,
                },
                ecology: EcologyState {
                    vegetation: vec![0.0; cell_count],
                    habitability: vec![0.0; cell_count],
                    productivity: vec![0.0; cell_count],
                },
                civilization: CivilizationState {
                    population: vec![0.0; cell_count],
                    state_id: vec![0; cell_count],
                    agriculture: vec![0.0; cell_count],
                    water_withdrawal: vec![0.0; cell_count],
                    dam_level: vec![0.0; cell_count],
                    pollution: vec![0.0; cell_count],
                },
            },
            exec: ExecState {
                tick: 0,
                era,
                real_years_per_tick: era.real_years_per_tick(),
                runtime_tick_ms: era.runtime_tick_ms(),
                target_sea_ratio,
                budgets: era.budgets(),
                feedback_queue: FeedbackQueue::new(cell_count),
                transition: TransitionState {
                    era_enter_tick: 0,
                    stable_ticks_in_era: 0,
                    last_land_ratio: land_ratio,
                    ema_geology_activity: 1.0,
                    ema_climate_activity: 1.0,
                    ema_ecology_activity: 1.0,
                    ema_civilization_activity: 1.0,
                },
                terrain_dynamics: None,
                river_erosion_state: None,
            },
        }
    }

    pub fn cell_count(&self) -> usize {
        self.state.geology.height.len()
    }

    pub fn attach_river_erosion_state(
        &mut self,
        state: ErosionAutomatonState,
    ) -> Result<(), String> {
        let expected = self.state.geology.height.len();
        if state.height.len() != expected
            || state.river_flux.len() != expected
            || state.river_next.len() != expected
        {
            return Err("river erosion state length does not match core cell count".to_string());
        }
        self.state.geology.height = state.height.clone();
        self.state.geology.river_flux = state.river_flux.clone();
        self.state.geology.river_next = state.river_next.clone();
        self.exec.river_erosion_state = Some(state);
        Ok(())
    }

    pub fn metrics(&self) -> WorldMetrics {
        let height = &self.state.geology.height;
        let river_flux = &self.state.geology.river_flux;
        let cell_count = height.len();
        if cell_count == 0 {
            return WorldMetrics::default();
        }

        let mut land_cells = 0usize;
        let mut min_height = f32::INFINITY;
        let mut max_height = f32::NEG_INFINITY;
        let mut sum_height = 0.0f32;
        let mut sum_height_sq = 0.0f32;
        let mut sum_flux = 0.0f32;
        let mut max_flux = 0.0f32;
        let mut top_fluxes = [0.0f32; 10];
        let mut top_fluxes_len = 0usize;

        for i in 0..cell_count {
            let h = height[i];
            let flux = river_flux.get(i).copied().unwrap_or(0.0).max(0.0);
            if h > 0.0 {
                land_cells += 1;
            }
            min_height = min_height.min(h);
            max_height = max_height.max(h);
            sum_height += h;
            sum_height_sq += h * h;
            sum_flux += flux;
            max_flux = max_flux.max(flux);
            push_top_flux(&mut top_fluxes, &mut top_fluxes_len, flux);
        }

        let cell_count_f32 = cell_count as f32;
        let mean_height = sum_height / cell_count_f32;
        let variance = (sum_height_sq / cell_count_f32) - (mean_height * mean_height);
        let height_std_dev = variance.max(0.0).sqrt();

        let (continent_count, largest_continent_cells) = continent_stats(self);
        let top10_river_flux_sum = top_fluxes.iter().take(top_fluxes_len).sum::<f32>();
        let (
            river_active_cells,
            river_fragmentation_ratio,
            river_ocean_reach_ratio,
            river_mainstem_persistence,
            river_flux_concentration,
        ) = river_network_metrics(
            &self.state.geology.height,
            &self.state.geology.river_flux,
            &self.state.geology.river_next,
            top10_river_flux_sum,
            sum_flux,
            max_flux,
        );

        WorldMetrics {
            cell_count: cell_count as u32,
            land_cells: land_cells as u32,
            land_ratio: land_cells as f32 / cell_count_f32,
            mean_height,
            height_std_dev,
            min_height: if min_height.is_finite() {
                min_height
            } else {
                0.0
            },
            max_height: if max_height.is_finite() {
                max_height
            } else {
                0.0
            },
            mean_river_flux: sum_flux / cell_count_f32,
            max_river_flux: max_flux,
            top10_river_flux_sum,
            river_active_cells,
            river_fragmentation_ratio,
            river_ocean_reach_ratio,
            river_mainstem_persistence,
            river_flux_concentration,
            continent_count: continent_count as u32,
            largest_continent_cells: largest_continent_cells as u32,
        }
    }
}

fn land_and_sea_ratios(height: &[f32]) -> (f32, f32) {
    if height.is_empty() {
        return (1.0 - default_target_sea_ratio(), default_target_sea_ratio());
    }

    let sea_count = height.iter().filter(|&&h| h <= 0.0).count() as f32;
    let cell_count = height.len() as f32;
    let sea_ratio = sea_count / cell_count;
    (1.0 - sea_ratio, sea_ratio)
}

impl FeedbackQueue {
    pub fn new(cell_count: usize) -> Self {
        Self {
            active: FeedbackFields::zeros(cell_count),
            pending: FeedbackFields::zeros(cell_count),
        }
    }
}

impl FeedbackFields {
    pub fn zeros(cell_count: usize) -> Self {
        Self {
            water_withdrawal: vec![0.0; cell_count],
            dam_pressure: vec![0.0; cell_count],
            pollution: vec![0.0; cell_count],
        }
    }

    pub fn clear(&mut self) {
        self.water_withdrawal.fill(0.0);
        self.dam_pressure.fill(0.0);
        self.pollution.fill(0.0);
    }
}

impl TransitionState {
    pub fn reset_for_era(&mut self, tick: u64, era: EraKind, land_ratio: f32) {
        self.era_enter_tick = tick;
        self.stable_ticks_in_era = 0;
        self.last_land_ratio = land_ratio;
        self.ema_geology_activity = if era == EraKind::Crust { 1.0 } else { 0.0 };
        self.ema_climate_activity = if era == EraKind::Environment {
            1.0
        } else {
            0.0
        };
        self.ema_ecology_activity = if era == EraKind::Life { 1.0 } else { 0.0 };
        self.ema_civilization_activity = if era == EraKind::Civilization {
            1.0
        } else {
            0.0
        };
    }
}

pub fn default_target_sea_ratio() -> f32 {
    0.62
}

fn build_geo_state(mesh: &WorldMesh, height: &[f32]) -> GeoState {
    let cell_count = height.len();
    let mut latitude_deg = vec![0.0; cell_count];
    let mut is_coastal = vec![false; cell_count];

    for i in 0..cell_count {
        let pos = mesh.positions.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
        latitude_deg[i] = pos[1].clamp(-1.0, 1.0).asin().to_degrees();
        let is_land = height[i] > 0.0;
        let start = mesh.nbr_offsets.get(i).copied().unwrap_or(0) as usize;
        let end = mesh.nbr_offsets.get(i + 1).copied().unwrap_or(start as u32) as usize;
        for &n_u32 in mesh.nbrs.get(start..end).unwrap_or(&[]) {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            if (height[n] > 0.0) != is_land {
                is_coastal[i] = true;
                break;
            }
        }
    }

    let distance_from_ocean_km = build_distance_from_ocean_km(mesh, height);
    let mut coast_side = vec![CoastSide::None; cell_count];
    for i in 0..cell_count {
        if !is_coastal[i] {
            continue;
        }
        coast_side[i] = classify_coast_side(mesh, height, i);
    }

    GeoState {
        latitude_deg,
        distance_from_ocean_km,
        coast_side,
        is_coastal,
    }
}

fn build_distance_from_ocean_km(mesh: &WorldMesh, height: &[f32]) -> Vec<f32> {
    let cell_count = height.len();
    let mut distance = vec![f32::INFINITY; cell_count];
    let mut heap = BinaryHeap::new();

    for (i, &elevation) in height.iter().enumerate() {
        if elevation <= 0.0 {
            distance[i] = 0.0;
            heap.push(DistanceNode {
                cost: 0.0,
                index: i,
            });
        }
    }

    if heap.is_empty() {
        return vec![EARTH_HALF_CIRCUMFERENCE_KM; cell_count];
    }

    while let Some(DistanceNode { cost, index }) = heap.pop() {
        if cost > distance[index] {
            continue;
        }
        let start = mesh.nbr_offsets.get(index).copied().unwrap_or(0) as usize;
        let end = mesh
            .nbr_offsets
            .get(index + 1)
            .copied()
            .unwrap_or(start as u32) as usize;
        for &n_u32 in mesh.nbrs.get(start..end).unwrap_or(&[]) {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            let edge_cost = edge_distance_km(
                mesh.positions
                    .get(index)
                    .copied()
                    .unwrap_or([0.0, 0.0, 1.0]),
                mesh.positions.get(n).copied().unwrap_or([0.0, 0.0, 1.0]),
            );
            let next_cost = cost + edge_cost;
            if next_cost < distance[n] {
                distance[n] = next_cost;
                heap.push(DistanceNode {
                    cost: next_cost,
                    index: n,
                });
            }
        }
    }

    distance
        .into_iter()
        .map(|value| {
            if value.is_finite() {
                value
            } else {
                EARTH_HALF_CIRCUMFERENCE_KM
            }
        })
        .collect()
}

fn classify_coast_side(mesh: &WorldMesh, height: &[f32], index: usize) -> CoastSide {
    let cell_count = height.len();
    if index >= cell_count {
        return CoastSide::None;
    }
    let pos = mesh
        .positions
        .get(index)
        .copied()
        .unwrap_or([0.0, 0.0, 1.0]);
    let is_land = height[index] > 0.0;
    let seek_land = !is_land;
    let start = mesh.nbr_offsets.get(index).copied().unwrap_or(0) as usize;
    let end = mesh
        .nbr_offsets
        .get(index + 1)
        .copied()
        .unwrap_or(start as u32) as usize;
    let mut dir_sum = [0.0_f32; 3];

    for &n_u32 in mesh.nbrs.get(start..end).unwrap_or(&[]) {
        let n = n_u32 as usize;
        if n >= cell_count {
            continue;
        }
        if (height[n] > 0.0) != seek_land {
            continue;
        }
        let neighbor = mesh.positions.get(n).copied().unwrap_or([0.0, 0.0, 1.0]);
        let tangent = normalize3(project_to_tangent(sub3(neighbor, pos), pos));
        dir_sum = add3(dir_sum, tangent);
    }

    let east = east_direction(pos);
    let dir = normalize3(dir_sum);
    let dot = dot3(dir, east);
    if dot > 0.15 {
        CoastSide::East
    } else if dot < -0.15 {
        CoastSide::West
    } else {
        CoastSide::None
    }
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

fn base_ocean_temperature(latitude_deg: f32) -> f32 {
    28.0 * latitude_deg.to_radians().cos() - 2.0
}

const EARTH_HALF_CIRCUMFERENCE_KM: f32 = std::f32::consts::PI * EARTH_RADIUS_KM;

#[derive(Debug, Clone, Copy, PartialEq)]
struct DistanceNode {
    cost: f32,
    index: usize,
}

impl Eq for DistanceNode {}

impl Ord for DistanceNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.index.cmp(&other.index))
    }
}

impl PartialOrd for DistanceNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn push_top_flux(top_fluxes: &mut [f32; 10], len: &mut usize, value: f32) {
    if !value.is_finite() || value <= 0.0 {
        return;
    }
    if *len < top_fluxes.len() {
        top_fluxes[*len] = value;
        *len += 1;
        return;
    }
    let mut min_index = 0usize;
    for i in 1..top_fluxes.len() {
        if top_fluxes[i] < top_fluxes[min_index] {
            min_index = i;
        }
    }
    if value > top_fluxes[min_index] {
        top_fluxes[min_index] = value;
    }
}

fn continent_stats(world: &World) -> (usize, usize) {
    let height = &world.state.geology.height;
    let cell_count = height.len();
    if cell_count == 0 {
        return (0, 0);
    }
    let min_continent_cells = ((cell_count as f32) * 0.01).ceil().max(1.0) as usize;
    let mut visited = vec![false; cell_count];
    let mut queue = VecDeque::new();
    let mut continent_count = 0usize;
    let mut largest_continent_cells = 0usize;

    for start_index in 0..cell_count {
        if visited[start_index] || height[start_index] <= 0.0 {
            continue;
        }
        visited[start_index] = true;
        queue.clear();
        queue.push_back(start_index);
        let mut component_size = 0usize;

        while let Some(index) = queue.pop_front() {
            component_size += 1;
            let start = world.mesh.nbr_offsets.get(index).copied().unwrap_or(0) as usize;
            let end = world
                .mesh
                .nbr_offsets
                .get(index + 1)
                .copied()
                .unwrap_or(start as u32) as usize;
            for &neighbor in world.mesh.nbrs.get(start..end).unwrap_or(&[]) {
                let neighbor_index = neighbor as usize;
                if neighbor_index >= cell_count
                    || visited[neighbor_index]
                    || height[neighbor_index] <= 0.0
                {
                    continue;
                }
                visited[neighbor_index] = true;
                queue.push_back(neighbor_index);
            }
        }

        if component_size >= min_continent_cells {
            continent_count += 1;
            largest_continent_cells = largest_continent_cells.max(component_size);
        }
    }

    (continent_count, largest_continent_cells)
}

fn river_network_metrics(
    height: &[f32],
    flux: &[f32],
    river_next: &[i32],
    top10_river_flux_sum: f32,
    sum_flux: f32,
    max_flux: f32,
) -> (u32, f32, f32, f32, f32) {
    let cell_count = height.len();
    if cell_count == 0 || river_next.len() != cell_count {
        return (0, 0.0, 0.0, 0.0, 0.0);
    }

    let active_threshold = (max_flux * 0.08).max(0.02);
    let mut active = vec![false; cell_count];
    let mut active_cells = 0usize;
    for i in 0..cell_count {
        if height[i] > 0.0 && flux.get(i).copied().unwrap_or(0.0) >= active_threshold {
            active[i] = true;
            active_cells += 1;
        }
    }
    if active_cells == 0 {
        return (0, 0.0, 0.0, 0.0, 0.0);
    }

    let mut upstream_active = vec![0u32; cell_count];
    for i in 0..cell_count {
        if !active[i] {
            continue;
        }
        let next = river_next[i];
        if next < 0 {
            continue;
        }
        let n = next as usize;
        if n < cell_count && active[n] {
            upstream_active[n] = upstream_active[n].saturating_add(1);
        }
    }

    let mut head_cells = Vec::new();
    for i in 0..cell_count {
        if active[i] && upstream_active[i] == 0 {
            head_cells.push(i);
        }
    }
    let fragmentation_ratio = head_cells.len() as f32 / active_cells as f32;

    let mut memo = vec![0u8; cell_count];
    let mut visit_mark = vec![0u32; cell_count];
    let mut run_id = 1u32;
    let mut reaches_ocean_count = 0usize;
    let mut path = Vec::<usize>::new();

    for i in 0..cell_count {
        if !active[i] {
            continue;
        }
        if trace_active_to_ocean(
            i,
            height,
            river_next,
            &active,
            &mut memo,
            &mut visit_mark,
            &mut run_id,
            &mut path,
        ) {
            reaches_ocean_count += 1;
        }
    }

    let mut longest_mainstem = 0usize;
    for &head in &head_cells {
        let mut steps = 0usize;
        let mut current = head;
        let mut guard = 0usize;
        while guard < cell_count {
            guard += 1;
            steps += 1;
            let next = river_next.get(current).copied().unwrap_or(-1);
            if next < 0 {
                break;
            }
            let n = next as usize;
            if n >= cell_count || !active[n] {
                break;
            }
            if n == current {
                break;
            }
            current = n;
        }
        longest_mainstem = longest_mainstem.max(steps);
    }

    let ocean_reach_ratio = reaches_ocean_count as f32 / active_cells as f32;
    let mainstem_persistence = longest_mainstem as f32 / active_cells as f32;
    let flux_concentration = top10_river_flux_sum / sum_flux.max(1e-6);

    (
        active_cells as u32,
        fragmentation_ratio,
        ocean_reach_ratio,
        mainstem_persistence,
        flux_concentration.clamp(0.0, 1.0),
    )
}

fn trace_active_to_ocean(
    start: usize,
    height: &[f32],
    river_next: &[i32],
    active: &[bool],
    memo: &mut [u8],
    visit_mark: &mut [u32],
    run_id: &mut u32,
    path: &mut Vec<usize>,
) -> bool {
    if memo.get(start).copied().unwrap_or(0) == 2 {
        return true;
    }
    if memo.get(start).copied().unwrap_or(0) == 3 {
        return false;
    }

    path.clear();
    let current_run = *run_id;
    *run_id = (*run_id).saturating_add(1).max(1);
    let mut cur = start;
    let mut result = false;

    for _ in 0..height.len() {
        if memo[cur] == 2 {
            result = true;
            break;
        }
        if memo[cur] == 3 || !active[cur] {
            result = false;
            break;
        }
        if visit_mark[cur] == current_run {
            result = false;
            break;
        }
        visit_mark[cur] = current_run;
        path.push(cur);

        let next = river_next.get(cur).copied().unwrap_or(-1);
        if next < 0 {
            result = false;
            break;
        }
        let n = next as usize;
        if n >= height.len() {
            result = false;
            break;
        }
        if height[n] <= 0.0 {
            result = true;
            break;
        }
        cur = n;
    }

    let mark = if result { 2 } else { 3 };
    for &v in path.iter() {
        memo[v] = mark;
    }
    result
}

fn default_reclassify_interval() -> u32 {
    4
}

#[cfg(test)]
mod tests {
    use super::{EraKind, FeedbackQueue, GeologyState, World, WorldMesh};
    use crate::common::mesh::{build_neighbors, generate_icosphere};
    use crate::sim::erosion::ErosionAutomatonState;
    use crate::sim::step_world;
    use crate::TerrainParams;

    fn build_world() -> World {
        World::new(
            WorldMesh {
                positions: vec![[0.0, 0.0, 1.0]; 4],
                nbr_offsets: vec![0, 1, 2, 3, 4],
                nbrs: vec![1, 2, 3, 0],
            },
            GeologyState {
                height: vec![0.2, -0.1, 0.1, -0.2],
                plate_id: vec![0, 0, 1, 1],
                river_flux: vec![0.0; 4],
                river_next: vec![-1; 4],
                erosion_rate: vec![0.0; 4],
                deposition_rate: vec![0.0; 4],
                boundary_condition: vec![0.0; 4],
            },
        )
    }

    #[test]
    fn world_initializes_exec_state() {
        let world = build_world();
        assert_eq!(world.exec.era, EraKind::Crust);
        assert_eq!(
            world.exec.real_years_per_tick,
            EraKind::Crust.real_years_per_tick()
        );
        assert_eq!(world.exec.budgets, EraKind::Crust.budgets());
        assert_eq!(world.exec.transition.last_land_ratio, 0.5);
        assert_eq!(world.exec.feedback_queue.pending.pollution.len(), 4);
    }

    #[test]
    fn world_initializes_land_ratio_independently_from_sea_ratio() {
        let world = World::new(
            WorldMesh {
                positions: vec![[0.0, 0.0, 1.0]; 4],
                nbr_offsets: vec![0, 1, 2, 3, 4],
                nbrs: vec![1, 2, 3, 0],
            },
            GeologyState {
                height: vec![0.3, 0.1, 0.2, -0.4],
                plate_id: vec![0, 0, 1, 1],
                river_flux: vec![0.0; 4],
                river_next: vec![-1; 4],
                erosion_rate: vec![0.0; 4],
                deposition_rate: vec![0.0; 4],
                boundary_condition: vec![0.0; 4],
            },
        );

        assert_eq!(world.exec.target_sea_ratio, 0.25);
        assert_eq!(world.exec.transition.last_land_ratio, 0.75);
    }

    #[test]
    fn feedback_queue_sizes_match_world() {
        let queue = FeedbackQueue::new(8);
        assert_eq!(queue.active.water_withdrawal.len(), 8);
        assert_eq!(queue.pending.dam_pressure.len(), 8);
    }

    #[test]
    fn metrics_collects_height_and_flux_stats() {
        let world = World::new(
            WorldMesh {
                positions: vec![[0.0, 0.0, 1.0]; 4],
                nbr_offsets: vec![0, 3, 5, 7, 8],
                nbrs: vec![1, 2, 3, 0, 2, 0, 1, 0],
            },
            GeologyState {
                height: vec![1.0, -1.0, 2.0, -2.0],
                plate_id: vec![0, 0, 1, 1],
                river_flux: vec![0.5, 1.2, 3.0, 0.1],
                river_next: vec![1, 2, -1, 0],
                erosion_rate: vec![0.0; 4],
                deposition_rate: vec![0.0; 4],
                boundary_condition: vec![0.0; 4],
            },
        );

        let metrics = world.metrics();
        assert_eq!(metrics.cell_count, 4);
        assert_eq!(metrics.land_cells, 2);
        assert!((metrics.land_ratio - 0.5).abs() < 1e-6);
        assert!((metrics.mean_height - 0.0).abs() < 1e-6);
        assert!((metrics.height_std_dev - 1.5811388).abs() < 1e-5);
        assert!((metrics.mean_river_flux - 1.2).abs() < 1e-6);
        assert!((metrics.max_river_flux - 3.0).abs() < 1e-6);
        assert!((metrics.top10_river_flux_sum - 4.8).abs() < 1e-6);
        assert_eq!(metrics.continent_count, 1);
        assert_eq!(metrics.largest_continent_cells, 2);
    }

    #[test]
    fn metrics_are_deterministic_for_fixed_seed() {
        let mut params = TerrainParams::default();
        params.level = 2;
        let seed = "metrics-regression-seed";

        let terrain_a = crate::domains::build_terrain(seed, params.clone());
        let terrain_b = crate::domains::build_terrain(seed, params);
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id_a = terrain_a
            .plate_id
            .iter()
            .map(|&v| v as u16)
            .collect::<Vec<_>>();
        let plate_id_b = terrain_b
            .plate_id
            .iter()
            .map(|&v| v as u16)
            .collect::<Vec<_>>();

        let mut world_a = World::new(
            WorldMesh {
                positions: positions.clone(),
                nbr_offsets: nbr_offsets.clone(),
                nbrs: nbrs.clone(),
            },
            GeologyState {
                height: terrain_a.height,
                plate_id: plate_id_a,
                river_flux: terrain_a.river_flux,
                river_next: terrain_a.river_next,
                erosion_rate: vec![0.0; positions.len()],
                deposition_rate: vec![0.0; positions.len()],
                boundary_condition: vec![0.0; positions.len()],
            },
        );
        let mut world_b = World::new(
            WorldMesh {
                positions,
                nbr_offsets,
                nbrs,
            },
            GeologyState {
                height: terrain_b.height,
                plate_id: plate_id_b,
                river_flux: terrain_b.river_flux,
                river_next: terrain_b.river_next,
                erosion_rate: vec![0.0; world_a.cell_count()],
                deposition_rate: vec![0.0; world_a.cell_count()],
                boundary_condition: vec![0.0; world_a.cell_count()],
            },
        );

        for _ in 0..8 {
            step_world(&mut world_a);
            step_world(&mut world_b);
        }

        let metrics_a = world_a.metrics();
        let metrics_b = world_b.metrics();

        assert_eq!(metrics_a.cell_count, metrics_b.cell_count);
        assert_eq!(metrics_a.land_cells, metrics_b.land_cells);
        assert!((metrics_a.land_ratio - metrics_b.land_ratio).abs() < 1e-6);
        assert!((metrics_a.mean_height - metrics_b.mean_height).abs() < 1e-6);
        assert!((metrics_a.height_std_dev - metrics_b.height_std_dev).abs() < 1e-6);
        assert!((metrics_a.max_river_flux - metrics_b.max_river_flux).abs() < 1e-6);
        assert!((metrics_a.top10_river_flux_sum - metrics_b.top10_river_flux_sum).abs() < 1e-6);
        assert_eq!(metrics_a.continent_count, metrics_b.continent_count);
        assert_eq!(
            metrics_a.largest_continent_cells,
            metrics_b.largest_continent_cells
        );
    }

    #[test]
    fn river_network_persists_without_early_collapse() {
        let mut params = TerrainParams::default();
        params.level = 2;
        let seed = "river-network-stability-seed";

        let terrain = crate::domains::build_terrain(seed, params.clone());
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = terrain
            .plate_id
            .iter()
            .map(|&v| v as u16)
            .collect::<Vec<_>>();

        let mut world = World::new(
            WorldMesh {
                positions: positions.clone(),
                nbr_offsets: nbr_offsets.clone(),
                nbrs: nbrs.clone(),
            },
            GeologyState {
                height: terrain.height.clone(),
                plate_id,
                river_flux: terrain.river_flux.clone(),
                river_next: terrain.river_next.clone(),
                erosion_rate: vec![0.0; positions.len()],
                deposition_rate: vec![0.0; positions.len()],
                boundary_condition: vec![0.0; positions.len()],
            },
        );

        let erosion = ErosionAutomatonState {
            positions,
            nbr_offsets,
            nbrs,
            height: terrain.height,
            water: vec![0.0; terrain.river_flux.len()],
            sediment: vec![0.0; terrain.river_flux.len()],
            armor: vec![0.0; terrain.river_flux.len()],
            rain: vec![0.12; terrain.river_flux.len()],
            river_flux: terrain.river_flux,
            river_next: terrain.river_next,
            active_queue: (0..world.cell_count() as u32).collect(),
            active_head: 0,
            in_queue: vec![1; world.cell_count()],
            rain_cursor: 0,
            tick: 0,
            last_rebuild_tick: 0,
            flux_scale_ema: 1.0,
            last_river_driver: 1.0,
            prev_river_next: world.state.geology.river_next.clone(),
            flow_heading: vec![[0.0, 0.0, 0.0]; world.cell_count()],
            groundwater_storage: vec![0.0; world.cell_count()],
            scratch_effective_runoff: vec![0.0; world.cell_count()],
            recent_changed: Vec::new(),
            sink_id: vec![-1; world.cell_count()],
            sink_route_next: vec![-1; world.cell_count()],
            sink_spill_cell: Vec::new(),
            sink_spill_to: Vec::new(),
            sink_capacity_total: Vec::new(),
            sink_capacity_remaining: Vec::new(),
            sink_storage_sediment: Vec::new(),
            sink_spill_level: Vec::new(),
            sink_overflow_active: Vec::new(),
            sink_dirty: vec![1; world.cell_count()],
            params,
        };
        let _ = world.attach_river_erosion_state(erosion);

        for _ in 0..2 {
            step_world(&mut world);
        }
        let metrics_t2 = world.metrics();

        for _ in 2..28 {
            step_world(&mut world);
        }
        let metrics_t28 = world.metrics();

        assert!(metrics_t2.river_active_cells > 0);
        assert!(metrics_t28.river_active_cells > 0);
        assert!(metrics_t2.river_ocean_reach_ratio > 0.10);
        assert!(metrics_t28.river_ocean_reach_ratio > 0.05);
        assert!(metrics_t2.river_fragmentation_ratio < 0.95);
        assert!(metrics_t28.river_fragmentation_ratio < 0.98);
    }
}
