use super::{blend_alpha, route_river_flux, World};

pub(super) fn run_climate_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }
    let alpha = blend_alpha(budget, 0.10);
    let max_flux = world
        .state
        .geology
        .river_flux
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    for i in 0..world.state.geology.height.len() {
        let pos = world
            .mesh
            .positions
            .get(i)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0]);
        let latitude = pos[1].abs().clamp(0.0, 1.0);
        let altitude = world.state.geology.height[i].max(0.0);
        let base_temp = 0.15 + (1.0 - latitude) * 0.85;
        let target_temp = (base_temp - altitude * 0.35).clamp(0.0, 1.0);

        let river_norm = (world.state.geology.river_flux[i] / max_flux).clamp(0.0, 1.0);
        let orographic = (altitude * 0.50).clamp(0.0, 0.35);
        let withdrawal = world.exec.feedback_queue.active.water_withdrawal[i].clamp(0.0, 1.0);
        let dam_pressure = world.exec.feedback_queue.active.dam_pressure[i].clamp(0.0, 1.0);
        let target_rain = ((0.20 + river_norm * 0.45 + (1.0 - latitude) * 0.25 + orographic)
            * (1.0 - withdrawal * 0.08)
            + dam_pressure * 0.03)
            .clamp(0.0, 1.0);

        world.state.climate.temp[i] = super::lerp(world.state.climate.temp[i], target_temp, alpha);
        world.state.climate.rain[i] = super::lerp(world.state.climate.rain[i], target_rain, alpha);
    }

    let flux = route_river_flux(
        &world.state.geology.height,
        &world.state.geology.river_next,
        &world.state.climate.rain,
    );
    world.state.geology.river_flux = flux.clone();
    if let Some(state) = world.exec.river_erosion_state.as_mut() {
        if state.river_flux.len() == flux.len() {
            state.rain.clone_from(&world.state.climate.rain);
            state.river_flux.clone_from(&flux);
        }
    }
}
