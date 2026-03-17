pub mod types;

#[allow(unused_imports)]
pub use crate::sim::population::types::*;

use crate::sim::world::World;

pub(crate) fn update_population(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let n = world.state.geology.height.len();
    for i in 0..n {
        if world.state.geology.height[i] <= 0.0 {
            world.state.population.population[i] *= 0.98;
            world.state.population.population_density[i] = 0.0;
            world.state.population.migration_pressure[i] = 0.0;
            continue;
        }
        let current = world.state.population.population[i].max(0.0);
        let hab = world.state.ecology.habitability[i].clamp(0.0, 1.0);
        let food = world.state.subsistence.food_production[i].clamp(0.0, 1.0);
        let carrying = 1.0 + food * 130.0 + hab * 70.0;
        let seeded = if current < 1.0 && hab > 0.55 { 1.0 } else { current };
        let growth = 0.18_f32 * hab.max(0.05_f32) * seeded * (1.0_f32 - seeded / carrying).max(-0.5);
        let next = (seeded + growth * (budget as f32).max(1.0) * 0.5).max(0.0);
        world.state.population.population[i] = next;
        world.state.population.population_density[i] = next;
        world.state.population.migration_pressure[i] = (next / carrying).clamp(0.0, 1.0);
        world.exec.feedback_queue.pending.pollution[i] = (next / 260.0).clamp(0.0, 1.0);
    }
}
