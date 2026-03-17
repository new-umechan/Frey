use super::*;
use crate::sim::world::{GeologyState, World, WorldMesh};

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
