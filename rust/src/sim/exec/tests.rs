use super::*;
use crate::sim::exec::modules::MODULE_DECLARATIONS;
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
        plate_emergence_regime: Default::default(),
        plate_emergence_fallback: Default::default(),
        initial_plate_kinematics: Vec::new(),
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
fn hydrology_deposition_is_limited_by_fluvial_erosion_budget() {
    let mut world = build_test_world();
    let mut geology_state = None;
    let mut hydrology_state = None;
    world.state.hydrology.erosion_rate = vec![0.10, 0.05, 0.0, 0.0];
    world.state.hydrology.deposition_rate = vec![0.20, 0.20, 0.10, 0.10];
    world.state.glaciology.glacial_erosion_rate = vec![0.0; 4];
    super::geology::apply_hydrology_erosion_to_geology(
        &mut world,
        &mut geology_state,
        &mut hydrology_state,
    );

    let applied_deposition = world.state.hydrology.deposition_rate.iter().sum::<f32>();
    let applied_erosion = world.state.hydrology.erosion_rate.iter().sum::<f32>();

    assert!((applied_deposition - applied_erosion).abs() < 1e-6);
    assert!(world.state.hydrology.deposition_rate[0] < 0.20);
    assert!(world.state.hydrology.deposition_rate[3] < 0.10);
}

#[test]
fn hydrology_budget_constraint_does_not_recenter_global_heights() {
    let mut world = build_test_world();
    let mut geology_state = None;
    let mut hydrology_state = None;
    let initial_height = world.state.geology.height.clone();
    world.control.sea_level_offset = -0.25;
    world.state.hydrology.erosion_rate = vec![0.0; 4];
    world.state.hydrology.deposition_rate = vec![0.0; 4];
    world.state.glaciology.glacial_erosion_rate = vec![0.0; 4];

    super::geology::apply_hydrology_erosion_to_geology(
        &mut world,
        &mut geology_state,
        &mut hydrology_state,
    );

    assert_eq!(world.state.geology.height, initial_height);
}

#[test]
fn crust_era_hydrology_does_not_modify_geology_height() {
    let mut world = build_test_world();
    let mut geology_state = None;
    let mut hydrology_state = None;
    let initial_height = world.state.geology.height.clone();
    world.clock.epoch = EraKind::Crust;
    world.state.hydrology.erosion_rate = vec![0.20, 0.15, 0.05, 0.01];
    world.state.hydrology.deposition_rate = vec![0.10, 0.10, 0.10, 0.10];
    world.state.glaciology.glacial_erosion_rate = vec![0.02; 4];

    super::geology::apply_hydrology_erosion_to_geology(
        &mut world,
        &mut geology_state,
        &mut hydrology_state,
    );

    assert_eq!(world.state.geology.height, initial_height);
}

#[test]
fn hydrology_budget_constraint_does_not_override_sea_level_offset() {
    let mut world = build_test_world();
    let mut geology_state = None;
    let mut hydrology_state = None;
    world.control.sea_level_offset = 0.12;
    world.state.hydrology.erosion_rate = vec![0.0; 4];
    world.state.hydrology.deposition_rate = vec![0.0; 4];
    world.state.glaciology.glacial_erosion_rate = vec![0.0; 4];

    super::geology::apply_hydrology_erosion_to_geology(
        &mut world,
        &mut geology_state,
        &mut hydrology_state,
    );

    assert!((world.control.sea_level_offset - 0.12).abs() < 1e-6);
}

#[test]
fn hydrology_mfd_runs_in_later_eras_when_height_changed() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::History;
    let hydrology_state = Some(crate::sim::build_hydrology_state_for_bench(
        &world,
        world.control.geology_params.clone(),
    ));

    world.state.geology.height[0] += 0.01;

    assert!(super::geology::should_run_hydrology_mfd_for_geology(
        &world,
        None,
        hydrology_state.as_ref(),
    ));
}

#[test]
fn hydrology_mfd_skips_in_later_eras_without_height_change_and_low_activity() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::History;
    let hydrology_state = Some(crate::sim::build_hydrology_state_for_bench(
        &world,
        world.control.geology_params.clone(),
    ));
    let geology_state = Some(crate::sim::world::GeologyDynamicsState {
        update_index: 0,
        plate_states: Vec::new(),
        vertex_states: Vec::new(),
        boundary_state: crate::sim::world::BoundaryDynamicsState::default(),
        mantle_heat: Vec::new(),
        cached_metrics: crate::sim::world::GeologyStepMetrics::default(),
        boundary_front_accumulators: Vec::new(),
        plate_material: Vec::new(),
        plate_area_targets: Vec::new(),
        plate_influence_centers: Vec::new(),
        plate_velocity_centers: Vec::new(),
        surface_material: Vec::new(),
        surface_material_elements: Vec::new(),
        previous_surface_plate_id: Vec::new(),
        plate_surface_polygons: Vec::new(),
        plate_boundary_topology: Default::default(),
    });

    assert!(!super::geology::should_run_hydrology_mfd_for_geology(
        &world,
        geology_state.as_ref(),
        hydrology_state.as_ref(),
    ));
}

#[test]
fn hydrology_flow_step_refreshes_public_lake_flags_on_skip() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::History;
    world.clock.tick = 10;
    world.control.geology_params.sink_full_rebuild_changed_ratio = 1.0;
    world
        .control
        .geology_params
        .sink_full_rebuild_interval_ticks = u32::MAX;
    world.state.geology.height = vec![0.4, 0.2, -0.3, 0.1];
    let params = world.control.geology_params.clone();
    let mut hydrology_state = Some(crate::sim::build_hydrology_state_for_bench(&world, params));

    {
        let hydrology = &mut world.state.hydrology;
        hydrology.sink_id = vec![0, 0, -1, -1];
        hydrology.sink_route_next = vec![-1; 4];
        hydrology.sink_member_offsets = vec![0, 2];
        hydrology.sink_member_cells = vec![0, 1];
        hydrology.sink_spill_cell = vec![1];
        hydrology.sink_spill_to = vec![2];
        hydrology.sink_spill_level = vec![0.25];
        hydrology.sink_capacity_total = vec![1.0];
        hydrology.sink_capacity_remaining = vec![0.5];
        hydrology.sink_storage_water = vec![0.0];
        hydrology.sink_storage_sediment = vec![0.0];
        hydrology.sink_overflow_active = vec![0];
        hydrology.is_lake.fill(false);
    }
    crate::sim::hydrology::sync_fill_spill_to_erosion(
        hydrology_state
            .as_mut()
            .expect("hydrology state should exist"),
        &world.state.hydrology,
    );
    hydrology_state
        .as_mut()
        .expect("hydrology state should exist")
        .last_sink_full_rebuild_tick = world.clock.tick;

    let detail =
        crate::sim::hydrology::run_hydrology_flow_step(&mut world, &mut hydrology_state, 1);

    assert_eq!(detail.sink_rebuild_skipped_count, 1);
    assert_eq!(
        world.state.hydrology.is_lake,
        vec![false, true, false, false]
    );
}

#[test]
fn geology_step_preserves_crust_land_ratio_target() {
    let mut world = build_test_world();
    let mut geology_state = Some(crate::sim::world::GeologyDynamicsState {
        update_index: 0,
        plate_states: Vec::new(),
        vertex_states: Vec::new(),
        boundary_state: crate::sim::world::BoundaryDynamicsState::default(),
        mantle_heat: Vec::new(),
        cached_metrics: crate::sim::world::GeologyStepMetrics::default(),
        boundary_front_accumulators: Vec::new(),
        plate_material: Vec::new(),
        plate_area_targets: Vec::new(),
        plate_influence_centers: Vec::new(),
        plate_velocity_centers: Vec::new(),
        surface_material: Vec::new(),
        surface_material_elements: Vec::new(),
        previous_surface_plate_id: Vec::new(),
        plate_surface_polygons: Vec::new(),
        plate_boundary_topology: Default::default(),
    });
    world.clock.epoch = EraKind::Crust;
    world.clock.transition.last_land_ratio = 0.5;

    super::geology::run_geology_step_with_state(&mut world, &mut geology_state, 1);

    let land_cells = world
        .state
        .geology
        .height
        .iter()
        .filter(|value| **value > 0.0)
        .count();
    assert_eq!(land_cells, 2);
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

    let mut phase = first_phase();
    let mut completed = 0;
    while completed == 0 {
        let result = exec_world_slice(&mut sliced_world, &mut sliced_feedback, phase, 1);
        phase = result.next_phase;
        completed = result.ticks_completed;
    }

    assert_eq!(phase, first_phase());
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
fn module_declarations_cover_each_exec_phase_once() {
    let phases = declared_phase_order();
    let declared = MODULE_DECLARATIONS
        .iter()
        .map(|declaration| declaration.phase)
        .collect::<Vec<_>>();

    assert_eq!(phases.len(), declared.len());
    assert_eq!(phases.first().copied(), Some(first_phase()));
    assert_eq!(
        phases.last().copied(),
        MODULE_DECLARATIONS
            .iter()
            .find(|declaration| declaration.completes_tick)
            .map(|declaration| declaration.phase)
    );
    for phase in declared {
        assert_eq!(
            phases
                .iter()
                .filter(|candidate| **candidate == phase)
                .count(),
            1
        );
    }
}

#[test]
fn module_dependencies_follow_declared_phase_order() {
    let dependencies = declared_dependencies();
    assert!(dependencies.contains(&ModuleDependency {
        from: ExecWorldPhase::Prepare,
        to: ExecWorldPhase::ExecFeedback,
    }));
    assert!(dependencies.contains(&ModuleDependency {
        from: ExecWorldPhase::ExecFeedback,
        to: ExecWorldPhase::Domesticates,
    }));
    assert!(dependencies.contains(&ModuleDependency {
        from: ExecWorldPhase::Climate,
        to: ExecWorldPhase::Ecology,
    }));
    assert!(dependencies.contains(&ModuleDependency {
        from: ExecWorldPhase::Hydrology,
        to: ExecWorldPhase::Ecology,
    }));
    assert!(!dependencies
        .iter()
        .any(|edge| edge.to == ExecWorldPhase::Prepare));
}

#[test]
fn module_order_remains_stable_under_generated_dependencies() {
    let phases = declared_phase_order();
    let declaration_index = MODULE_DECLARATIONS
        .iter()
        .enumerate()
        .map(|(index, declaration)| (declaration.phase, index))
        .collect::<std::collections::HashMap<_, _>>();

    for window in phases.windows(2) {
        let lhs = declaration_index[&window[0]];
        let rhs = declaration_index[&window[1]];
        assert!(
            lhs < rhs,
            "declaration order regressed: {:?} then {:?}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn exec_feedback_stage_does_not_consume_other_module_entries() {
    let mut world = build_test_world();
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());
    world.clock.tick = 1;
    feedback.push(FeedbackEntry {
        source: ModuleId::Population,
        target_module: ModuleId::Hydrology,
        target_ref: TargetRef::Cell(CellId(0)),
        enqueued_tick: 0,
        payload: FeedbackPayload::SetValue {
            field: CellFieldId::CropAdoption(0),
            cell: CellId(0),
            value: FieldValue::F32(0.5),
        },
    });

    super::pipeline::run_feedback_stage(&mut world, &mut feedback);

    assert_eq!(feedback.entries.len(), 1);
    assert_eq!(feedback.entries[0].target_module, ModuleId::Hydrology);
}

#[test]
fn domesticates_feedback_payload_updates_internal_pressure() {
    let mut world = build_test_world();
    world.clock.tick = 5;
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());
    feedback.push(FeedbackEntry {
        source: ModuleId::Settlement,
        target_module: ModuleId::Domesticates,
        target_ref: TargetRef::Cell(CellId(0)),
        enqueued_tick: 4,
        payload: FeedbackPayload::DeltaF32 {
            field: CellFieldId::DomesticatesRoutedCropFeedback(0),
            cell: CellId(0),
            delta: 0.03,
        },
    });
    feedback.push(FeedbackEntry {
        source: ModuleId::Settlement,
        target_module: ModuleId::Domesticates,
        target_ref: TargetRef::Cell(CellId(0)),
        enqueued_tick: 4,
        payload: FeedbackPayload::DeltaF32 {
            field: CellFieldId::DomesticatesRoutedLivestockFeedback(0),
            cell: CellId(0),
            delta: 0.02,
        },
    });
    feedback.push(FeedbackEntry {
        source: ModuleId::Population,
        target_module: ModuleId::Domesticates,
        target_ref: TargetRef::Cell(CellId(0)),
        enqueued_tick: 4,
        payload: FeedbackPayload::DeltaF32 {
            field: CellFieldId::DomesticatesIntensificationBonus,
            cell: CellId(0),
            delta: 0.4,
        },
    });

    super::feedback::apply_feedback_queue_for_module(
        &mut world,
        &mut feedback,
        ModuleId::Domesticates,
    );

    assert!(feedback.entries.is_empty());
    assert!(
        world.state.domesticates.domesticates_internal[0].routed_feedback_crop[0] > 0.0,
        "crop routed feedback was not applied"
    );
    assert!(
        world.state.domesticates.domesticates_internal[0].routed_feedback_livestock[0] > 0.0,
        "livestock routed feedback was not applied"
    );
    assert!(
        world.state.domesticates.domesticates_internal[0].population_pressure_bonus > 0.0,
        "population pressure bonus was not applied"
    );
}

#[test]
fn population_stage_enqueues_domesticates_population_pressure() {
    let mut world = build_test_world();
    world.clock.tick = 8;
    world.clock.budgets.civilization = 4;
    world.state.geology.height = vec![0.3, 0.2, 0.1, 0.2];
    world.state.population.population = vec![180.0, 0.0, 0.0, 0.0];
    world.state.subsistence.food_energy_mean = vec![0.9, 0.0, 0.0, 0.0];
    world.state.subsistence.food_energy_variance = vec![0.1, 0.9, 0.9, 0.9];
    world.state.subsistence.buffer_capacity = vec![0.8, 0.0, 0.0, 0.0];
    world.state.hydrology.surface_water_access = vec![0.9, 0.0, 0.0, 0.0];
    world.state.ecology.soil_fertility = vec![0.8, 0.0, 0.0, 0.0];
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());

    super::pipeline::run_population_stage(&mut world, &mut feedback);

    assert!(feedback.entries.iter().any(|entry| {
        matches!(
            entry.payload,
            FeedbackPayload::DeltaF32 {
                field: CellFieldId::DomesticatesIntensificationBonus,
                ..
            }
        ) && entry.target_module == ModuleId::Domesticates
            && entry.source == ModuleId::Population
    }));
}

#[test]
fn settlement_stage_enqueues_domesticates_spread_feedback() {
    let mut world = build_test_world();
    world.clock.tick = 9;
    world.clock.budgets.civilization = 4;
    world.state.geology.height = vec![0.3, 0.2, 0.1, 0.2];
    world.state.population.population = vec![120.0, 40.0, 0.0, 0.0];
    world.state.domesticates.crop_adoption[0][0] = 0.7;
    world.state.domesticates.livestock_adoption[0][0] = 0.6;
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());

    super::pipeline::run_settlement_stage(&mut world, &mut feedback);

    assert!(feedback.entries.iter().any(|entry| {
        matches!(
            entry.payload,
            FeedbackPayload::DeltaF32 {
                field: CellFieldId::DomesticatesRoutedCropFeedback(_)
                    | CellFieldId::DomesticatesRoutedLivestockFeedback(_),
                ..
            }
        ) && entry.target_module == ModuleId::Domesticates
            && entry.source == ModuleId::Settlement
    }));
}

#[test]
fn settlement_stage_limits_domesticates_spread_to_neighboring_settlements() {
    let mut world = build_test_world();
    world.mesh_mut().nbr_offsets = vec![0, 1, 3, 5, 6];
    world.mesh_mut().nbrs = vec![1, 0, 2, 1, 3, 2];
    world.clock.tick = 9;
    world.clock.budgets.civilization = 4;
    world.state.geology.height = vec![0.3, 0.2, 0.1, 0.2];
    world.state.population.population = vec![120.0, 40.0, 35.0, 0.0];
    world.state.domesticates.crop_adoption[0][0] = 0.7;
    world.state.domesticates.livestock_adoption[0][0] = 0.6;
    let mut feedback = crate::sim::world::FeedbackQueue::new(world.cell_count());

    super::pipeline::run_settlement_stage(&mut world, &mut feedback);

    let targets = feedback
        .entries
        .iter()
        .filter_map(|entry| match entry.payload {
            FeedbackPayload::DeltaF32 {
                field:
                    CellFieldId::DomesticatesRoutedCropFeedback(_)
                    | CellFieldId::DomesticatesRoutedLivestockFeedback(_),
                cell,
                ..
            } => Some(cell.as_usize()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(targets.contains(&1));
    assert!(!targets.contains(&2));
}

#[test]
fn module_manifest_includes_generated_dependencies() {
    let manifests = module_manifests();
    let ecology = manifests
        .iter()
        .find(|manifest| manifest.phase == ExecWorldPhase::Ecology)
        .expect("ecology manifest is missing");
    let domesticates = manifests
        .iter()
        .find(|manifest| manifest.phase == ExecWorldPhase::Domesticates)
        .expect("domesticates manifest is missing");

    assert!(ecology.depends_on.contains(&ExecWorldPhase::Climate));
    assert!(ecology.depends_on.contains(&ExecWorldPhase::Hydrology));
    assert!(domesticates
        .depends_on
        .contains(&ExecWorldPhase::ExecFeedback));
    assert!(domesticates.depends_on.contains(&ExecWorldPhase::Ecology));
    assert_eq!(ecology.phase_key, "ecology");
    assert_eq!(ecology.module_key, "ecology");
    assert_eq!(ecology.description, "update biome and ecosystem state");
    assert_eq!(ecology.feedback_mode, FeedbackMode::ModuleInbox);
    assert_eq!(ecology.profile_category, ProfileCategory::Ecology);
    assert_eq!(ecology.display_group, DisplayGroup::Ecology);
    assert_eq!(ecology.execution_kind, ExecutionKind::Plain);
    assert!(!ecology.completes_tick);
    assert_eq!(domesticates.feedback_mode, FeedbackMode::ModuleInbox);
    assert_eq!(domesticates.profile_category, ProfileCategory::Society);
    assert_eq!(domesticates.display_group, DisplayGroup::Society);
}

#[test]
fn module_manifest_lines_expose_doc_facing_metadata() {
    let lines = module_manifest_lines();
    let ecology = lines
        .iter()
        .find(|line| line.starts_with("ecology "))
        .expect("ecology manifest line is missing");
    let exec_feedback = lines
        .iter()
        .find(|line| line.starts_with("exec_feedback "))
        .expect("exec_feedback manifest line is missing");

    assert!(ecology
        .contains("reads=clock,terrain_projection,climate_cells,hydrology_cells,ecology_cells"));
    assert!(ecology.contains("inbox=module_inbox"));
    assert!(ecology.contains("profile=ecology"));
    assert!(ecology.contains("display=ecology"));
    assert!(ecology.contains("exec=plain"));
    assert!(ecology.contains("tick_boundary=no"));
    assert!(ecology.contains("depends_on="));
    assert!(ecology.contains("climate"));
    assert!(ecology.contains("hydrology"));
    assert!(ecology.contains("desc=\"update biome and ecosystem state\""));
    assert!(exec_feedback.contains("[exec]"));
    assert!(exec_feedback.contains("inbox=exec_inbox"));
    assert!(exec_feedback.contains("profile=feedback"));
    assert!(exec_feedback.contains("display=feedback"));
    assert!(exec_feedback.contains("exec=plain"));
    assert!(exec_feedback
        .contains("desc=\"apply global exec-targeted feedback queued before this tick\""));
}

#[test]
fn module_doc_records_expose_structured_doc_metadata() {
    let records = module_doc_records();
    let hydrology = records
        .iter()
        .find(|record| record.phase == "hydrology")
        .expect("hydrology doc record is missing");
    let finalize = records
        .iter()
        .find(|record| record.phase == "finalize")
        .expect("finalize doc record is missing");

    assert_eq!(hydrology.module, "hydrology");
    assert_eq!(hydrology.profile, "hydrology");
    assert_eq!(hydrology.display, "hydrology");
    assert_eq!(hydrology.execution, "hydrology_coupled");
    assert!(hydrology.reads.contains(&"terrain_projection"));
    assert!(hydrology.feedback_targets.contains(&"ecology"));
    assert!(hydrology.depends_on.contains(&"glaciology"));
    assert_eq!(finalize.display, "post_step");
    assert!(finalize.tick_boundary);
}

#[test]
fn module_doc_records_are_ready_for_wasm_export() {
    let records = module_doc_records();
    let serialized = serde_json::to_value(&records).expect("module doc records should serialize");
    let array = serialized
        .as_array()
        .expect("serialized module records should be an array");
    let first = array
        .first()
        .expect("serialized module records should not be empty");

    assert_eq!(
        first.get("phase").and_then(|value| value.as_str()),
        Some("prepare")
    );
    assert_eq!(
        first.get("display").and_then(|value| value.as_str()),
        Some("feedback")
    );
}

#[test]
fn module_graph_record_is_ready_for_wasm_export() {
    let graph = module_graph_record();
    let serialized = serde_json::to_value(&graph).expect("module graph should serialize");
    let modules = serialized
        .get("modules")
        .and_then(|value| value.as_array())
        .expect("module graph should include modules");
    let edges = serialized
        .get("edges")
        .and_then(|value| value.as_array())
        .expect("module graph should include edges");

    assert!(!modules.is_empty());
    assert!(!edges.is_empty());
    let first_edge = edges
        .first()
        .expect("module graph should include at least one edge");
    assert!(first_edge.get("from_phase").is_some());
    assert!(first_edge.get("to_phase").is_some());
}

#[test]
fn feedback_mode_helpers_follow_declarations() {
    assert!(!phase_accepts_module_feedback(ExecWorldPhase::Prepare));
    assert!(!phase_accepts_module_feedback(ExecWorldPhase::ExecFeedback));
    assert!(phase_accepts_exec_feedback(ExecWorldPhase::ExecFeedback));
    assert_eq!(
        phase_profile_category(ExecWorldPhase::ExecFeedback),
        ProfileCategory::Feedback
    );
    assert_eq!(
        phase_profile_category(ExecWorldPhase::Domesticates),
        ProfileCategory::Society
    );
    assert_eq!(
        phase_execution_kind(ExecWorldPhase::Hydrology),
        ExecutionKind::HydrologyCoupled
    );
    assert_eq!(
        phase_display_group(ExecWorldPhase::Population),
        DisplayGroup::Society
    );
    assert_eq!(
        phase_display_group(ExecWorldPhase::Finalize),
        DisplayGroup::PostStep
    );
    assert_eq!(
        phase_execution_kind(ExecWorldPhase::Climate),
        ExecutionKind::Plain
    );
    assert!(phase_completes_tick(ExecWorldPhase::Finalize));
    assert!(!phase_completes_tick(ExecWorldPhase::Transition));
    assert!(phase_accepts_module_feedback(ExecWorldPhase::Hydrology));
    assert!(phase_accepts_module_feedback(ExecWorldPhase::Conflict));
    assert!(!phase_accepts_exec_feedback(ExecWorldPhase::Finalize));
}

#[test]
fn exec_world_profiled_uses_declared_profile_categories() {
    let mut world = build_test_world();
    world.clock.epoch = EraKind::Environment;

    let breakdown = exec_world_profiled(&mut world);

    assert!(breakdown.exec_feedback_ms.is_finite());
    assert!(breakdown.exec_geology_terrain_ms.is_finite());
    assert!(breakdown.exec_climate_ms.is_finite());
    assert!(breakdown.exec_glaciology_ms.is_finite());
    assert!(breakdown.exec_hydrology_ms.is_finite());
    assert!(breakdown.exec_ecology_ms.is_finite());
    assert!(breakdown.exec_society_ms.is_finite());
    assert!(breakdown.exec_transition_ms.is_finite());
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
    world.state.subsistence.food_energy_mean = vec![0.9, 0.9, 0.0, 0.0];
    world.state.ecology.soil_fertility = vec![0.9, 0.8, 0.0, 0.0];

    exec_world(&mut world);

    assert!(world.entities.iter_regions().count() >= 1);
    assert_eq!(
        world
            .relations
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
fn feedback_entity_payloads_use_entity_state() {
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

    crate::sim::exec::feedback::apply_feedback_queue_for_module(
        &mut world,
        &mut feedback,
        ModuleId::Polity,
    );
    crate::sim::exec::feedback::apply_feedback_queue_for_module(
        &mut world,
        &mut feedback,
        ModuleId::Settlement,
    );
    crate::sim::exec::feedback::apply_feedback_queue_for_module(
        &mut world,
        &mut feedback,
        ModuleId::Conflict,
    );

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
    crate::sim::exec::feedback::apply_feedback_queue_for_module(
        &mut world,
        &mut feedback,
        ModuleId::Polity,
    );

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
        .relations
        .polity_relations
        .insert((PolityId(1), PolityId(2)), PolityRelation::default());
    world
        .relations
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
            .relations
            .polity_relations
            .get(&(PolityId(1), PolityId(2)))
            .map(|relation| relation.at_war),
        Some(true)
    );
    assert_eq!(
        world
            .relations
            .polity_relations
            .get(&(PolityId(2), PolityId(1)))
            .map(|relation| relation.at_war),
        Some(true)
    );
}
