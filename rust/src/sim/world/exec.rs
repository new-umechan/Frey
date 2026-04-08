use serde::{Deserialize, Serialize};

use super::era::EraKind;
use super::state::{
    CellId, GeologyDynamicsState, PolityComponent, PolityId, RegionComponent, RegionId,
    SettlementComponent, SettlementId,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockState {
    pub tick: u64,
    pub epoch: EraKind,
    pub real_years_per_tick: f32,
    pub runtime_tick_ms: u32,
    pub budgets: SubsystemBudgets,
    pub transition: TransitionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExecScratchState {
    #[serde(default)]
    pub geology_dynamics: Option<GeologyDynamicsState>,
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
    pub entries: Vec<FeedbackEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ModuleId {
    Exec,
    Geology,
    Climate,
    Hydrology,
    Ecology,
    Domesticates,
    Subsistence,
    Population,
    Settlement,
    Polity,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellFieldId {
    CropAdoption(u8),
    LivestockAdoption(u8),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    F32(f32),
    U32(u32),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntityBundle {
    Polity(PolityComponent),
    Settlement(SettlementComponent),
    Region(RegionComponent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentPatch {
    Polity {
        capital_cell: Option<CellId>,
        legitimacy: Option<f32>,
        centralization: Option<f32>,
        military_tech: Option<f32>,
        cells_cache: Option<Vec<CellId>>,
    },
    Settlement {
        cell: Option<CellId>,
    },
    Region {
        cells: Option<Vec<CellId>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TargetRef {
    Cell(CellId),
    Polity(PolityId),
    Settlement(SettlementId),
    Region(RegionId),
    Global,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntityRef {
    Polity(PolityId),
    Settlement(SettlementId),
    Region(RegionId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeedbackPayload {
    DeltaF32 {
        field: CellFieldId,
        cell: CellId,
        delta: f32,
    },
    SetValue {
        field: CellFieldId,
        cell: CellId,
        value: FieldValue,
    },
    SpawnEntity {
        bundle: EntityBundle,
    },
    DestroyEntity {
        entity: EntityRef,
    },
    MutateEntity {
        entity: EntityRef,
        patch: ComponentPatch,
    },
    TriggerEpochTransition {
        to: EraKind,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub source: ModuleId,
    pub target_module: ModuleId,
    pub target_ref: TargetRef,
    pub enqueued_tick: u64,
    pub payload: FeedbackPayload,
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

impl FeedbackQueue {
    pub fn new(_cell_count: usize) -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: FeedbackEntry) {
        self.entries.push(entry);
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
