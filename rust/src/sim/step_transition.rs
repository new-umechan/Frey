use super::{EraKind, World};

pub(super) fn update_era_transition(world: &mut World) {
    let cell_count = world.state.geology.height.len().max(1) as f32;
    let land_ratio = world
        .state
        .geology
        .height
        .iter()
        .filter(|&&h| h > 0.0)
        .count() as f32
        / cell_count;
    let river_network = world
        .state
        .geology
        .river_flux
        .iter()
        .filter(|&&flux| flux > 0.8)
        .count() as f32
        / cell_count;
    let habitable_ratio = world
        .state
        .ecology
        .habitability
        .iter()
        .filter(|&&v| v > 0.45)
        .count() as f32
        / cell_count;
    let settled_cells = world
        .state
        .civilization
        .population
        .iter()
        .filter(|&&v| v >= 10.0)
        .count();
    let total_population = world
        .state
        .civilization
        .population
        .iter()
        .copied()
        .sum::<f32>();
    let state_cells = world
        .state
        .civilization
        .state_id
        .iter()
        .filter(|&&id| id > 0)
        .count();
    let geology_activity = world
        .exec
        .terrain_dynamics
        .as_ref()
        .map(|state| {
            state
                .cached_metrics
                .terrain_activity
                .max(state.cached_metrics.boundary_activity)
        })
        .unwrap_or(0.0);
    let climate_activity = world
        .state
        .climate
        .precipitation
        .iter()
        .copied()
        .map(|value| (value / 1_500.0).clamp(0.0, 1.0))
        .sum::<f32>()
        / cell_count;
    let ecology_activity = world
        .state
        .ecology
        .productivity
        .iter()
        .copied()
        .sum::<f32>()
        / cell_count;
    let civilization_activity = (total_population / cell_count / 40.0).clamp(0.0, 1.0);

    world.exec.transition.ema_geology_activity =
        update_ema(world.exec.transition.ema_geology_activity, geology_activity);
    world.exec.transition.ema_climate_activity =
        update_ema(world.exec.transition.ema_climate_activity, climate_activity);
    world.exec.transition.ema_ecology_activity =
        update_ema(world.exec.transition.ema_ecology_activity, ecology_activity);
    world.exec.transition.ema_civilization_activity = update_ema(
        world.exec.transition.ema_civilization_activity,
        civilization_activity,
    );

    let next_era = match world.exec.era {
        EraKind::Crust if ticks_in_era(world) >= 8 => {
            let stable_land = (land_ratio - world.exec.transition.last_land_ratio).abs() < 0.002;
            if stable_land && world.exec.transition.ema_geology_activity < 0.08 {
                world.exec.transition.stable_ticks_in_era =
                    world.exec.transition.stable_ticks_in_era.saturating_add(1);
            } else {
                world.exec.transition.stable_ticks_in_era = 0;
            }
            if world.exec.transition.stable_ticks_in_era >= 6 {
                Some(EraKind::Environment)
            } else {
                None
            }
        }
        EraKind::Environment if ticks_in_era(world) >= 24 => {
            if river_network > 0.06 && world.exec.transition.ema_climate_activity > 0.20 {
                world.exec.transition.stable_ticks_in_era =
                    world.exec.transition.stable_ticks_in_era.saturating_add(1);
            } else {
                world.exec.transition.stable_ticks_in_era = 0;
            }
            if world.exec.transition.stable_ticks_in_era >= 8 {
                Some(EraKind::Life)
            } else {
                None
            }
        }
        EraKind::Life if ticks_in_era(world) >= 24 => {
            if habitable_ratio > 0.18 {
                world.exec.transition.stable_ticks_in_era =
                    world.exec.transition.stable_ticks_in_era.saturating_add(1);
            } else {
                world.exec.transition.stable_ticks_in_era = 0;
            }
            if world.exec.transition.stable_ticks_in_era >= 10 {
                Some(EraKind::Civilization)
            } else {
                None
            }
        }
        EraKind::Civilization if ticks_in_era(world) >= 32 => {
            let signal_count = usize::from(settled_cells > 0)
                + usize::from(total_population > 50.0)
                + usize::from(state_cells > 0);
            if signal_count >= 2 {
                world.exec.transition.stable_ticks_in_era =
                    world.exec.transition.stable_ticks_in_era.saturating_add(1);
            } else {
                world.exec.transition.stable_ticks_in_era = 0;
            }
            if world.exec.transition.stable_ticks_in_era >= 12 {
                Some(EraKind::History)
            } else {
                None
            }
        }
        _ => None,
    };

    world.exec.transition.last_land_ratio = land_ratio;
    if let Some(next_era) = next_era {
        world.exec.era = next_era;
        world.exec.budgets = next_era.budgets();
        world.exec.real_years_per_tick = next_era.real_years_per_tick();
        world.exec.runtime_tick_ms = next_era.runtime_tick_ms();
        world.exec.transition.reset_for_era(
            world.exec.tick.saturating_add(1),
            next_era,
            land_ratio,
        );
    }
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
        .exec
        .tick
        .saturating_sub(world.exec.transition.era_enter_tick)
}
