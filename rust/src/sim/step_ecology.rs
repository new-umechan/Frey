use super::{blend_alpha, lerp, World};

pub(super) fn run_ecology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let climate_temp = world.state.climate.temp.clone();
    let climate_rain = world.state.climate.rain.clone();
    let alpha = blend_alpha(budget, 0.16);
    let max_flux = world
        .state
        .geology
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.state.geology.height.len() {
        let temp = climate_temp[i];
        let rain = climate_rain[i];
        let land = if world.state.geology.height[i] > 0.0 {
            1.0
        } else {
            0.15
        };
        let river_bonus = (world.state.geology.river_flux[i] / max_flux).clamp(0.0, 1.0) * 0.20;
        let pollution = world.exec.feedback_queue.active.pollution[i].clamp(0.0, 1.0);
        let temp_suit = 1.0 - ((temp - 0.55).abs() / 0.55).clamp(0.0, 1.0);
        let rain_suit = 1.0 - ((rain - 0.60).abs() / 0.60).clamp(0.0, 1.0);
        let target_vegetation = ((rain * 0.60 + river_bonus * 0.35 + temp_suit * 0.20)
            * (1.0 - pollution * 0.50))
            .clamp(0.0, 1.0);
        let target_habitability = (((temp_suit * 0.55 + rain_suit * 0.45) * land + river_bonus)
            * (1.0 - pollution * 0.35))
            .clamp(0.0, 1.0);
        let target_productivity = (target_habitability
            * (0.45 + rain * 0.40 + river_bonus + target_vegetation * 0.25)
            * (1.0 - pollution * 0.25))
            .clamp(0.0, 1.0);

        world.state.ecology.vegetation[i] =
            lerp(world.state.ecology.vegetation[i], target_vegetation, alpha);
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
