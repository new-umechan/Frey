pub mod types;

#[allow(unused_imports)]
pub use crate::sim::population::types::*;

use crate::sim::world::{
    CellId, FeedbackEntry, FeedbackPayload, FeedbackQueue, ModuleId, TargetRef, World,
};

pub(crate) fn update_population(
    world: &mut World,
    budget: u32,
    mut feedback: Option<&mut FeedbackQueue>,
) {
    if budget == 0 {
        return;
    }
    let n = world.state.geology.height.len();
    for i in 0..n {
        if world.state.geology.height[i] <= 0.0 {
            world.state.population.population[i] *= 0.98;
            world.state.population.birth_rate[i] = 0.0;
            world.state.population.death_rate[i] = 0.03;
            continue;
        }
        let current = world.state.population.population[i].max(0.0);
        let food = world.state.subsistence.food_production[i].clamp(0.0, 1.0);
        let water = world.state.subsistence.freshwater_access[i].clamp(0.0, 1.0);
        let hab = (food * 0.6
            + water * 0.2
            + world.state.ecology.soil_fertility[i].clamp(0.0, 1.0) * 0.2)
            .clamp(0.0, 1.0);
        let carrying = 1.0 + food * 130.0 + hab * 70.0 + water * 40.0;
        let seeded = if current < 1.0 && hab > 0.55 {
            1.0
        } else {
            current
        };
        let birth_rate = (0.012 + hab * 0.024).clamp(0.0, 0.08);
        let death_rate = (0.006 + (1.0 - hab) * 0.030).clamp(0.0, 0.09);
        let logistic = (1.0_f32 - seeded / carrying).max(-0.5);
        let growth = seeded * (birth_rate - death_rate) * logistic;
        let next = (seeded + growth * (budget as f32).max(1.0) * 0.5).max(0.0);
        world.state.population.population[i] = next;
        world.state.population.birth_rate[i] = birth_rate;
        world.state.population.death_rate[i] = death_rate;

        if let Some(queue) = feedback.as_deref_mut() {
            let pressure = ((next / 120.0).clamp(0.0, 1.0) * 0.22).clamp(0.0, 0.22);
            if pressure > 0.002 {
                queue.push(FeedbackEntry {
                    source: ModuleId::Population,
                    target_module: ModuleId::Domesticates,
                    target_ref: TargetRef::Cell(CellId(i as u32)),
                    enqueued_tick: world.clock.tick,
                    payload: FeedbackPayload::DomesticatesPopulationPressure {
                        cell: CellId(i as u32),
                        intensification_bonus: pressure,
                    },
                });
            }
        }
    }
}
