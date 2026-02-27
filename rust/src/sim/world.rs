use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub tick: u64,
    pub era: EraKind,
    pub mesh: WorldMesh,
    pub core: CoreCells,
    pub layers: HashMap<LayerKind, CellLayer>,
    pub budgets: SubsystemBudgets,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldMesh {
    pub positions: Vec<[f32; 3]>,
    pub nbr_offsets: Vec<u32>,
    pub nbrs: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreCells {
    pub height: Vec<f32>,
    pub plate_id: Vec<u16>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerKind {
    Climate,
    Ecology,
    Civilization,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CellLayer {
    Climate(ClimateLayer),
    Ecology(EcologyLayer),
    Civilization(CivilizationLayer),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClimateLayer {
    pub temp: Vec<f32>,
    pub rain: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcologyLayer {
    pub habitability: Vec<f32>,
    pub productivity: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CivilizationLayer {
    pub population: Vec<f32>,
    pub state_id: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SubsystemBudgets {
    pub terrain: u32,
    pub river: u32,
    pub climate: u32,
    pub ecology: u32,
    pub civilization: u32,
}

impl World {
    pub fn new(mesh: WorldMesh, core: CoreCells) -> Self {
        Self {
            tick: 0,
            era: EraKind::Crust,
            mesh,
            core,
            layers: HashMap::new(),
            budgets: SubsystemBudgets::default(),
        }
    }
}

pub fn step_world(world: &mut World) {
    world.budgets = compute_budgets(world.era);
    ensure_required_layers(world);
    run_terrain_step(world, world.budgets.terrain);
    run_river_step(world, world.budgets.river);
    run_climate_step(world, world.budgets.climate);
    run_ecology_step(world, world.budgets.ecology);
    run_civilization_step(world, world.budgets.civilization);
    update_era_transition(world);
    world.tick = world.tick.saturating_add(1);
}

fn compute_budgets(era: EraKind) -> SubsystemBudgets {
    match era {
        EraKind::Crust => SubsystemBudgets {
            terrain: 4,
            river: 1,
            climate: 0,
            ecology: 0,
            civilization: 0,
        },
        EraKind::Environment => SubsystemBudgets {
            terrain: 2,
            river: 4,
            climate: 3,
            ecology: 1,
            civilization: 0,
        },
        EraKind::Life => SubsystemBudgets {
            terrain: 1,
            river: 2,
            climate: 3,
            ecology: 4,
            civilization: 1,
        },
        EraKind::Civilization => SubsystemBudgets {
            terrain: 1,
            river: 1,
            climate: 2,
            ecology: 2,
            civilization: 4,
        },
        EraKind::History => SubsystemBudgets {
            terrain: 0,
            river: 1,
            climate: 1,
            ecology: 1,
            civilization: 4,
        },
    }
}

fn ensure_required_layers(world: &mut World) {
    let cell_count = world.core.height.len();
    if world.era == EraKind::Environment
        || world.era == EraKind::Life
        || world.era == EraKind::Civilization
        || world.era == EraKind::History
    {
        world.layers.entry(LayerKind::Climate).or_insert_with(|| {
            CellLayer::Climate(ClimateLayer {
                temp: vec![0.5; cell_count],
                rain: vec![0.0; cell_count],
            })
        });
    }
    if world.era == EraKind::Life || world.era == EraKind::Civilization || world.era == EraKind::History {
        world.layers.entry(LayerKind::Ecology).or_insert_with(|| {
            CellLayer::Ecology(EcologyLayer {
                habitability: vec![0.0; cell_count],
                productivity: vec![0.0; cell_count],
            })
        });
    }
    if world.era == EraKind::Civilization || world.era == EraKind::History {
        world.layers.entry(LayerKind::Civilization).or_insert_with(|| {
            CellLayer::Civilization(CivilizationLayer {
                population: vec![0.0; cell_count],
                state_id: vec![0; cell_count],
            })
        });
    }
}

fn run_terrain_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let delta = (budget as f32) * 0.0001;
    for h in &mut world.core.height {
        *h = (*h + delta).clamp(-1.0, 1.0);
    }
}

fn run_river_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let gain = (budget as f32) * 0.01;
    for flux in &mut world.core.river_flux {
        *flux = (*flux + gain).max(0.0);
    }
}

fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let Some(CellLayer::Climate(climate)) = world.layers.get_mut(&LayerKind::Climate) else {
        return;
    };
    let budget_scale = (budget as f32) * 0.01;
    for (temp, rain) in climate.temp.iter_mut().zip(climate.rain.iter_mut()) {
        *temp = (*temp + budget_scale * 0.1).clamp(0.0, 1.0);
        *rain = (*rain + budget_scale).clamp(0.0, 1.0);
    }
}

fn run_ecology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let Some(CellLayer::Ecology(ecology)) = world.layers.get_mut(&LayerKind::Ecology) else {
        return;
    };
    let budget_scale = (budget as f32) * 0.01;
    for (habitability, productivity) in ecology
        .habitability
        .iter_mut()
        .zip(ecology.productivity.iter_mut())
    {
        *habitability = (*habitability + budget_scale * 0.5).clamp(0.0, 1.0);
        *productivity = (*productivity + budget_scale * 0.3).clamp(0.0, 1.0);
    }
}

fn run_civilization_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let Some(CellLayer::Civilization(civilization)) = world.layers.get_mut(&LayerKind::Civilization) else {
        return;
    };
    let growth = (budget as f32) * 0.1;
    for population in &mut civilization.population {
        *population = (*population + growth).max(0.0);
    }
}

fn update_era_transition(world: &mut World) {
    let next_tick = world.tick.saturating_add(1);
    world.era = era_for_tick(next_tick);
}

pub fn era_for_tick(tick: u64) -> EraKind {
    match tick {
        0..=47 => EraKind::Crust,
        48..=143 => EraKind::Environment,
        144..=319 => EraKind::Life,
        320..=639 => EraKind::Civilization,
        _ => EraKind::History,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldTime {
    pub tick: u64,
    pub era: EraKind,
    pub era_enter_tick: u64,
    pub ema_terrain_activity: f32,
    pub ema_river_activity: f32,
    pub ema_climate_activity: f32,
    pub ema_ecology_activity: f32,
    pub ema_civilization_activity: f32,
    pub stable_ticks_in_era: u32,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldTime {
    pub fn new() -> Self {
        Self {
            tick: 0,
            era: era_for_tick(0),
            era_enter_tick: 0,
            ema_terrain_activity: 1.0,
            ema_river_activity: 1.0,
            ema_climate_activity: 1.0,
            ema_ecology_activity: 1.0,
            ema_civilization_activity: 1.0,
            stable_ticks_in_era: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn step(&mut self, ticks: u32) {
        let delta = ticks as u64;
        self.tick = self.tick.saturating_add(delta);
    }

    pub fn sync_era(&mut self) {
        self.set_era(era_for_tick(self.tick));
    }

    pub fn observe_activity(
        &mut self,
        terrain: f32,
        river: f32,
        climate: f32,
        ecology: f32,
        civilization: f32,
    ) {
        self.ema_terrain_activity = update_ema(self.ema_terrain_activity, terrain);
        self.ema_river_activity = update_ema(self.ema_river_activity, river);
        self.ema_climate_activity = update_ema(self.ema_climate_activity, climate);
        self.ema_ecology_activity = update_ema(self.ema_ecology_activity, ecology);
        self.ema_civilization_activity = update_ema(self.ema_civilization_activity, civilization);

        if self.is_current_era_converged() {
            self.stable_ticks_in_era = self.stable_ticks_in_era.saturating_add(1);
        } else {
            self.stable_ticks_in_era = 0;
        }

        if let Some(next_era) = self.next_era_if_ready() {
            self.set_era(next_era);
        }
    }

    fn set_era(&mut self, next_era: EraKind) {
        if self.era == next_era {
            return;
        }
        self.era = next_era;
        self.era_enter_tick = self.tick;
        self.stable_ticks_in_era = 0;
    }

    fn ticks_in_era(&self) -> u64 {
        self.tick.saturating_sub(self.era_enter_tick)
    }

    fn next_era_if_ready(&self) -> Option<EraKind> {
        if self.ticks_in_era() < min_ticks_before_transition(self.era) {
            return None;
        }
        if self.stable_ticks_in_era < stable_ticks_required(self.era) {
            return None;
        }
        match self.era {
            EraKind::Crust => Some(EraKind::Environment),
            EraKind::Environment => Some(EraKind::Life),
            EraKind::Life => Some(EraKind::Civilization),
            EraKind::Civilization => Some(EraKind::History),
            EraKind::History => None,
        }
    }

    fn is_current_era_converged(&self) -> bool {
        let threshold = convergence_threshold(self.era);
        match self.era {
            EraKind::Crust => self.ema_terrain_activity <= threshold,
            EraKind::Environment => self.ema_river_activity.max(self.ema_climate_activity) <= threshold,
            EraKind::Life => self.ema_ecology_activity.max(self.ema_climate_activity) <= threshold,
            EraKind::Civilization => self.ema_civilization_activity <= threshold,
            EraKind::History => false,
        }
    }
}

fn update_ema(prev: f32, sample: f32) -> f32 {
    let alpha = 0.15_f32;
    let x = if sample.is_finite() {
        sample.clamp(0.0, 1.0)
    } else {
        0.0
    };
    prev.mul_add(1.0 - alpha, alpha * x)
}

fn min_ticks_before_transition(era: EraKind) -> u64 {
    match era {
        EraKind::Crust => 8,
        EraKind::Environment => 24,
        EraKind::Life => 24,
        EraKind::Civilization => 32,
        EraKind::History => u64::MAX,
    }
}

fn stable_ticks_required(era: EraKind) -> u32 {
    match era {
        EraKind::Crust => 6,
        EraKind::Environment => 10,
        EraKind::Life => 12,
        EraKind::Civilization => 16,
        EraKind::History => u32::MAX,
    }
}

fn convergence_threshold(era: EraKind) -> f32 {
    match era {
        EraKind::Crust => 0.02,
        EraKind::Environment => 0.03,
        EraKind::Life => 0.02,
        EraKind::Civilization => 0.015,
        EraKind::History => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        era_for_tick, step_world, CellLayer, CoreCells, EraKind, LayerKind, World, WorldMesh, WorldTime,
    };

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            nbr_offsets: vec![0, 2, 4, 6],
            nbrs: vec![1, 2, 0, 2, 0, 1],
        };
        let core = CoreCells {
            height: vec![0.0, 0.1, -0.1],
            plate_id: vec![0, 1, 1],
            river_flux: vec![0.0, 0.2, 0.4],
            river_next: vec![1, 2, -1],
        };
        World::new(mesh, core)
    }

    #[test]
    fn era_transitions_follow_thresholds() {
        assert_eq!(era_for_tick(0), EraKind::Crust);
        assert_eq!(era_for_tick(47), EraKind::Crust);
        assert_eq!(era_for_tick(48), EraKind::Environment);
        assert_eq!(era_for_tick(143), EraKind::Environment);
        assert_eq!(era_for_tick(144), EraKind::Life);
        assert_eq!(era_for_tick(319), EraKind::Life);
        assert_eq!(era_for_tick(320), EraKind::Civilization);
        assert_eq!(era_for_tick(639), EraKind::Civilization);
        assert_eq!(era_for_tick(640), EraKind::History);
    }

    #[test]
    fn world_time_updates_tick_and_era() {
        let mut time = WorldTime::new();
        assert_eq!(time.tick, 0);
        assert_eq!(time.era, EraKind::Crust);

        time.step(48);
        assert_eq!(time.tick, 48);
        assert_eq!(time.era, EraKind::Crust);

        time.reset();
        assert_eq!(time.tick, 0);
        assert_eq!(time.era, EraKind::Crust);
    }

    #[test]
    fn convergence_can_advance_era() {
        let mut time = WorldTime::new();
        for _ in 0..64 {
            time.observe_activity(0.0, 1.0, 1.0, 1.0, 1.0);
            time.step(1);
        }
        assert_eq!(time.era, EraKind::Environment);
    }

    #[test]
    fn step_with_zero_does_not_advance_tick() {
        let mut time = WorldTime::new();
        time.step(0);
        assert_eq!(time.tick, 0);
    }

    #[test]
    fn step_world_advances_tick_and_sets_budgets() {
        let mut world = build_test_world();
        assert_eq!(world.tick, 0);
        assert_eq!(world.era, EraKind::Crust);

        step_world(&mut world);

        assert_eq!(world.tick, 1);
        assert_eq!(world.era, EraKind::Crust);
        assert_eq!(world.budgets.terrain, 4);
        assert_eq!(world.budgets.river, 1);
    }

    #[test]
    fn step_world_generates_required_layers_by_era() {
        let mut world = build_test_world();
        world.era = EraKind::Life;

        step_world(&mut world);

        assert!(matches!(
            world.layers.get(&LayerKind::Climate),
            Some(CellLayer::Climate(_))
        ));
        assert!(matches!(
            world.layers.get(&LayerKind::Ecology),
            Some(CellLayer::Ecology(_))
        ));
        assert!(world.layers.get(&LayerKind::Civilization).is_none());
    }

    #[test]
    fn step_world_transitions_era_on_tick_boundary() {
        let mut world = build_test_world();
        world.tick = 47;
        world.era = EraKind::Crust;

        step_world(&mut world);

        assert_eq!(world.tick, 48);
        assert_eq!(world.era, EraKind::Environment);
    }
}
