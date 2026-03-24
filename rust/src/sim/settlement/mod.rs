pub mod types;

#[allow(unused_imports)]
pub use crate::sim::settlement::types::*;

use crate::sim::exec::lerp;
use crate::sim::world::{SettlementComponent, World};

pub(crate) fn update_settlement(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let alpha = 0.18_f32;
    let n = world.state.geology.height.len();
    let mut settlements = Vec::new();
    for i in 0..n {
        let pop = world.state.population.population[i];
        let next_size = if world.state.geology.height[i] > 0.0 {
            pop
        } else {
            0.0
        };
        let urban = (next_size / 60.0).clamp(0.0, 1.0);
        world.state.settlement.urbanization[i] =
            lerp(world.state.settlement.urbanization[i], urban, alpha);
        if next_size > 0.5 {
            settlements.push(SettlementComponent {
                settlement_id: i as u32 + 1,
                cell: i as u32,
            });
        }
    }
    world.entities.replace_settlements(settlements);
}
