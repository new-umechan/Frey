use super::*;
use crate::sim::polity::PolityRelation;
use crate::sim::world::{
    CellFieldId, CellId, FeedbackEntry, FeedbackPayload, FieldValue, GeologyState, ModuleId,
    PlateId, PolityId, TargetRef, World, WorldMesh,
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
        lake_depth: vec![0.0; 4],
        plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
        erosion_rate: vec![0.0; 4],
        deposition_rate: vec![0.0; 4],
        volcanism: vec![0.0; 4],
        vertex_buoyancy: vec![0.0; 4],
        geology_internal: vec![crate::sim::world::GeologyInternal::default(); 4],
        boundary_condition: vec![0.0; 4],
    };
    World::new(mesh, geology)
}

#[test]
fn exec_world_advances_tick_and_sets_budget_to_one() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::History;
    world.clock.tick = 1_445;
    exec_world(&mut world);
    assert_eq!(world.clock.tick, 1_446);
    assert_eq!(world.clock.budgets.geology, 1);
    assert_eq!(world.clock.budgets.climate, 1);
    assert_eq!(world.clock.budgets.ecology, 1);
    assert_eq!(world.clock.budgets.civilization, 4);
}

#[test]
fn climate_water_budget_residual_stays_bounded() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::Environment;
    exec_world(&mut world);

    let diagnostics = crate::sim::climate::surface::last_precip_diagnostics_summary();
    assert!(diagnostics.budget_residual_ratio.is_finite());
    assert!(diagnostics.budget_residual_ratio <= 5.0);
}

#[test]
fn exec_world_slice_matches_full_tick_execution() {
    let mut full_world = build_test_world();
    let mut sliced_world = build_test_world();
    full_world.clock.epoch = EraKind::Environment;
    sliced_world.clock.epoch = EraKind::Environment;

    exec_world(&mut full_world);

    let mut phase = ExecWorldPhase::Prepare;
    let mut completed = 0;
    while completed == 0 {
        let result = exec_world_slice(&mut sliced_world, phase, 1);
        phase = result.next_phase;
        completed = result.ticks_completed;
    }

    assert_eq!(phase, ExecWorldPhase::Prepare);
    assert_eq!(sliced_world.clock.tick, full_world.clock.tick);
    assert_eq!(sliced_world.clock.epoch, full_world.clock.epoch);
    assert_eq!(
        sliced_world.clock.budgets.geology,
        full_world.clock.budgets.geology
    );
    assert_eq!(
        sliced_world.state.geology.height,
        full_world.state.geology.height
    );
    assert_eq!(
        sliced_world.state.hydrology.river_flow,
        full_world.state.hydrology.river_flow
    );
    assert_eq!(
        sliced_world.state.hydrology.river_next,
        full_world.state.hydrology.river_next
    );
}

#[test]
fn feedback_queue_applies_entries_on_next_tick() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::Crust;
    world.clock.tick = 1;

    world.feedback.push(FeedbackEntry {
        source: ModuleId::Subsistence,
        target_module: ModuleId::Hydrology,
        target_ref: TargetRef::Cell(CellId(0)),
        enqueued_tick: 0,
        payload: FeedbackPayload::SetValue {
            field: CellFieldId::CropAdoption(0),
            cell: CellId(0),
            value: FieldValue::F32(0.42),
        },
    });
    world.feedback.push(FeedbackEntry {
        source: ModuleId::Subsistence,
        target_module: ModuleId::Hydrology,
        target_ref: TargetRef::Cell(CellId(0)),
        enqueued_tick: 0,
        payload: FeedbackPayload::SetValue {
            field: CellFieldId::LivestockAdoption(0),
            cell: CellId(0),
            value: FieldValue::F32(0.31),
        },
    });
    world.feedback.push(FeedbackEntry {
        source: ModuleId::Population,
        target_module: ModuleId::Ecology,
        target_ref: TargetRef::Cell(CellId(2)),
        enqueued_tick: 0,
        payload: FeedbackPayload::DeltaF32 {
            field: CellFieldId::CropAdoption(1),
            cell: CellId(2),
            delta: 0.77,
        },
    });

    exec_world(&mut world);

    assert!((world.state.domesticates.crop_adoption[0][0] - 0.42).abs() < 1e-6);
    assert!((world.state.domesticates.livestock_adoption[0][0] - 0.31).abs() < 1e-6);
    assert!((world.state.domesticates.crop_adoption[2][1] - 0.77).abs() < 1e-6);
}

#[test]
fn feedback_payload_trigger_epoch_transition_is_ignored() {
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
    assert_eq!(world.clock.epoch, EraKind::Crust);
}

#[test]
fn fixed_tick_transition_changes_era_at_end_of_tick() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::Crust;
    world.clock.tick = 799;

    exec_world(&mut world);

    assert_eq!(world.clock.tick, 800);
    assert_eq!(world.clock.epoch, EraKind::Environment);
}

#[test]
fn fixed_tick_transition_keeps_era_before_boundary() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::Environment;
    world.clock.tick = 1_298;

    exec_world(&mut world);

    assert_eq!(world.clock.tick, 1_299);
    assert_eq!(world.clock.epoch, EraKind::Environment);
}

#[test]
fn fixed_tick_transition_matches_all_remaining_boundaries() {
    let mut world = build_test_world();

    world.clock.epoch = EraKind::Environment;
    world.clock.tick = 1_299;
    exec_world(&mut world);
    assert_eq!(world.clock.tick, 1_300);
    assert_eq!(world.clock.epoch, EraKind::Life);

    world.clock.epoch = EraKind::Life;
    world.clock.tick = 1_394;
    exec_world(&mut world);
    assert_eq!(world.clock.tick, 1_395);
    assert_eq!(world.clock.epoch, EraKind::Civilization);

    world.clock.epoch = EraKind::Civilization;
    world.clock.tick = 1_444;
    exec_world(&mut world);
    assert_eq!(world.clock.tick, 1_445);
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
            .get(&(PolityId(1), PolityId(2)))
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
    world.state.polity.polity_id = vec![
        Some(PolityId(10)),
        Some(PolityId(20)),
        Some(PolityId(30)),
        Some(PolityId(40)),
    ];

    crate::sim::polity::update_polity(&mut world, 1);

    assert_eq!(
        world.state.polity.polity_id,
        vec![Some(PolityId(1)), None, Some(PolityId(3)), None]
    );
}

#[test]
fn conflict_update_treats_none_polity_as_unclaimed_and_clears_occupiers() {
    let mut world = build_test_world();
    world.state.polity.polity_id = vec![Some(PolityId(1)), None, Some(PolityId(2)), None];
    world.state.conflict.occupier_id = vec![
        Some(PolityId(9)),
        Some(PolityId(9)),
        Some(PolityId(9)),
        Some(PolityId(9)),
    ];
    world
        .polity_relations
        .insert((PolityId(1), PolityId(2)), PolityRelation::default());
    world
        .polity_relations
        .insert((PolityId(2), PolityId(1)), PolityRelation::default());

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
            .get(&(PolityId(1), PolityId(2)))
            .map(|relation| relation.at_war),
        Some(true)
    );
    assert_eq!(
        world
            .polity_relations
            .get(&(PolityId(2), PolityId(1)))
            .map(|relation| relation.at_war),
        Some(true)
    );
}
