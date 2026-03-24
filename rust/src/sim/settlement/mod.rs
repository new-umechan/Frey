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
        world.state.settlement.settlement_population[i] =
            lerp(world.state.settlement.settlement_population[i], next_size, alpha);
        world.state.settlement.urbanization[i] =
            lerp(world.state.settlement.urbanization[i], urban, alpha);
        world.state.settlement.centrality[i] = lerp(
            world.state.settlement.centrality[i],
            (urban
                + world.state.hydrology.river_transport_cost[i]
                    .recip()
                    .clamp(0.0, 1.0))
                * 0.5,
            alpha,
        );
        if world.state.settlement.settlement_population[i] > 0.5 {
            settlements.push(SettlementComponent {
                settlement_id: i as u32 + 1,
                cell: i as u32,
                size: world.state.settlement.settlement_population[i],
                urbanization: world.state.settlement.urbanization[i],
            });
        }
    }
    world.entities.replace_settlements(settlements);
}
