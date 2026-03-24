use super::*;
use crate::sim::polity::PolityRelation;
use crate::sim::world::{
    CellFieldId, FeedbackEntry, FeedbackPayload, FieldValue, GeologyState, ModuleId, TargetRef,
    World, WorldMesh,
};

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
    world.clock.epoch = EraKind::History;
    exec_world(&mut world);
    assert_eq!(world.clock.tick, 1);
    assert_eq!(world.clock.budgets.geology, 1);
    assert_eq!(world.clock.budgets.climate, 1);
    assert_eq!(world.clock.budgets.ecology, 1);
    assert_eq!(world.clock.budgets.civilization, 4);
}

#[test]
fn feedback_queue_applies_entries_on_next_tick() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::Crust;
    world.clock.tick = 1;

    world.feedback.push(FeedbackEntry {
        source: ModuleId::Subsistence,
        target_module: ModuleId::Hydrology,
        target_ref: TargetRef::Cell(0),
        enqueued_tick: 0,
        payload: FeedbackPayload::SetValue {
            field: CellFieldId::CropAdoption(0),
            cell: 0,
            value: FieldValue::F32(0.42),
        },
    });
    world.feedback.push(FeedbackEntry {
        source: ModuleId::Subsistence,
        target_module: ModuleId::Hydrology,
        target_ref: TargetRef::Cell(0),
        enqueued_tick: 0,
        payload: FeedbackPayload::SetValue {
            field: CellFieldId::LivestockAdoption(0),
            cell: 0,
            value: FieldValue::F32(0.31),
        },
    });
    world.feedback.push(FeedbackEntry {
        source: ModuleId::Population,
        target_module: ModuleId::Ecology,
        target_ref: TargetRef::Cell(2),
        enqueued_tick: 0,
        payload: FeedbackPayload::DeltaF32 {
            field: CellFieldId::CropAdoption(1),
            cell: 2,
            delta: 0.77,
        },
    });

    exec_world(&mut world);

    assert!((world.state.domesticates.crop_adoption[0][0] - 0.42).abs() < 1e-6);
    assert!((world.state.domesticates.livestock_adoption[0][0] - 0.31).abs() < 1e-6);
    assert!((world.state.domesticates.crop_adoption[2][1] - 0.77).abs() < 1e-6);
}

#[test]
fn feedback_payload_can_trigger_epoch_transition() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::Crust;
    world.clock.tick = 1;
    world.feedback.push(FeedbackEntry {
        source: ModuleId::Exec,
        target_module: ModuleId::Exec,
        target_ref: TargetRef::Global,
        enqueued_tick: 0,
        payload: FeedbackPayload::TriggerEpochTransition {
            to: EraKind::History,
        },
    });

    exec_world(&mut world);
    assert_eq!(world.clock.epoch, EraKind::History);
}

#[test]
fn conflict_generates_region_components_and_updates_relations() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::History;
    world.state.population.population = vec![20.0, 18.0, 0.0, 0.0];
    world.state.population.birth_rate = vec![0.02, 0.02, 0.0, 0.0];
    world.state.population.death_rate = vec![0.01, 0.01, 0.0, 0.0];
    world.state.subsistence.food_production = vec![0.9, 0.9, 0.0, 0.0];
    world.state.ecology.soil_fertility = vec![0.9, 0.8, 0.0, 0.0];

    exec_world(&mut world);

    assert!(!world.entities.region_components.is_empty());
    assert!(
        world
            .entities
            .world
            .query::<&crate::sim::world::RegionComponent>()
            .iter()
            .count()
            >= 1
    );
    assert_eq!(
        world
            .polity_relations
            .get(&(1, 2))
            .map(|relation| relation.at_war),
        Some(true)
    );
}

#[test]
fn polity_update_overwrites_stale_ids_with_none_for_low_population_cells() {
    let mut world = build_test_world();
    world.state.population.population = vec![12.0, 4.0, 11.0, 3.0];
    world.state.population.birth_rate = vec![0.02; 4];
    world.state.population.death_rate = vec![0.01; 4];
    world.state.polity.polity_id = vec![Some(10), Some(20), Some(30), Some(40)];

    crate::sim::polity::update_polity(&mut world, 1);

    assert_eq!(
        world.state.polity.polity_id,
        vec![Some(1), None, Some(3), None]
    );
}

#[test]
fn conflict_update_treats_none_polity_as_unclaimed_and_clears_occupiers() {
    let mut world = build_test_world();
    world.state.polity.polity_id = vec![Some(1), None, Some(2), None];
    world.state.conflict.occupier_id = vec![Some(9), Some(9), Some(9), Some(9)];
    world
        .polity_relations
        .insert((1, 2), PolityRelation::default());
    world
        .polity_relations
        .insert((2, 1), PolityRelation::default());

    crate::sim::conflict::update_conflict(&mut world, 1);

    assert_eq!(
        world.state.conflict.occupier_id,
        vec![None, None, None, None]
    );
    assert_eq!(
        world.state.conflict.conflict_intensity,
        vec![1.0, 0.0, 1.0, 0.0]
    );
    assert_eq!(world.entities.region_components.len(), 3);
    assert_eq!(
        world
            .polity_relations
            .get(&(1, 2))
            .map(|relation| relation.at_war),
        Some(true)
    );
    assert_eq!(
        world
            .polity_relations
            .get(&(2, 1))
            .map(|relation| relation.at_war),
        Some(true)
    );
}
