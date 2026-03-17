use super::{blend_alpha, lerp, World};

pub(super) fn run_civilization_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let eco_habitability = world.state.ecology.habitability.clone();
    let eco_productivity = world.state.ecology.productivity.clone();
    let alpha = blend_alpha(budget, 0.12);
    let max_flux = world
        .state
        .geology
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.state.geology.height.len() {
        if world.state.geology.height[i] <= 0.0 {
            world.state.civilization.population[i] *= 0.98;
            world.state.civilization.state_id[i] = 0;
            world.state.civilization.agriculture[i] = 0.0;
            world.exec.feedback_queue.pending.water_withdrawal[i] = 0.0;
            world.exec.feedback_queue.pending.dam_pressure[i] = 0.0;
            world.exec.feedback_queue.pending.pollution[i] = 0.0;
            continue;
        }

        let river_support = (world.state.geology.river_flux[i] / max_flux).clamp(0.0, 1.0);
        let carrying =
            1.0 + eco_productivity[i] * 130.0 + eco_habitability[i] * 70.0 + river_support * 40.0;
        let current = world.state.civilization.population[i].max(0.0);
        let seeded = if current < 1.0 && eco_habitability[i] > 0.55 {
            1.0
        } else {
            current
        };
        let growth = 0.18_f32
            * eco_habitability[i].max(0.05_f32)
            * seeded
            * (1.0_f32 - seeded / carrying).max(-0.5_f32);
        let next_population = (seeded + growth * alpha * 4.0).max(0.0);
        let agriculture = (eco_productivity[i] * 0.65 + river_support * 0.35).clamp(0.0, 1.0);
        let withdrawal = (agriculture * next_population / 180.0).clamp(0.0, 1.0);
        let dam_pressure = (river_support * next_population / 220.0).clamp(0.0, 1.0);
        let pollution = (next_population / 260.0).clamp(0.0, 1.0);

        world.state.civilization.population[i] = next_population;
        world.state.civilization.state_id[i] = if next_population >= 10.0 {
            (i + 1) as u32
        } else {
            0
        };
        world.state.civilization.agriculture[i] =
            lerp(world.state.civilization.agriculture[i], agriculture, alpha);
        world.exec.feedback_queue.pending.water_withdrawal[i] = withdrawal;
        world.exec.feedback_queue.pending.dam_pressure[i] = dam_pressure;
        world.exec.feedback_queue.pending.pollution[i] = pollution;
    }
}
