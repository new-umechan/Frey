use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::geo::{
    add3, dot3, east_direction, edge_distance_km, normalize3, project_to_tangent, sub3,
    EARTH_RADIUS_KM,
};
use crate::sim::hydrology::rebuild_mfd_from_primary;
use smallvec::SmallVec;

use super::era::EraKind;
use super::exec::{ClockState, FeedbackQueue, RuntimeState, TransitionState};
use super::state::{
    Biome, ClimateState, CoastSide, ConflictState, DomesticatesInternal, DomesticatesState,
    EcologyInternal, EcologyState, EntitiesState, GeoState, GeologyState, HydrologyState,
    PolityState, PopulationState, SettlementState, SubsistenceMix, SubsistenceState, World,
    WorldMesh, WorldState, N_CROPS, N_LIVESTOCK,
};

impl World {
    pub fn new(mesh: WorldMesh, geology: GeologyState) -> Self {
        let cell_count = geology.height.len();
        let (land_ratio, target_sea_ratio) = land_and_sea_ratios(&geology.height);
        let geo = build_geo_state(&mesh, &geology.height);
        let ocean_temperature = geo
            .latitude
            .iter()
            .map(|&latitude| base_ocean_temperature(latitude))
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
                    precipitable_water: vec![0.0; cell_count],
                    wind_u: vec![0.0; cell_count],
                    wind_v: vec![0.0; cell_count],
                    moisture_flux_u: vec![0.0; cell_count],
                    moisture_flux_v: vec![0.0; cell_count],
                },
                hydrology: HydrologyState {
                    river_downstream: vec![SmallVec::new(); cell_count],
                    river_next: vec![-1; cell_count],
                    river_flow: vec![0.0; cell_count],
                    river_transport_cost: vec![1.0; cell_count],
                    is_lake: vec![false; cell_count],
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
                    food_production: vec![0.0; cell_count],
                    freshwater_access: vec![0.0; cell_count],
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
            entities: EntitiesState::default(),
            clock: ClockState {
                tick: 0,
                epoch: era,
                real_years_per_tick: era.real_years_per_tick(),
                runtime_tick_ms: era.runtime_tick_ms(),
                budgets: era.budgets(),
            },
            feedback: FeedbackQueue::new(cell_count),
            runtime: RuntimeState {
                target_sea_ratio,
                transition: TransitionState {
                    era_enter_tick: 0,
                    stable_ticks_in_era: 0,
                    last_land_ratio: land_ratio,
                    ema_geology_activity: 1.0,
                    ema_climate_activity: 1.0,
                    ema_ecology_activity: 1.0,
                    ema_civilization_activity: 1.0,
                },
                geology_dynamics: None,
                hydrology_dynamics: None,
            },
            polity_relations: std::collections::HashMap::new(),
            polity_groups: Vec::new(),
            archive: super::state::ArchiveState::default(),
        }
    }

    pub fn cell_count(&self) -> usize {
        self.state.geology.height.len()
    }

    pub fn attach_hydrology_dynamics(
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
        self.state
            .hydrology
            .river_flow
            .clone_from(&state.river_flux);
        self.state
            .hydrology
            .river_next
            .clone_from(&state.river_next);
        rebuild_mfd_from_primary(&mut self.state.hydrology);
        self.runtime.hydrology_dynamics = Some(state);
        Ok(())
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

pub fn default_target_sea_ratio() -> f32 {
    0.62
}

fn build_geo_state(mesh: &WorldMesh, height: &[f32]) -> GeoState {
    let cell_count = height.len();
    let mut latitude = vec![0.0; cell_count];
    let mut is_coastal = vec![false; cell_count];

    for i in 0..cell_count {
        let pos = mesh.positions.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
        latitude[i] = pos[1].clamp(-1.0, 1.0).asin().to_degrees();
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

    let distance_from_ocean = build_distance_from_ocean(mesh, height);
    let mut coast_side = vec![CoastSide::None; cell_count];
    for i in 0..cell_count {
        if !is_coastal[i] {
            continue;
        }
        coast_side[i] = classify_coast_side(mesh, height, i);
    }

    GeoState {
        latitude,
        distance_from_ocean,
        coast_side,
        is_coastal,
        neighbors_offsets: mesh.nbr_offsets.clone(),
        neighbors: mesh.nbrs.clone(),
    }
}

fn build_distance_from_ocean(mesh: &WorldMesh, height: &[f32]) -> Vec<f32> {
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
