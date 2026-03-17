use super::*;
use crate::sim::world::{FeedbackFields, GeologyState, World, WorldMesh};

fn build_test_world() -> World {
    let mesh = WorldMesh {
        positions: vec![
            normalize3([0.0, 0.8, 0.6]),
            normalize3([0.7, 0.2, 0.6]),
            normalize3([0.4, -0.7, 0.6]),
            normalize3([-0.6, -0.1, 0.8]),
        ],
        nbr_offsets: vec![0, 3, 6, 9, 12],
        nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
    };
    let geology = GeologyState {
        height: vec![0.45, 0.15, -0.25, 0.05],
        plate_id: vec![0, 0, 1, 1],
        erosion_rate: vec![0.0; 4],
        deposition_rate: vec![0.0; 4],
        boundary_condition: vec![0.0; 4],
    };
    World::new(mesh, geology)
}

#[test]
fn exec_world_advances_tick_and_sets_budget_to_one() {
    let mut world = build_test_world();
    world.exec.era = EraKind::History;
    exec_world(&mut world);
    assert_eq!(world.exec.tick, 1);
    assert_eq!(world.exec.budgets.geology, 1);
    assert_eq!(world.exec.budgets.climate, 1);
    assert_eq!(world.exec.budgets.ecology, 1);
    assert_eq!(world.exec.budgets.civilization, 4);
}

#[test]
fn feedback_queue_applies_pending_to_active_on_next_tick() {
    let mut world = build_test_world();
    world.exec.era = EraKind::History;

    world
        .exec
        .feedback_queue
        .pending
        .channel_mut(FeedbackFields::WATER_WITHDRAWAL_KEY, world.cell_count())[0] = 0.42;
    world
        .exec
        .feedback_queue
        .pending
        .channel_mut(FeedbackFields::DAM_PRESSURE_KEY, world.cell_count())[0] = 0.31;
    world
        .exec
        .feedback_queue
        .pending
        .channel_mut(FeedbackFields::POLLUTION_KEY, world.cell_count())[2] = 0.77;

    exec_world(&mut world);

    assert!((world.state.subsistence.water_withdrawal[0] - 0.42).abs() < 1e-6);
    assert!((world.state.subsistence.dam_pressure[0] - 0.31).abs() < 1e-6);
    assert!((world.state.subsistence.pollution[2] - 0.77).abs() < 1e-6);
    assert_eq!(
        world
            .exec
            .feedback_queue
            .active
            .channel(FeedbackFields::POLLUTION_KEY)
            .and_then(|values| values.get(2).copied()),
        Some(0.77)
    );
    assert_eq!(
        world
            .exec
            .feedback_queue
            .pending
            .channel(FeedbackFields::POLLUTION_KEY)
            .and_then(|values| values.get(2).copied()),
        Some(0.0)
    );
}
