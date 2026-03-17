use crate::sim::exec::{blend_alpha, lerp};
use crate::sim::world::World;

pub(crate) fn run_ecology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let climate_temp = world.state.climate.temperature.clone();
    let climate_precipitation = world.state.climate.precipitation.clone();
    let alpha = blend_alpha(budget, 0.16);
    let max_flux = world
        .state
        .hydrology
        .river_flow
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.state.geology.height.len() {
        let temp = climate_temp[i];
        let precipitation = climate_precipitation[i];
        let land = if world.state.geology.height[i] > 0.0 {
            1.0
        } else {
            0.08
        };
        let river_bonus = (world.state.hydrology.river_flow[i] / max_flux).clamp(0.0, 1.0) * 0.20;
        let pollution = world.exec.feedback_queue.active.pollution[i].clamp(0.0, 1.0);
        let temp_suit = (1.0_f32 - ((temp - 18.0_f32).abs() / 30.0_f32)).clamp(0.0_f32, 1.0_f32);
        let rain_suit = (1.0_f32 - ((precipitation - 1_000.0_f32).abs() / 1_200.0_f32))
            .clamp(0.0_f32, 1.0_f32);
        let target_vegetation = ((rain_suit * 0.60 + river_bonus * 0.35 + temp_suit * 0.20)
            * (1.0 - pollution * 0.50))
            .clamp(0.0, 1.0);
        let target_habitability = (((temp_suit * 0.55 + rain_suit * 0.45) * land + river_bonus)
            * (1.0 - pollution * 0.35))
            .clamp(0.0, 1.0);
        let target_productivity = (target_habitability
            * (0.35 + rain_suit * 0.40 + river_bonus + target_vegetation * 0.25)
            * (1.0 - pollution * 0.25))
            .clamp(0.0, 1.0);

        world.state.ecology.vegetation[i] =
            lerp(world.state.ecology.vegetation[i], target_vegetation, alpha);
        world.state.ecology.riparian_vegetation[i] = lerp(
            world.state.ecology.riparian_vegetation[i],
            (target_vegetation * (0.6 + river_bonus)).clamp(0.0, 1.0),
            alpha,
        );
        world.state.ecology.habitability[i] = lerp(
            world.state.ecology.habitability[i],
            target_habitability,
            alpha,
        );
        world.state.ecology.productivity[i] = lerp(
            world.state.ecology.productivity[i],
            target_productivity,
            alpha,
        );
    }
}
