use crate::sim::world::{EraKind, World};

pub(super) fn update_era_transition(world: &mut World) {
    let next_tick = world.clock.tick.saturating_add(1);
    let next_era = era_for_tick(next_tick);
    if next_era != world.clock.epoch {
        let land_ratio = current_land_ratio(world);
        if world.clock.epoch == EraKind::Crust && next_era == EraKind::Environment {
            rebaseline_ocean_inventory(world);
        }
        world.clock.epoch = next_era;
        world.clock.budgets = next_era.budgets();
        world.clock.real_years_per_tick = next_era.real_years_per_tick();
        world.clock.runtime_tick_ms = next_era.runtime_tick_ms();
        world
            .clock
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
        |value| *value > world.control.sea_level_offset,
        cell_count,
    )
}

fn rebaseline_ocean_inventory(world: &mut World) {
    let sea_level = world.control.sea_level_offset;
    let inventory = world
        .state
        .geology
        .height
        .iter()
        .copied()
        .map(|h| (sea_level - h).max(0.0))
        .sum::<f32>();
    world.control.ocean_water_inventory = inventory;
    world.control.ocean_water_inventory_baseline = inventory;
    world.control.ice_inventory = 0.0;
}

fn ratio_of(values: &[f32], mut predicate: impl FnMut(&f32) -> bool, denominator: f32) -> f32 {
    values.iter().filter(|value| predicate(value)).count() as f32 / denominator
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::{GeologyState, World, WorldMesh};
    use crate::PlateId;

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                [0.0, 0.8, 0.6],
                [0.7, 0.2, 0.6],
                [0.4, -0.7, 0.6],
                [-0.6, -0.1, 0.8],
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let geology = GeologyState {
            height: vec![0.30, 0.05, -0.20, -0.40],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
            smoothing_limited_cells_ratio: 0.0,
            mean_smoothing_factor: 1.0,
            zero_mean_adjusted_cells_ratio: 0.0,
            zero_mean_mean_abs_correction: 0.0,
            zero_mean_std_delta: 0.0,
        };
        World::new(mesh, geology)
    }

    #[test]
    fn crust_to_environment_rebaselines_ocean_inventory() {
        let mut world = build_test_world();
        world.clock.tick = 799;
        world.control.sea_level_offset = 0.10;
        world.control.ocean_water_inventory = 999.0;
        world.control.ocean_water_inventory_baseline = 999.0;
        world.control.ice_inventory = 42.0;

        update_era_transition(&mut world);

        assert_eq!(world.clock.epoch, EraKind::Environment);
        let expected_inventory = world
            .state
            .geology
            .height
            .iter()
            .copied()
            .map(|h| (world.control.sea_level_offset - h).max(0.0))
            .sum::<f32>();
        assert!((world.control.ocean_water_inventory - expected_inventory).abs() < 1e-6);
        assert!((world.control.ocean_water_inventory_baseline - expected_inventory).abs() < 1e-6);
        assert!(world.control.ice_inventory.abs() < 1e-6);
    }
}
