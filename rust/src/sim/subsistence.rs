use crate::sim::exec::lerp;
use crate::sim::world::World;

pub(crate) fn update_subsistence(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let alpha = 0.2_f32;
    let n = world.state.geology.height.len();
    let max_flow = world
        .state
        .hydrology
        .river_flow
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    for i in 0..n {
        if world.state.geology.height[i] <= 0.0 {
            world.state.subsistence.subsistence_mix[i] = 0.0;
            world.state.subsistence.food_production[i] = 0.0;
            world.state.subsistence.land_use[i] = 0.0;
            continue;
        }
        let eco = world.state.ecology.productivity[i].clamp(0.0, 1.0);
        let river = (world.state.hydrology.river_flow[i] / max_flow).clamp(0.0, 1.0);
        let crop = world.state.domesticates.crop_available[i] as f32;
        let food = (eco * 0.65 + river * 0.25 + crop * 0.10).clamp(0.0, 1.0);
        let land_use = (food * 0.8).clamp(0.0, 1.0);
        world.state.subsistence.food_production[i] =
            lerp(world.state.subsistence.food_production[i], food, alpha * budget.max(1) as f32);
        world.state.subsistence.land_use[i] =
            lerp(world.state.subsistence.land_use[i], land_use, alpha * budget.max(1) as f32);
        world.state.subsistence.subsistence_mix[i] =
            lerp(world.state.subsistence.subsistence_mix[i], food, alpha * 0.5);
        world.exec.feedback_queue.pending.water_withdrawal[i] =
            (food * world.state.population.population[i] / 180.0).clamp(0.0, 1.0);
        world.exec.feedback_queue.pending.dam_pressure[i] =
            (river * world.state.population.population[i] / 220.0).clamp(0.0, 1.0);
    }
}
