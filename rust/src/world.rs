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

pub fn era_for_tick(tick: u64) -> EraKind {
    match tick {
        0..=47 => EraKind::Crust,
        48..=143 => EraKind::Environment,
        144..=319 => EraKind::Life,
        320..=639 => EraKind::Civilization,
        _ => EraKind::History,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldTime {
    pub tick: u64,
    pub era: EraKind,
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
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn step(&mut self, ticks: u32) {
        let delta = ticks.max(1) as u64;
        self.tick = self.tick.saturating_add(delta);
        self.era = era_for_tick(self.tick);
    }

    pub fn sync_era(&mut self) {
        self.era = era_for_tick(self.tick);
    }
}

#[cfg(test)]
mod tests {
    use super::{era_for_tick, EraKind, WorldTime};

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
        assert_eq!(time.era, EraKind::Environment);

        time.reset();
        assert_eq!(time.tick, 0);
        assert_eq!(time.era, EraKind::Crust);
    }
}
