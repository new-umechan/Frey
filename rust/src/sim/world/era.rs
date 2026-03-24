use serde::{Deserialize, Serialize};

use super::exec::SubsystemBudgets;

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
            EraKind::Environment => 1_000_000.0,
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
