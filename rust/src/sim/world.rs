use serde::{Deserialize, Serialize};

use super::erosion::ErosionAutomatonState;

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
    pub geology: GeologyState,
    pub climate: ClimateState,
    pub ecology: EcologyState,
    pub civilization: CivilizationState,
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
    pub temp: Vec<f32>,
    pub rain: Vec<f32>,
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

impl World {
    pub fn new(mesh: WorldMesh, geology: GeologyState) -> Self {
        let cell_count = geology.height.len();
        let (land_ratio, target_sea_ratio) = land_and_sea_ratios(&geology.height);
        let era = EraKind::Crust;
        Self {
            mesh,
            state: WorldState {
                geology,
                climate: ClimateState {
                    temp: vec![0.5; cell_count],
                    rain: vec![0.5; cell_count],
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

#[cfg(test)]
mod tests {
    use super::{EraKind, FeedbackQueue, GeologyState, World, WorldMesh};

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
}
