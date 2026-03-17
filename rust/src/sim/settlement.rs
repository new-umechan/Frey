use crate::sim::exec::lerp;
use crate::sim::world::World;

pub(crate) fn update_settlement(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let alpha = 0.18_f32;
    let n = world.state.geology.height.len();
    for i in 0..n {
        let pop = world.state.population.population[i];
        let next_size = if world.state.geology.height[i] > 0.0 { pop } else { 0.0 };
        let urban = (next_size / 60.0).clamp(0.0, 1.0);
        world.state.settlement.settlement_size[i] = lerp(world.state.settlement.settlement_size[i], next_size, alpha);
        world.state.settlement.urbanization[i] = lerp(world.state.settlement.urbanization[i], urban, alpha);
        world.state.settlement.centrality[i] = lerp(
            world.state.settlement.centrality[i],
            (urban + world.state.hydrology.river_transport_cost[i].recip().clamp(0.0, 1.0)) * 0.5,
            alpha,
        );
        world.state.settlement.residence[i] = world.state.settlement.settlement_size[i];
    }
}
