#![cfg(feature = "wasm_transport")]

use crate::application::world_dto::{
    ExecWorldSliceResponse, ForkWorldOutput, InitWorldConfig, InitWorldOutput, RestoreWorldResult,
    StepWorldProfiledDetailResponse, StepWorldProfiledResponse,
};
use crate::application::world_runtime::{
    InterventionCommand, ManagedWorld, ManagedWorldExecState, WorldArchive, WorldTransportCache,
};
use crate::application::world_service::WorldService;
use crate::application::world_support::{
    build_erosion_state, post_step_sync_light, sync_erosion_state,
};
use crate::sim;
use crate::sim::geology_types::GeologyInternal;
use crate::sim::{
    display_group_key, exec_world_profiled_detailed_with_feedback_and_states,
    exec_world_slice_with_states, exec_world_with_feedback_and_states, first_phase,
    phase_display_group, world, ExecWorldBreakdown, ExecWorldBreakdownDetailed, ExecWorldPhase,
};
use verification_runtime::{
    run_post_step as run_post_step_runtime,
    run_post_step_profiled as run_post_step_profiled_runtime, PostStepProfile, PostStepRuntime,
    ProfileClock, VerificationMode,
};

fn world_not_found_error(world_id: &str) -> String {
    format!("world not found: {world_id}")
}

fn history_tick_not_available_error(tick: u64) -> String {
    format!("tick {tick} is not available in history")
}

fn scaled_step_count(simulation_rate: f32, tick_count: u32) -> u32 {
    let scaled_ticks = ((tick_count as f32) * simulation_rate).round() as u32;
    scaled_ticks.max(1)
}

fn reset_pending_slice(managed: &mut ManagedWorld) {
    managed.reset_exec_state();
}

fn exec_phase_label(phase: ExecWorldPhase) -> &'static str {
    display_group_key(phase_display_group(phase))
}

struct WorldPostStepRuntime<'a> {
    managed: &'a mut ManagedWorld,
    archive: &'a mut WorldArchive,
}

impl PostStepRuntime for WorldPostStepRuntime<'_> {
    fn verification_mode(&self) -> VerificationMode {
        self.managed.verification_mode
    }

    fn sync_light(&mut self) {
        post_step_sync_light(self.managed);
    }

    fn observe_after_world_change(&mut self) {
        self.managed.observe_after_world_change();
    }

    fn save_snapshot_if_needed(&mut self) {
        self.archive.save_snapshot_if_needed(self.managed);
    }

    fn refresh_reduced_metrics(&mut self) {
        self.managed.refresh_reduced_metrics();
    }

    fn push_scientific_benchmark_sample(&mut self) {
        self.managed.push_scientific_benchmark_sample();
    }
}

struct DefaultProfileClock;

impl ProfileClock for DefaultProfileClock {
    #[cfg(target_arch = "wasm32")]
    type Stamp = f64;
    #[cfg(not(target_arch = "wasm32"))]
    type Stamp = std::time::Instant;

    #[cfg(target_arch = "wasm32")]
    fn now(&self) -> Self::Stamp {
        js_sys::Date::now()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn now(&self) -> Self::Stamp {
        std::time::Instant::now()
    }

    #[cfg(target_arch = "wasm32")]
    fn elapsed_ms(&self, start: Self::Stamp) -> f64 {
        js_sys::Date::now() - start
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn elapsed_ms(&self, start: Self::Stamp) -> f64 {
        start.elapsed().as_secs_f64() * 1000.0
    }
}

fn run_post_step(managed: &mut ManagedWorld, archive: &mut WorldArchive) {
    let mut runtime = WorldPostStepRuntime { managed, archive };
    run_post_step_runtime(&mut runtime);
}

fn run_post_step_profiled(
    managed: &mut ManagedWorld,
    archive: &mut WorldArchive,
) -> PostStepProfile {
    let mut runtime = WorldPostStepRuntime { managed, archive };
    run_post_step_profiled_runtime(&mut runtime, &DefaultProfileClock)
}

pub(crate) fn init_world(
    service: &mut WorldService,
    seed: String,
    mesh_level: u32,
    config: InitWorldConfig,
) -> Result<InitWorldOutput, String> {
    if mesh_level > 8 {
        return Err("mesh_level must be between 0 and 8".to_string());
    }

    let mut geology_params = config.geology_params.unwrap_or_default();
    geology_params.level = mesh_level;

    let (terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(&seed, geology_params.clone());

    if terrain.height.len() != positions.len() || terrain.plate_id.len() != positions.len() {
        return Err("terrain output does not match mesh vertex count".to_string());
    }

    let geology = world::GeologyState {
        height: terrain.height,
        lake_depth: terrain.lake_depth,
        plate_id: terrain.plate_id,
        erosion_rate: vec![0.0; positions.len()],
        deposition_rate: vec![0.0; positions.len()],
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); positions.len()],
        boundary_condition: vec![0.0; positions.len()],
    };

    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };

    let mut sim_world = world::World::new(mesh, geology);
    sim_world.state.hydrology.river_flow = terrain.river_flux;
    sim_world.state.hydrology.river_next = terrain.river_next;
    crate::sim::hydrology::rebuild_mfd_from_primary(&mut sim_world.state.hydrology);
    if let Some(target) = config.target_sea_ratio {
        sim_world.control.target_sea_ratio = target.clamp(0.02, 0.98);
    }
    sim_world.control.geology_params = geology_params.clone();
    sim_world.control.erosion_thickness_coupling = geology_params.erosion_thickness_coupling;
    sim_world.control.deposition_thickness_coupling = geology_params.deposition_thickness_coupling;
    sim_world.clock.epoch = world::EraKind::Crust;

    let erosion_state = build_erosion_state(&sim_world, geology_params.clone());
    let _ = crate::sim::hydrology::apply_hydrology_state_view(&mut sim_world, &erosion_state);
    let geology_dynamics = sim_world.exec_scratch.geology_dynamics.take();
    let transport_cache = WorldTransportCache::from_world(&sim_world, geology_dynamics.as_ref());
    let cell_count = sim_world.cell_count();

    let managed = ManagedWorld {
        world: sim_world,
        hydrology_dynamics: Some(erosion_state),
        geology_dynamics,
        feedback: world::FeedbackQueue::new(cell_count),
        simulation_rate: config.simulation_rate.unwrap_or(1.0).clamp(0.1, 32.0),
        verification_mode: config
            .verification_mode
            .unwrap_or(VerificationMode::Interactive),
        reduced_metrics: None,
        scientific_benchmark_samples: Vec::new(),
        geology_params,
        transport_cache,
        exec_state: ManagedWorldExecState::default(),
        applied_intervention_seq: 0,
    };
    let mut managed = managed;
    managed.refresh_reduced_metrics();

    let mut archive = WorldArchive::new();
    archive.insert_snapshot(managed.world.clock.tick, managed.snapshot_world());

    let tick = managed.world.clock.tick as f64;
    let era = managed.world.clock.epoch.as_key().to_string();
    let cell_count = managed.world.state.geology.height.len() as u32;
    let world_id = service.insert_world(managed, archive);

    Ok(InitWorldOutput {
        world_id,
        tick,
        era,
        cell_count,
    })
}

pub(crate) fn exec_world(
    service: &mut WorldService,
    world_id: &str,
    tick_count: u32,
) -> Result<(), String> {
    if tick_count == 0 {
        return Ok(());
    }
    let (managed, archive) = service
        .world_and_archive_mut(world_id)
        .ok_or_else(|| world_not_found_error(world_id))?;
    reset_pending_slice(managed);

    let steps = scaled_step_count(managed.simulation_rate, tick_count);
    for _ in 0..steps {
        archive.apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
        managed.with_exec_states(exec_world_with_feedback_and_states);
        run_post_step(managed, archive);
    }
    Ok(())
}

pub(crate) fn exec_world_profiled(
    service: &mut WorldService,
    world_id: String,
    tick_count: u32,
) -> Result<StepWorldProfiledResponse, String> {
    if tick_count == 0 {
        return Ok(StepWorldProfiledResponse {
            world_id,
            steps: 0,
            exec_feedback_ms: 0.0,
            exec_geology_terrain_ms: 0.0,
            exec_climate_ms: 0.0,
            exec_glaciology_ms: 0.0,
            exec_hydrology_ms: 0.0,
            exec_ecology_ms: 0.0,
            exec_society_ms: 0.0,
            exec_transition_ms: 0.0,
            step_sync_erosion_ms: 0.0,
            step_observe_world_change_ms: 0.0,
            step_history_snapshot_ms: 0.0,
        });
    }
    let (managed, archive) = service
        .world_and_archive_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;
    reset_pending_slice(managed);

    let steps = scaled_step_count(managed.simulation_rate, tick_count);
    let mut sim_breakdown = ExecWorldBreakdown::default();
    let mut step_sync_erosion_ms = 0.0;
    let mut step_observe_world_change_ms = 0.0;
    let mut step_history_snapshot_ms = 0.0;

    for _ in 0..steps {
        archive.apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
        let step_breakdown = managed
            .with_exec_states(exec_world_profiled_detailed_with_feedback_and_states)
            .breakdown;
        sim_breakdown.accumulate(&step_breakdown);
        let profile = run_post_step_profiled(managed, archive);
        step_sync_erosion_ms += profile.step_sync_erosion_ms;
        step_observe_world_change_ms += profile.step_observe_world_change_ms;
        step_history_snapshot_ms += profile.step_history_snapshot_ms;
    }

    Ok(StepWorldProfiledResponse {
        world_id,
        steps,
        exec_feedback_ms: sim_breakdown.exec_feedback_ms,
        exec_geology_terrain_ms: sim_breakdown.exec_geology_terrain_ms,
        exec_climate_ms: sim_breakdown.exec_climate_ms,
        exec_glaciology_ms: sim_breakdown.exec_glaciology_ms,
        exec_hydrology_ms: sim_breakdown.exec_hydrology_ms,
        exec_ecology_ms: sim_breakdown.exec_ecology_ms,
        exec_society_ms: sim_breakdown.exec_society_ms,
        exec_transition_ms: sim_breakdown.exec_transition_ms,
        step_sync_erosion_ms,
        step_observe_world_change_ms,
        step_history_snapshot_ms,
    })
}

pub(crate) fn exec_world_profiled_detail(
    service: &mut WorldService,
    world_id: String,
    tick_count: u32,
) -> Result<StepWorldProfiledDetailResponse, String> {
    if tick_count == 0 {
        return Ok(StepWorldProfiledDetailResponse {
            world_id,
            steps: 0,
            exec_feedback_ms: 0.0,
            exec_geology_terrain_ms: 0.0,
            exec_climate_ms: 0.0,
            exec_glaciology_ms: 0.0,
            exec_hydrology_ms: 0.0,
            exec_ecology_ms: 0.0,
            exec_society_ms: 0.0,
            exec_transition_ms: 0.0,
            step_sync_erosion_ms: 0.0,
            step_observe_world_change_ms: 0.0,
            step_history_snapshot_ms: 0.0,
            step_geology_river_prepare_ms: 0.0,
            step_geology_river_automaton_ms: 0.0,
            step_geology_river_automaton_sink_ms: 0.0,
            step_geology_river_automaton_cell_ms: 0.0,
            step_geology_river_automaton_queue_ms: 0.0,
            step_geology_river_network_ms: 0.0,
            step_geology_river_sync_ms: 0.0,
            step_geology_river_fallback_ms: 0.0,
            river_network_rebuild_count: 0,
            river_fallback_count: 0,
            sink_rebuild_full_count: 0,
            sink_rebuild_partial_count: 0,
            sink_rebuild_skipped_count: 0,
            sink_rebuild_fallback_full_count: 0,
            step_geology_river_sink_incremental_rebuild_ms: 0.0,
            step_geology_river_sink_full_rebuild_ms: 0.0,
            sink_affected_ratio: 0.0,
            sink_validation_fail_count: 0,
        });
    }
    let (managed, archive) = service
        .world_and_archive_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;
    reset_pending_slice(managed);

    let steps = scaled_step_count(managed.simulation_rate, tick_count);
    let mut sim_breakdown = ExecWorldBreakdownDetailed::default();
    let mut step_sync_erosion_ms = 0.0;
    let mut step_observe_world_change_ms = 0.0;
    let mut step_history_snapshot_ms = 0.0;

    for _ in 0..steps {
        archive.apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
        let step_breakdown =
            managed.with_exec_states(exec_world_profiled_detailed_with_feedback_and_states);
        sim_breakdown.accumulate(&step_breakdown);
        let profile = run_post_step_profiled(managed, archive);
        step_sync_erosion_ms += profile.step_sync_erosion_ms;
        step_observe_world_change_ms += profile.step_observe_world_change_ms;
        step_history_snapshot_ms += profile.step_history_snapshot_ms;
    }

    Ok(StepWorldProfiledDetailResponse {
        world_id,
        steps,
        exec_feedback_ms: sim_breakdown.breakdown.exec_feedback_ms,
        exec_geology_terrain_ms: sim_breakdown.breakdown.exec_geology_terrain_ms,
        exec_climate_ms: sim_breakdown.breakdown.exec_climate_ms,
        exec_glaciology_ms: sim_breakdown.breakdown.exec_glaciology_ms,
        exec_hydrology_ms: sim_breakdown.breakdown.exec_hydrology_ms,
        exec_ecology_ms: sim_breakdown.breakdown.exec_ecology_ms,
        exec_society_ms: sim_breakdown.breakdown.exec_society_ms,
        exec_transition_ms: sim_breakdown.breakdown.exec_transition_ms,
        step_sync_erosion_ms,
        step_observe_world_change_ms,
        step_history_snapshot_ms,
        step_geology_river_prepare_ms: sim_breakdown.river.step_geology_river_prepare_ms,
        step_geology_river_automaton_ms: sim_breakdown.river.step_geology_river_automaton_ms,
        step_geology_river_automaton_sink_ms: sim_breakdown
            .river
            .step_geology_river_automaton_sink_ms,
        step_geology_river_automaton_cell_ms: sim_breakdown
            .river
            .step_geology_river_automaton_cell_ms,
        step_geology_river_automaton_queue_ms: sim_breakdown
            .river
            .step_geology_river_automaton_queue_ms,
        step_geology_river_network_ms: sim_breakdown.river.step_geology_river_network_ms,
        step_geology_river_sync_ms: sim_breakdown.river.step_geology_river_sync_ms,
        step_geology_river_fallback_ms: sim_breakdown.river.step_geology_river_fallback_ms,
        river_network_rebuild_count: sim_breakdown.river.river_network_rebuild_count,
        river_fallback_count: sim_breakdown.river.river_fallback_count,
        sink_rebuild_full_count: sim_breakdown.river.sink_rebuild_full_count,
        sink_rebuild_partial_count: sim_breakdown.river.sink_rebuild_partial_count,
        sink_rebuild_skipped_count: sim_breakdown.river.sink_rebuild_skipped_count,
        sink_rebuild_fallback_full_count: sim_breakdown.river.sink_rebuild_fallback_full_count,
        step_geology_river_sink_incremental_rebuild_ms: sim_breakdown
            .river
            .step_geology_river_sink_incremental_rebuild_ms,
        step_geology_river_sink_full_rebuild_ms: sim_breakdown
            .river
            .step_geology_river_sink_full_rebuild_ms,
        sink_affected_ratio: sim_breakdown.river.sink_affected_ratio,
        sink_validation_fail_count: sim_breakdown.river.sink_validation_fail_count,
    })
}

pub(crate) fn exec_world_slice(
    service: &mut WorldService,
    world_id: String,
    work_budget: u32,
) -> Result<ExecWorldSliceResponse, String> {
    let (managed, archive) = service
        .world_and_archive_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;

    let budget = work_budget.max(1);
    if !managed.exec_is_busy() {
        managed.exec_state.remaining_steps = scaled_step_count(managed.simulation_rate, 1);
        managed.exec_state.next_phase = first_phase();
    }

    let mut remaining_budget = budget;
    let mut processed_ticks = 0u32;
    while remaining_budget > 0 && managed.exec_is_busy() {
        let next_phase = managed.exec_state.next_phase;
        archive.apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
        let slice = managed.with_exec_states(|world, feedback, geology_state, hydrology_state| {
            exec_world_slice_with_states(
                world,
                feedback,
                geology_state,
                hydrology_state,
                next_phase,
                remaining_budget,
            )
        });
        managed.exec_state.next_phase = slice.next_phase;
        remaining_budget = remaining_budget.saturating_sub(slice.work_units_consumed);
        if slice.ticks_completed > 0 {
            run_post_step(managed, archive);
            managed.exec_state.remaining_steps = managed
                .exec_state
                .remaining_steps
                .saturating_sub(slice.ticks_completed);
            processed_ticks = processed_ticks.saturating_add(slice.ticks_completed);
        }
        if slice.work_units_consumed == 0 {
            break;
        }
    }

    let phase = exec_phase_label(managed.exec_state.next_phase).to_string();
    Ok(ExecWorldSliceResponse {
        world_id,
        processed_ticks,
        busy: managed.exec_is_busy(),
        phase,
        tick: managed.world.clock.tick as f64,
    })
}

pub(crate) fn set_simulation_rate(
    service: &mut WorldService,
    world_id: &str,
    rate: f32,
) -> Result<(), String> {
    if !rate.is_finite() {
        return Err("rate must be finite".to_string());
    }
    let (managed, archive) = service
        .world_and_archive_mut(world_id)
        .ok_or_else(|| world_not_found_error(world_id))?;
    let _ = archive.enqueue_intervention(
        managed,
        InterventionCommand::SetSimulationRate { value: rate },
    );
    Ok(())
}

pub(crate) fn set_target_sea_ratio(
    service: &mut WorldService,
    world_id: &str,
    target_sea_ratio: f32,
) -> Result<(), String> {
    if !target_sea_ratio.is_finite() {
        return Err("target_sea_ratio must be finite".to_string());
    }
    let (managed, archive) = service
        .world_and_archive_mut(world_id)
        .ok_or_else(|| world_not_found_error(world_id))?;
    let _ = archive.enqueue_intervention(
        managed,
        InterventionCommand::SetTargetSeaRatio {
            value: target_sea_ratio,
        },
    );
    Ok(())
}

pub(crate) fn replay_world_to_tick(
    managed: &mut ManagedWorld,
    archive: &mut WorldArchive,
    target_tick: u64,
) -> Result<(), String> {
    let checkpoint = archive
        .history
        .range(..=target_tick)
        .next_back()
        .map(|(_, snapshot)| snapshot.clone())
        .ok_or_else(|| history_tick_not_available_error(target_tick))?;

    managed.world.apply_core(checkpoint.core);
    managed.world.refresh_terrain_state();
    managed.hydrology_dynamics = checkpoint.hydrology_dynamics;
    managed.geology_dynamics = checkpoint.geology_dynamics;
    managed.applied_intervention_seq = checkpoint.applied_intervention_seq;
    if managed.hydrology_dynamics.is_none() {
        sync_erosion_state(managed);
    }

    while managed.world.clock.tick < target_tick {
        archive.apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
        managed.with_exec_states(exec_world_with_feedback_and_states);
        run_post_step(managed, archive);
    }
    archive.apply_pending_interventions_for_tick(managed, target_tick);

    managed.transport_cache =
        WorldTransportCache::from_world(&managed.world, managed.geology_dynamics.as_ref());
    managed.refresh_reduced_metrics();
    managed.reset_exec_state();
    managed.observe_after_world_change();
    archive.insert_snapshot(managed.world.clock.tick, managed.snapshot_world());
    Ok(())
}

pub(crate) fn restore_world_to_tick(
    service: &mut WorldService,
    world_id: String,
    tick: u64,
) -> Result<RestoreWorldResult, String> {
    let (managed, archive) = service
        .world_and_archive_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;
    replay_world_to_tick(managed, archive, tick)?;
    Ok(RestoreWorldResult {
        world_id,
        tick: managed.world.clock.tick as f64,
    })
}

pub(crate) fn fork_world(
    service: &mut WorldService,
    world_id: String,
    tick: u64,
) -> Result<ForkWorldOutput, String> {
    let (mut forked_world, mut forked_archive) = service
        .cloned_world_and_archive(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;
    replay_world_to_tick(&mut forked_world, &mut forked_archive, tick)?;

    let forked_world_id = service.insert_world(forked_world, forked_archive);
    Ok(ForkWorldOutput {
        source_world_id: world_id,
        world_id: forked_world_id,
        tick: tick as f64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::world_dto::InitWorldConfig;
    use crate::application::world_query_use_cases;
    use verification_runtime::VerificationMode;

    fn default_init_config() -> InitWorldConfig {
        InitWorldConfig {
            geology_params: None,
            target_sea_ratio: None,
            simulation_rate: None,
            verification_mode: None,
        }
    }

    #[test]
    fn profiled_detail_exec_keeps_world_metrics_equivalent_to_normal_exec() {
        let mut normal_service = WorldService::new();
        let mut profiled_service = WorldService::new();

        let normal_world = init_world(
            &mut normal_service,
            "seed-profile-equivalence".to_string(),
            1,
            default_init_config(),
        )
        .expect("init normal world");
        let profiled_world = init_world(
            &mut profiled_service,
            "seed-profile-equivalence".to_string(),
            1,
            default_init_config(),
        )
        .expect("init profiled world");

        exec_world(&mut normal_service, &normal_world.world_id, 16).expect("exec normal world");
        for _ in 0..16 {
            let response = exec_world_profiled_detail(
                &mut profiled_service,
                profiled_world.world_id.clone(),
                1,
            )
            .expect("exec profiled detail");
            assert_eq!(response.steps, 1);
        }

        let normal_metrics =
            world_query_use_cases::get_metrics(&normal_service, normal_world.world_id.clone())
                .expect("metrics from normal world");
        let profiled_metrics =
            world_query_use_cases::get_metrics(&profiled_service, profiled_world.world_id.clone())
                .expect("metrics from profiled world");

        assert_eq!(normal_metrics.tick, 16.0);
        assert_eq!(profiled_metrics.tick, 16.0);
        assert_eq!(normal_metrics.land_cells, profiled_metrics.land_cells);
        assert_eq!(
            normal_metrics.max_river_flux,
            profiled_metrics.max_river_flux
        );
        assert_eq!(
            normal_metrics.top10_river_flux_sum,
            profiled_metrics.top10_river_flux_sum
        );
    }

    #[test]
    fn restore_world_to_tick_keeps_metrics_equivalent_to_direct_progress() {
        let mut restored_service = WorldService::new();
        let mut direct_service = WorldService::new();

        let restored_world = init_world(
            &mut restored_service,
            "seed-restore-equivalence".to_string(),
            1,
            default_init_config(),
        )
        .expect("init restored world");
        let direct_world = init_world(
            &mut direct_service,
            "seed-restore-equivalence".to_string(),
            1,
            default_init_config(),
        )
        .expect("init direct world");

        exec_world(&mut restored_service, &restored_world.world_id, 80)
            .expect("exec restored world to 80");
        restore_world_to_tick(&mut restored_service, restored_world.world_id.clone(), 65)
            .expect("restore world to tick 65");

        exec_world(&mut direct_service, &direct_world.world_id, 65)
            .expect("exec direct world to 65");

        let restored_metrics =
            world_query_use_cases::get_metrics(&restored_service, restored_world.world_id.clone())
                .expect("metrics from restored world");
        let direct_metrics =
            world_query_use_cases::get_metrics(&direct_service, direct_world.world_id.clone())
                .expect("metrics from direct world");

        assert_eq!(restored_metrics.tick, 65.0);
        assert_eq!(direct_metrics.tick, 65.0);
        assert_eq!(restored_metrics.land_cells, direct_metrics.land_cells);
        assert_eq!(
            restored_metrics.max_river_flux,
            direct_metrics.max_river_flux
        );
        assert_eq!(
            restored_metrics.top10_river_flux_sum,
            direct_metrics.top10_river_flux_sum
        );
    }

    #[test]
    fn headless_metrics_mode_skips_history_snapshots_on_exec() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-headless-history".to_string(),
            1,
            InitWorldConfig {
                geology_params: None,
                target_sea_ratio: None,
                simulation_rate: None,
                verification_mode: Some(VerificationMode::HeadlessMetrics),
            },
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 80).expect("exec world");
        let history = world_query_use_cases::list_history_ticks(&service, world.world_id.clone())
            .expect("history ticks");
        assert_eq!(history.ticks, vec![0.0]);
    }

    #[test]
    fn exec_world_slice_phase_does_not_return_post_step_label() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-slice-phase".to_string(),
            1,
            default_init_config(),
        )
        .expect("init world");

        loop {
            let response =
                exec_world_slice(&mut service, world.world_id.clone(), 1).expect("slice exec");
            if !response.busy {
                assert_ne!(response.phase, "post_step");
                break;
            }
        }
    }

    #[test]
    fn scientific_benchmark_mode_records_samples_during_exec() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-science-sample".to_string(),
            1,
            InitWorldConfig {
                geology_params: None,
                target_sea_ratio: None,
                simulation_rate: None,
                verification_mode: Some(VerificationMode::ScientificBenchmark),
            },
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 3).expect("exec world");
        let managed = service.world(&world.world_id).expect("managed world");
        assert!(!managed.scientific_benchmark_samples.is_empty());
    }

    #[test]
    fn scientific_benchmark_mode_records_samples_during_profiled_exec() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-science-profiled-sample".to_string(),
            1,
            InitWorldConfig {
                geology_params: None,
                target_sea_ratio: None,
                simulation_rate: None,
                verification_mode: Some(VerificationMode::ScientificBenchmark),
            },
        )
        .expect("init world");

        let response =
            exec_world_profiled(&mut service, world.world_id.clone(), 3).expect("profiled exec");
        assert_eq!(response.steps, 3);
        let managed = service.world(&world.world_id).expect("managed world");
        assert!(!managed.scientific_benchmark_samples.is_empty());
    }
}
