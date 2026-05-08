use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::sim::geo::{
    add3, dot3, east_direction, edge_distance_km, normalize3, project_to_tangent, sub3,
    EARTH_RADIUS_KM,
};
use smallvec::SmallVec;

use super::entity_state::EntityState;
use super::era::EraKind;
use super::exec::{ClockState, ExecScratchState, TransitionState};
use super::state::{
    Biome, ClimateState, CoastSide, ConflictState, DomesticatesInternal, DomesticatesState,
    EcologyInternal, EcologyState, GeologyState, GlaciologyState, HydrologyState, PolityState,
    PopulationState, SettlementState, SubsistenceMix, SubsistenceState, TerrainState, World,
    WorldControlState, WorldMesh, WorldMetadata, WorldProjectionState, WorldRelations, WorldState,
    N_CROPS, N_LIVESTOCK,
};

impl World {
    pub fn new(mesh: WorldMesh, geology: GeologyState) -> Self {
        let cell_count = geology.height.len();
        let land_ratio = land_ratio(&geology.height);
        let terrain = build_terrain_state(&mesh, &geology.height, 0.0);
        let ocean_temperature = terrain
            .latitude
            .iter()
            .map(|&latitude| base_ocean_temperature(latitude))
            .collect::<Vec<_>>();
        let era = EraKind::Crust;
        let default_geology_params = crate::GeologyParams::default();
        let ocean_water_inventory = estimate_ocean_water_inventory(&geology.height, 0.0);
        let solid_earth_mass_proxy = estimate_solid_earth_mass_proxy(&geology.height);
        Self {
            metadata: WorldMetadata { mesh },
            state: WorldState {
                geology,
                climate: ClimateState {
                    temperature: vec![15.0; cell_count],
                    precipitation: vec![800.0; cell_count],
                    evapotranspiration: vec![400.0; cell_count],
                    runoff: vec![200.0; cell_count],
                    aridity: vec![1.0; cell_count],
                    ocean_temperature,
                    precipitable_water: vec![0.0; cell_count],
                    cloud_water: vec![0.0; cell_count],
                    wind_u: vec![0.0; cell_count],
                    wind_v: vec![0.0; cell_count],
                    moisture_flux_u: vec![0.0; cell_count],
                    moisture_flux_v: vec![0.0; cell_count],
                },
                glaciology: GlaciologyState {
                    ice_thickness: vec![0.0; cell_count],
                    ice_load: vec![0.0; cell_count],
                    accumulation: vec![0.0; cell_count],
                    ablation: vec![0.0; cell_count],
                    isostatic_adjustment: vec![0.0; cell_count],
                    applied_isostatic_adjustment: vec![0.0; cell_count],
                    glacial_erosion_rate: vec![0.0; cell_count],
                    glacial_melt_runoff: vec![0.0; cell_count],
                },
                hydrology: HydrologyState {
                    river_downstream: vec![SmallVec::new(); cell_count],
                    river_next: vec![-1; cell_count],
                    river_flow: vec![0.0; cell_count],
                    erosion_rate: vec![0.0; cell_count],
                    deposition_rate: vec![0.0; cell_count],
                    river_transport_cost: vec![1.0; cell_count],
                    surface_water_access: vec![0.0; cell_count],
                    is_lake: vec![false; cell_count],
                    sink_id: vec![-1; cell_count],
                    sink_route_next: vec![-1; cell_count],
                    sink_member_offsets: vec![0],
                    sink_member_cells: Vec::new(),
                    sink_spill_cell: Vec::new(),
                    sink_spill_to: Vec::new(),
                    sink_spill_level: Vec::new(),
                    sink_capacity_total: Vec::new(),
                    sink_capacity_remaining: Vec::new(),
                    sink_storage_water: Vec::new(),
                    sink_storage_sediment: Vec::new(),
                    sink_overflow_active: Vec::new(),
                },
                ecology: EcologyState {
                    biome: vec![Biome::TemperateForest; cell_count],
                    tree_cover: vec![0.0; cell_count],
                    ground_cover: vec![0.0; cell_count],
                    disturbance: vec![0.0; cell_count],
                    soil_fertility: vec![0.35; cell_count],
                    ecology_internal: vec![EcologyInternal::default(); cell_count],
                },
                domesticates: DomesticatesState {
                    crop_available: vec![0; cell_count],
                    crop_adoption: vec![[0.0; N_CROPS]; cell_count],
                    livestock_available: vec![0; cell_count],
                    livestock_adoption: vec![[0.0; N_LIVESTOCK]; cell_count],
                    domesticates_internal: vec![DomesticatesInternal::default(); cell_count],
                },
                subsistence: SubsistenceState {
                    subsistence_mix: vec![SubsistenceMix::default(); cell_count],
                    food_energy_mean: vec![0.0; cell_count],
                    food_energy_variance: vec![1.0; cell_count],
                    buffer_capacity: vec![0.0; cell_count],
                    mobility_capacity: vec![0.0; cell_count],
                    land_use_intensity: vec![0.0; cell_count],
                },
                population: PopulationState {
                    population: vec![0.0; cell_count],
                    birth_rate: vec![0.0; cell_count],
                    death_rate: vec![0.0; cell_count],
                },
                settlement: SettlementState {
                    urbanization: vec![0.0; cell_count],
                },
                polity: PolityState {
                    polity_id: vec![None; cell_count],
                },
                conflict: ConflictState {
                    conflict_intensity: vec![0.0; cell_count],
                    occupier_id: vec![None; cell_count],
                },
            },
            projections: WorldProjectionState { terrain },
            entities: EntityState::default(),
            clock: ClockState {
                tick: 0,
                epoch: era,
                real_years_per_tick: era.real_years_per_tick(),
                runtime_tick_ms: era.runtime_tick_ms(),
                budgets: era.budgets(),
                transition: TransitionState {
                    era_enter_tick: 0,
                    stable_ticks_in_era: 0,
                    last_land_ratio: land_ratio,
                    ema_geology_activity: 1.0,
                    ema_climate_activity: 1.0,
                    ema_ecology_activity: 1.0,
                    ema_civilization_activity: 1.0,
                },
            },
            control: WorldControlState {
                geology_params: default_geology_params.clone(),
                sea_level_offset: 0.0,
                erosion_thickness_coupling: default_geology_params.erosion_thickness_coupling,
                deposition_thickness_coupling: default_geology_params.deposition_thickness_coupling,
                ocean_water_inventory,
                ocean_water_inventory_baseline: ocean_water_inventory,
                ice_inventory: 0.0,
                marine_sediment_mass: 0.0,
                global_sediment_export: 0.0,
                solid_earth_mass_proxy,
                solid_earth_mass_proxy_baseline: solid_earth_mass_proxy,
            },
            exec_scratch: ExecScratchState {
                geology_dynamics: None,
            },
            relations: WorldRelations::default(),
        }
    }

    pub fn cell_count(&self) -> usize {
        self.state.geology.height.len()
    }

    pub fn refresh_terrain_state(&mut self) {
        self.projections.terrain = build_terrain_state(
            self.mesh(),
            &self.state.geology.height,
            self.control.sea_level_offset,
        );
    }
}

fn land_and_sea_ratios(height: &[f32]) -> (f32, f32) {
    if height.is_empty() {
        return (1.0, 0.0);
    }

    let sea_count = height.iter().filter(|&&h| h <= 0.0).count() as f32;
    let cell_count = height.len() as f32;
    let sea_ratio = sea_count / cell_count;
    (1.0 - sea_ratio, sea_ratio)
}

fn land_ratio(height: &[f32]) -> f32 {
    land_and_sea_ratios(height).0
}

fn estimate_ocean_water_inventory(height: &[f32], sea_level_offset: f32) -> f32 {
    height
        .iter()
        .copied()
        .map(|h| (sea_level_offset - h).max(0.0))
        .sum()
}

fn estimate_solid_earth_mass_proxy(height: &[f32]) -> f32 {
    height.iter().copied().sum()
}

fn build_terrain_state(mesh: &WorldMesh, height: &[f32], sea_level_offset: f32) -> TerrainState {
    let cell_count = height.len();
    let mut latitude = vec![0.0; cell_count];
    let mut is_coastal = vec![false; cell_count];

    for i in 0..cell_count {
        let pos = mesh.position(i).unwrap_or([0.0, 0.0, 1.0]);
        latitude[i] = pos[1].clamp(-1.0, 1.0).asin().to_degrees();
        let is_land = is_land_height(height[i], sea_level_offset);
        for &n_u32 in mesh.cell_neighbors(i) {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            if is_land_height(height[n], sea_level_offset) != is_land {
                is_coastal[i] = true;
                break;
            }
        }
    }

    let distance_from_ocean = build_distance_from_ocean(mesh, height, sea_level_offset);
    let mut coast_side = vec![CoastSide::None; cell_count];
    for i in 0..cell_count {
        if !is_coastal[i] {
            continue;
        }
        coast_side[i] = classify_coast_side(mesh, height, sea_level_offset, i);
    }

    TerrainState {
        latitude,
        distance_from_ocean,
        coast_side,
        is_coastal,
    }
}

fn build_distance_from_ocean(mesh: &WorldMesh, height: &[f32], sea_level_offset: f32) -> Vec<f32> {
    let cell_count = height.len();
    let mut distance = vec![f32::INFINITY; cell_count];
    let mut heap = BinaryHeap::new();

    for (i, &elevation) in height.iter().enumerate() {
        if !is_land_height(elevation, sea_level_offset) {
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
        for &n_u32 in mesh.cell_neighbors(index) {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            let edge_cost = edge_distance_km(
                mesh.position(index).unwrap_or([0.0, 0.0, 1.0]),
                mesh.position(n).unwrap_or([0.0, 0.0, 1.0]),
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

fn classify_coast_side(
    mesh: &WorldMesh,
    height: &[f32],
    sea_level_offset: f32,
    index: usize,
) -> CoastSide {
    let cell_count = height.len();
    if index >= cell_count {
        return CoastSide::None;
    }
    let pos = mesh.position(index).unwrap_or([0.0, 0.0, 1.0]);
    let is_land = is_land_height(height[index], sea_level_offset);
    let seek_land = !is_land;
    let mut dir_sum = [0.0_f32; 3];

    for &n_u32 in mesh.cell_neighbors(index) {
        let n = n_u32 as usize;
        if n >= cell_count {
            continue;
        }
        if is_land_height(height[n], sea_level_offset) != seek_land {
            continue;
        }
        let neighbor = mesh.position(n).unwrap_or([0.0, 0.0, 1.0]);
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

#[inline]
fn is_land_height(height: f32, sea_level_offset: f32) -> bool {
    height > sea_level_offset
}

fn base_ocean_temperature(latitude: f32) -> f32 {
    28.0 * latitude.to_radians().cos() - 2.0
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
