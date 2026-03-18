pub mod types;

#[allow(unused_imports)]
pub use crate::sim::polity::types::*;

use crate::sim::world::World;

pub(crate) fn update_polity(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let n = world.state.geology.height.len();
    for i in 0..n {
        let pop = world.state.population.population[i];
        world.state.polity.polity_id[i] = if pop >= 10.0 { (i + 1) as u32 } else { 0 };
        world.state.polity.territory_status[i] = if world.state.polity.polity_id[i] == 0 {
            0
        } else {
            1
        };
        world.state.polity.language_group[i] = if world.state.polity.polity_id[i] == 0 {
            0
        } else {
            (i % 8) as u16 + 1
        };
        world.state.polity.polity_stability[i] =
            (1.0 - world.state.population.migration_pressure[i]).clamp(0.0, 1.0);
    }
}
