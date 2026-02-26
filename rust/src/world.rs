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
