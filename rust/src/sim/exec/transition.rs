use crate::sim::world::{EraKind, World};

pub(super) fn update_era_transition(world: &mut World) {
    let inputs = collect_transition_inputs(world);

    world.runtime.transition.ema_geology_activity = update_ema(
        world.runtime.transition.ema_geology_activity,
        inputs.geology_activity,
    );
    world.runtime.transition.ema_climate_activity = update_ema(
        world.runtime.transition.ema_climate_activity,
        inputs.climate_activity,
    );
    world.runtime.transition.ema_ecology_activity = update_ema(
        world.runtime.transition.ema_ecology_activity,
        inputs.ecology_activity,
    );
    world.runtime.transition.ema_civilization_activity = update_ema(
        world.runtime.transition.ema_civilization_activity,
        inputs.civilization_activity,
    );

    let next_era = match world.clock.epoch {
        EraKind::Crust if ticks_in_era(world) >= 8 => {
            let stable_land =
                (inputs.land_ratio - world.runtime.transition.last_land_ratio).abs() < 0.002;
            if stable_land && world.runtime.transition.ema_geology_activity < 0.08 {
                world.runtime.transition.stable_ticks_in_era = world
                    .runtime
                    .transition
                    .stable_ticks_in_era
                    .saturating_add(1);
            } else {
                world.runtime.transition.stable_ticks_in_era = 0;
            }
            if world.runtime.transition.stable_ticks_in_era >= 6 {
                Some(EraKind::Environment)
            } else {
                None
            }
        }
        EraKind::Environment if ticks_in_era(world) >= 24 => {
            if inputs.river_network > 0.06 && world.runtime.transition.ema_climate_activity > 0.20 {
                world.runtime.transition.stable_ticks_in_era = world
                    .runtime
                    .transition
                    .stable_ticks_in_era
                    .saturating_add(1);
            } else {
                world.runtime.transition.stable_ticks_in_era = 0;
            }
            if world.runtime.transition.stable_ticks_in_era >= 8 {
                Some(EraKind::Life)
            } else {
                None
            }
        }
        EraKind::Life if ticks_in_era(world) >= 24 => {
            if inputs.habitable_ratio > 0.18 {
                world.runtime.transition.stable_ticks_in_era = world
                    .runtime
                    .transition
                    .stable_ticks_in_era
                    .saturating_add(1);
            } else {
                world.runtime.transition.stable_ticks_in_era = 0;
            }
            if world.runtime.transition.stable_ticks_in_era >= 10 {
                Some(EraKind::Civilization)
            } else {
                None
            }
        }
        EraKind::Civilization if ticks_in_era(world) >= 32 => {
            let signal_count = usize::from(inputs.settled_cells > 0)
                + usize::from(inputs.total_population > 50.0)
                + usize::from(inputs.state_cells > 0);
            if signal_count >= 2 {
                world.runtime.transition.stable_ticks_in_era = world
                    .runtime
                    .transition
                    .stable_ticks_in_era
                    .saturating_add(1);
            } else {
                world.runtime.transition.stable_ticks_in_era = 0;
            }
            if world.runtime.transition.stable_ticks_in_era >= 12 {
                Some(EraKind::History)
            } else {
                None
            }
        }
        _ => None,
    };

    world.runtime.transition.last_land_ratio = inputs.land_ratio;
    if let Some(next_era) = next_era {
        world.clock.epoch = next_era;
        world.clock.budgets = next_era.budgets();
        world.clock.real_years_per_tick = next_era.real_years_per_tick();
        world.clock.runtime_tick_ms = next_era.runtime_tick_ms();
        world.runtime.transition.reset_for_era(
            world.clock.tick.saturating_add(1),
            next_era,
            inputs.land_ratio,
        );
    }
}

struct EraTransitionInputs {
    land_ratio: f32,
    river_network: f32,
    habitable_ratio: f32,
    settled_cells: usize,
    total_population: f32,
    state_cells: usize,
    geology_activity: f32,
    climate_activity: f32,
    ecology_activity: f32,
    civilization_activity: f32,
}

fn collect_transition_inputs(world: &World) -> EraTransitionInputs {
    let cell_count = world.state.geology.height.len().max(1) as f32;
    let civilization = world.state.civilization_state();
    let indicators = civilization.indicators();
    let land_ratio = ratio_of(
        &world.state.geology.height,
        |value| *value > 0.0,
        cell_count,
    );
    let river_network = ratio_of(
        &world.state.hydrology.river_flow,
        |value| *value > 0.8,
        cell_count,
    );
    let habitable_ratio = ratio_of(
        &world.state.subsistence.food_production,
        |value| *value > 0.45,
        cell_count,
    );
    let geology_activity = world
        .runtime
        .geology_dynamics
        .as_ref()
        .map(|state| {
            state
                .cached_metrics
                .geology_activity
                .max(state.cached_metrics.boundary_activity)
        })
        .unwrap_or(0.0);
    let climate_activity = world
        .state
        .climate
        .precipitation
        .iter()
        .copied()
        .map(|value| (value / 1_500.0_f32).clamp(0.0_f32, 1.0_f32))
        .sum::<f32>()
        / cell_count;
    let ecology_activity = world
        .state
        .ecology
        .tree_cover
        .iter()
        .zip(world.state.ecology.ground_cover.iter())
        .map(|(tree_cover, ground_cover)| vegetation_density_proxy(*tree_cover, *ground_cover))
        .sum::<f32>()
        / cell_count;
    let civilization_activity = (indicators.total_population / cell_count / 40.0).clamp(0.0, 1.0);

    EraTransitionInputs {
        land_ratio,
        river_network,
        habitable_ratio,
        settled_cells: indicators.settled_cells,
        total_population: indicators.total_population,
        state_cells: indicators.state_cells,
        geology_activity,
        climate_activity,
        ecology_activity,
        civilization_activity,
    }
}

fn ratio_of(values: &[f32], mut predicate: impl FnMut(&f32) -> bool, denominator: f32) -> f32 {
    values.iter().filter(|value| predicate(value)).count() as f32 / denominator
}

fn update_ema(prev: f32, sample: f32) -> f32 {
    let alpha = 0.15_f32;
    let x = if sample.is_finite() {
        sample.clamp(0.0, 1.0)
    } else {
        0.0
    };
    prev.mul_add(1.0 - alpha, alpha * x)
}

fn ticks_in_era(world: &World) -> u64 {
    world
        .clock
        .tick
        .saturating_sub(world.runtime.transition.era_enter_tick)
}

fn vegetation_density_proxy(tree_cover: f32, ground_cover: f32) -> f32 {
    let tree = tree_cover.clamp(0.0, 1.0);
    let ground = ground_cover.clamp(0.0, 1.0);
    (tree + 0.6 * ground * (1.0 - tree)).clamp(0.0, 1.0)
}
