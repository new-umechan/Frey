use serde::{Deserialize, Serialize};

use crate::sim::erosion::ErosionAutomatonState;

use super::era::EraKind;
use super::init::default_target_sea_ratio;
use super::state::GeologyDynamicsState;

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
    pub geology_dynamics: Option<GeologyDynamicsState>,
    #[serde(default)]
    pub hydrology_dynamics: Option<ErosionAutomatonState>,
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
    #[serde(default)]
    pub extra: Vec<FeedbackChannel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackChannel {
    pub key: String,
    pub values: Vec<f32>,
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
    pub fn new(cell_count: usize) -> Self {
        Self {
            active: FeedbackFields::zeros(cell_count),
            pending: FeedbackFields::zeros(cell_count),
        }
    }
}

impl FeedbackFields {
    pub const WATER_WITHDRAWAL_KEY: &'static str = "water_withdrawal";
    pub const DAM_PRESSURE_KEY: &'static str = "dam_pressure";
    pub const POLLUTION_KEY: &'static str = "pollution";

    pub fn zeros(cell_count: usize) -> Self {
        Self {
            water_withdrawal: vec![0.0; cell_count],
            dam_pressure: vec![0.0; cell_count],
            pollution: vec![0.0; cell_count],
            extra: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.water_withdrawal.fill(0.0);
        self.dam_pressure.fill(0.0);
        self.pollution.fill(0.0);
        for channel in &mut self.extra {
            channel.values.fill(0.0);
        }
    }

    pub fn channel(&self, key: &str) -> Option<&[f32]> {
        match key {
            Self::WATER_WITHDRAWAL_KEY => Some(self.water_withdrawal.as_slice()),
            Self::DAM_PRESSURE_KEY => Some(self.dam_pressure.as_slice()),
            Self::POLLUTION_KEY => Some(self.pollution.as_slice()),
            _ => self
                .extra
                .iter()
                .find(|channel| channel.key == key)
                .map(|channel| channel.values.as_slice()),
        }
    }

    pub fn channel_mut(&mut self, key: &str, cell_count: usize) -> &mut [f32] {
        let values = match key {
            Self::WATER_WITHDRAWAL_KEY => &mut self.water_withdrawal,
            Self::DAM_PRESSURE_KEY => &mut self.dam_pressure,
            Self::POLLUTION_KEY => &mut self.pollution,
            _ => {
                let channel_index = self
                    .extra
                    .iter()
                    .position(|channel| channel.key == key)
                    .unwrap_or_else(|| {
                        self.extra.push(FeedbackChannel {
                            key: key.to_string(),
                            values: vec![0.0; cell_count],
                        });
                        self.extra.len() - 1
                    });
                &mut self.extra[channel_index].values
            }
        };
        if values.len() != cell_count {
            values.resize(cell_count, 0.0);
        }
        values.as_mut_slice()
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
