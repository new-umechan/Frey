use crate::sim::world::{EraKind, World};

pub(super) fn update_era_transition(world: &mut World) {
    let next_tick = world.clock.tick.saturating_add(1);
    let next_era = era_for_tick(next_tick);
    if next_era != world.clock.epoch {
        let land_ratio = current_land_ratio(world);
        world.clock.epoch = next_era;
        world.clock.budgets = next_era.budgets();
        world.clock.real_years_per_tick = next_era.real_years_per_tick();
        world.clock.runtime_tick_ms = next_era.runtime_tick_ms();
        world
            .runtime
            .transition
            .reset_for_era(next_tick, next_era, land_ratio);
    }
}

const ERA_TRANSITIONS: &[(u64, EraKind)] = &[
    (0, EraKind::Crust),
    (800, EraKind::Environment),
    (1_300, EraKind::Life),
    (1_395, EraKind::Civilization),
    (1_445, EraKind::History),
];

fn era_for_tick(tick: u64) -> EraKind {
    let mut era = EraKind::Crust;
    for (start_tick, candidate) in ERA_TRANSITIONS.iter().copied() {
        if tick >= start_tick {
            era = candidate;
        } else {
            break;
        }
    }
    era
}

fn current_land_ratio(world: &World) -> f32 {
    let cell_count = world.state.geology.height.len().max(1) as f32;
    ratio_of(
        &world.state.geology.height,
        |value| *value > 0.0,
        cell_count,
    )
}

fn ratio_of(values: &[f32], mut predicate: impl FnMut(&f32) -> bool, denominator: f32) -> f32 {
    values.iter().filter(|value| predicate(value)).count() as f32 / denominator
}
