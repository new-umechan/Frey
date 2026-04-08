use super::*;
use crate::sim::polity::PolityRelation;
use crate::sim::world::{
    CellFieldId, CellId, ComponentPatch, EntityBundle, EntityRef, FeedbackEntry, FeedbackPayload,
    FieldValue, GeologyState, ModuleId, PolityComponent, PolityId, RegionComponent, RegionId,
    SettlementComponent, SettlementId, TargetRef, World, WorldMesh,
};
use crate::PlateId;

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
        geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
        boundary_condition: vec![0.0; 4],
    };
    World::new(mesh, geology)
}

#[test]
fn glaciology_forcing_updates_geology_height_once_per_delta() {
    let mut world = build_test_world();
    let mut hydrology_state: HydrologyExecState = None;
    world.state.glaciology.isostatic_adjustment = vec![-0.02, 0.0, 0.01, -0.03];
    world.state.glaciology.applied_isostatic_adjustment = vec![0.0; 4];

    super::geology::apply_glaciology_forcing_to_geology(&mut world, &mut hydrology_state);

    assert!((world.state.geology.height[0] - 0.43).abs() < 1e-6);
    assert!((world.state.geology.height[2] - -0.24).abs() < 1e-6);
    assert_eq!(
        world.state.glaciology.applied_isostatic_adjustment,
        world.state.glaciology.isostatic_adjustment
    );

    super::geology::apply_glaciology_forcing_to_geology(&mut world, &mut hydrology_state);
    assert!((world.state.geology.height[0] - 0.43).abs() < 1e-6);
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
    let mut sliced_feedback = crate::sim::world::FeedbackQueue::new(sliced_world.cell_count());
    full_world.clock.epoch = EraKind::Environment;
    sliced_world.clock.epoch = EraKind::Environment;

    exec_world(&mut full_world);

    let mut phase = ExecWorldPhase::Prepare;
    let mut completed = 0;
    while completed == 0 {
        let result = exec_world_slice(&mut sliced_world, &mut sliced_feedback, phase, 1);
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
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());
    world.clock.epoch = EraKind::Crust;
    world.clock.tick = 1;

    feedback.push(FeedbackEntry {
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
    feedback.push(FeedbackEntry {
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
    feedback.push(FeedbackEntry {
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

    exec_world_with_feedback(&mut world, &mut feedback);

    assert!((world.state.domesticates.crop_adoption[0][0] - 0.42).abs() < 1e-6);
    assert!((world.state.domesticates.livestock_adoption[0][0] - 0.31).abs() < 1e-6);
    assert!((world.state.domesticates.crop_adoption[2][1] - 0.77).abs() < 1e-6);
}

#[test]
fn feedback_payload_trigger_epoch_transition_is_ignored() {
    let mut world = build_test_world();
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());
    world.clock.epoch = EraKind::Crust;
    world.clock.tick = 1;
    feedback.push(FeedbackEntry {
        source: ModuleId::Exec,
        target_module: ModuleId::Exec,
        target_ref: TargetRef::Global,
        enqueued_tick: 0,
        payload: FeedbackPayload::TriggerEpochTransition {
            to: EraKind::History,
        },
    });

    exec_world_with_feedback(&mut world, &mut feedback);
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

    assert!(world.entities.iter_regions().count() >= 1);
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
fn feedback_entity_payloads_use_entity_store() {
    let mut world = build_test_world();
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());
    world.clock.epoch = EraKind::History;
    world.clock.tick = 1;

    feedback.push(FeedbackEntry {
        source: ModuleId::Conflict,
        target_module: ModuleId::Polity,
        target_ref: TargetRef::Polity(PolityId(9)),
        enqueued_tick: 0,
        payload: FeedbackPayload::SpawnEntity {
            bundle: EntityBundle::Polity(PolityComponent {
                polity_id: PolityId(9),
                capital_cell: CellId(1),
                legitimacy: 0.4,
                centralization: 0.5,
                military_tech: 0.6,
                cells_cache: vec![CellId(1)],
            }),
        },
    });
    feedback.push(FeedbackEntry {
        source: ModuleId::Conflict,
        target_module: ModuleId::Settlement,
        target_ref: TargetRef::Settlement(SettlementId(3)),
        enqueued_tick: 0,
        payload: FeedbackPayload::SpawnEntity {
            bundle: EntityBundle::Settlement(SettlementComponent {
                settlement_id: SettlementId(3),
                cell: CellId(2),
            }),
        },
    });
    feedback.push(FeedbackEntry {
        source: ModuleId::Conflict,
        target_module: ModuleId::Conflict,
        target_ref: TargetRef::Region(RegionId(5)),
        enqueued_tick: 0,
        payload: FeedbackPayload::SpawnEntity {
            bundle: EntityBundle::Region(RegionComponent {
                region_id: RegionId(5),
                cells: vec![CellId(0), CellId(1)],
            }),
        },
    });
    feedback.push(FeedbackEntry {
        source: ModuleId::Conflict,
        target_module: ModuleId::Polity,
        target_ref: TargetRef::Polity(PolityId(9)),
        enqueued_tick: 0,
        payload: FeedbackPayload::MutateEntity {
            entity: EntityRef::Polity(PolityId(9)),
            patch: ComponentPatch::Polity {
                capital_cell: Some(CellId(3)),
                legitimacy: Some(0.9),
                centralization: Some(0.8),
                military_tech: Some(0.7),
                cells_cache: Some(vec![CellId(3)]),
            },
        },
    });
    feedback.push(FeedbackEntry {
        source: ModuleId::Conflict,
        target_module: ModuleId::Settlement,
        target_ref: TargetRef::Settlement(SettlementId(3)),
        enqueued_tick: 0,
        payload: FeedbackPayload::MutateEntity {
            entity: EntityRef::Settlement(SettlementId(3)),
            patch: ComponentPatch::Settlement {
                cell: Some(CellId(1)),
            },
        },
    });
    feedback.push(FeedbackEntry {
        source: ModuleId::Conflict,
        target_module: ModuleId::Conflict,
        target_ref: TargetRef::Region(RegionId(5)),
        enqueued_tick: 0,
        payload: FeedbackPayload::MutateEntity {
            entity: EntityRef::Region(RegionId(5)),
            patch: ComponentPatch::Region {
                cells: Some(vec![CellId(2), CellId(3)]),
            },
        },
    });

    crate::sim::exec::feedback::apply_feedback_queue(&mut world, &mut feedback);

    let polity = world.entities.get_polity(PolityId(9)).unwrap();
    assert_eq!(polity.capital_cell, CellId(3));
    assert!((polity.legitimacy - 0.9).abs() < 1e-6);
    assert!((polity.centralization - 0.8).abs() < 1e-6);
    assert!((polity.military_tech - 0.7).abs() < 1e-6);
    assert_eq!(polity.cells_cache, vec![CellId(3)]);

    let settlement = world.entities.get_settlement(SettlementId(3)).unwrap();
    assert_eq!(settlement.cell, CellId(1));

    let region = world.entities.get_region(RegionId(5)).unwrap();
    assert_eq!(region.cells, vec![CellId(2), CellId(3)]);

    feedback.push(FeedbackEntry {
        source: ModuleId::Conflict,
        target_module: ModuleId::Polity,
        target_ref: TargetRef::Polity(PolityId(9)),
        enqueued_tick: 0,
        payload: FeedbackPayload::DestroyEntity {
            entity: EntityRef::Polity(PolityId(9)),
        },
    });
    crate::sim::exec::feedback::apply_feedback_queue(&mut world, &mut feedback);

    assert!(world.entities.get_polity(PolityId(9)).is_none());
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
    assert_eq!(world.entities.iter_regions().count(), 3);
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
