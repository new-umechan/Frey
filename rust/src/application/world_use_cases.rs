#![cfg(feature = "wasm_transport")]

use crate::application::world_dto::{
    ExecWorldSliceResponse, InitWorldConfig, InitWorldOutput, RewindWorldResult, SeekWorldResult,
    StepWorldProfiledDetailResponse, StepWorldProfiledResponse, TimelineAdvanceResult,
};
use crate::application::world_runtime::{
    InterventionCommand, ManagedWorld, ManagedWorldExecState, TimelineRetentionPolicy,
    TimelineRuntime, TimelineViewCache, TICK_BOUNDARY_COMPLETED_TICK,
};
use crate::application::world_service::WorldService;
use crate::application::world_support::{
    build_erosion_state, post_step_sync_light, sync_erosion_state,
};
use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::sim;
use crate::sim::geology_types::GeologyInternal;
use crate::sim::precomputed::{
    geology_fingerprint, PrecomputedWorldSnapshotEnvelope, SNAPSHOT_FORMAT_VERSION,
};
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

fn checkpoint_tick_not_available_error(tick: u64) -> String {
    format!("tick {tick} is not available in checkpoints")
}

fn undo_log_not_available_error(tick: u64) -> String {
    format!("tick {tick} is not available in undo logs")
}

fn scaled_step_count(simulation_rate: f32, tick_count: u32) -> u32 {
    let scaled_ticks = ((tick_count as f32) * simulation_rate).round() as u32;
    scaled_ticks.max(1)
}

fn reset_pending_slice(managed: &mut ManagedWorld) {
    managed.reset_exec_state();
}

fn ensure_next_tick_undo_log(managed: &ManagedWorld, timeline: &mut TimelineRuntime) {
    let next_tick = managed.world.clock.tick.saturating_add(1);
    timeline.begin_tick_undo_log(next_tick, managed.checkpoint_snapshot());
}

fn exec_next_completed_tick(managed: &mut ManagedWorld, timeline: &mut TimelineRuntime) {
    ensure_next_tick_undo_log(managed, timeline);
    timeline
        .archive_mut()
        .apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
    managed.with_exec_states(exec_world_with_feedback_and_states);
    run_post_step(managed, timeline);
}

fn advance_runtime_by_steps(
    managed: &mut ManagedWorld,
    timeline: &mut TimelineRuntime,
    steps: u64,
) {
    for _ in 0..steps {
        exec_next_completed_tick(managed, timeline);
    }
}

fn exec_phase_label(phase: ExecWorldPhase) -> &'static str {
    display_group_key(phase_display_group(phase))
}

fn apply_checkpoint_snapshot(
    managed: &mut ManagedWorld,
    checkpoint: crate::application::world_runtime::CheckpointSnapshot,
) {
    managed.world.apply_core(checkpoint.core);
    managed.world.refresh_terrain_state();
    managed.hydrology_dynamics = checkpoint.hydrology_dynamics;
    managed.geology_dynamics = checkpoint.geology_dynamics;
    managed.applied_intervention_seq = checkpoint.applied_intervention_seq;
    if managed.hydrology_dynamics.is_none() {
        sync_erosion_state(managed);
    }
}

fn apply_core_change_set(
    managed: &mut ManagedWorld,
    change_set: &crate::application::world_runtime::WorldCoreChangeSet,
) {
    if let Some(geology) = &change_set.geology {
        geology.apply_to(&mut managed.world.state.geology);
    }
    if let Some(climate) = &change_set.climate {
        climate.apply_to(&mut managed.world.state.climate);
    }
    if let Some(glaciology) = &change_set.glaciology {
        glaciology.apply_to(&mut managed.world.state.glaciology);
    }
    if let Some(hydrology) = &change_set.hydrology {
        hydrology.apply_to(&mut managed.world.state.hydrology);
    }
    if let Some(ecology) = &change_set.ecology {
        ecology.apply_to(&mut managed.world.state.ecology);
    }
    if let Some(domesticates) = &change_set.domesticates {
        domesticates.apply_to(&mut managed.world.state.domesticates);
    }
    if let Some(subsistence) = &change_set.subsistence {
        subsistence.apply_to(&mut managed.world.state.subsistence);
    }
    if let Some(population) = &change_set.population {
        population.apply_to(&mut managed.world.state.population);
    }
    if let Some(settlement) = &change_set.settlement {
        settlement.apply_to(&mut managed.world.state.settlement);
    }
    if let Some(polity) = &change_set.polity {
        polity.apply_to(&mut managed.world.state.polity);
    }
    if let Some(conflict) = &change_set.conflict {
        conflict.apply_to(&mut managed.world.state.conflict);
    }
    if let Some(entities) = &change_set.entities {
        entities.apply_to(&mut managed.world.entities);
    }
    if let Some(relations) = &change_set.relations {
        relations.apply_to(&mut managed.world.relations);
    }
    if let Some(clock) = &change_set.clock {
        clock.apply_to(&mut managed.world.clock);
    }
    if let Some(control) = &change_set.control {
        control.apply_to(&mut managed.world.control);
    }
    managed.world.refresh_terrain_state();
}

struct WorldPostStepRuntime<'a> {
    managed: &'a mut ManagedWorld,
    timeline: &'a mut TimelineRuntime,
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
        let retention = self.timeline.retention.clone();
        self.timeline
            .archive_mut()
            .save_checkpoint_if_needed(self.managed, &retention);
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

fn run_post_step(managed: &mut ManagedWorld, timeline: &mut TimelineRuntime) {
    let mut runtime = WorldPostStepRuntime { managed, timeline };
    run_post_step_runtime(&mut runtime);
    let tick = managed.world.clock.tick;
    timeline.finalize_tick_undo_log(tick, managed);
}

fn run_post_step_profiled(
    managed: &mut ManagedWorld,
    timeline: &mut TimelineRuntime,
) -> PostStepProfile {
    let mut runtime = WorldPostStepRuntime { managed, timeline };
    let profile = run_post_step_profiled_runtime(&mut runtime, &DefaultProfileClock);
    let tick = managed.world.clock.tick;
    timeline.finalize_tick_undo_log(tick, managed);
    profile
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
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); positions.len()],
        boundary_condition: vec![0.0; positions.len()],
        smoothing_limited_cells_ratio: 0.0,
        mean_smoothing_factor: 1.0,
        zero_mean_adjusted_cells_ratio: 0.0,
        zero_mean_mean_abs_correction: 0.0,
        zero_mean_std_delta: 0.0,
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
    sim_world.control.geology_params = geology_params.clone();
    sim_world.control.erosion_thickness_coupling = geology_params.erosion_thickness_coupling;
    sim_world.control.deposition_thickness_coupling = geology_params.deposition_thickness_coupling;
    sim_world.clock.epoch = world::EraKind::Crust;

    let erosion_state = build_erosion_state(&sim_world, geology_params.clone());
    let _ = crate::sim::hydrology::apply_hydrology_state_view(&mut sim_world, &erosion_state);
    let geology_dynamics = sim_world.exec_scratch.geology_dynamics.take();
    let transport_cache = TimelineViewCache::from_world(&sim_world, geology_dynamics.as_ref());
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

    let mut timeline = TimelineRuntime::new(TimelineRetentionPolicy::from_config(
        config.timeline.as_ref(),
    ));
    timeline
        .archive_mut()
        .insert_checkpoint(managed.world.clock.tick, managed.checkpoint_snapshot());
    timeline.observe_tick(managed.world.clock.tick);

    let tick = managed.world.clock.tick as f64;
    let era = managed.world.clock.epoch.as_key().to_string();
    let cell_count = managed.world.state.geology.height.len() as u32;
    let world_id = service.insert_world(managed, timeline);

    Ok(InitWorldOutput {
        world_id,
        tick,
        head_tick: tick,
        era,
        cell_count,
    })
}

pub(crate) fn init_world_from_snapshot_bytes(
    service: &mut WorldService,
    seed: String,
    mesh_level: u32,
    config: InitWorldConfig,
    snapshot_bytes: &[u8],
) -> Result<InitWorldOutput, String> {
    if mesh_level > 8 {
        return Err("mesh_level must be between 0 and 8".to_string());
    }

    let (envelope, _): (PrecomputedWorldSnapshotEnvelope, usize) =
        bincode::serde::decode_from_slice(snapshot_bytes, bincode::config::standard())
            .map_err(|err| format!("failed to decode snapshot bytes: {err}"))?;
    if envelope.format_version != SNAPSHOT_FORMAT_VERSION {
        return Err(format!(
            "snapshot format mismatch: expected={}, actual={}",
            SNAPSHOT_FORMAT_VERSION, envelope.format_version
        ));
    }
    if envelope.seed != seed {
        return Err(format!(
            "snapshot seed mismatch: expected={}, actual={}",
            seed, envelope.seed
        ));
    }
    if envelope.mesh_level != mesh_level {
        return Err(format!(
            "snapshot mesh level mismatch: expected={}, actual={}",
            mesh_level, envelope.mesh_level
        ));
    }

    let mut geology_params = config.geology_params.unwrap_or_default();
    geology_params.level = mesh_level;
    let fingerprint = geology_fingerprint(&geology_params)?;
    if envelope.geology_fingerprint != fingerprint {
        return Err("snapshot geology fingerprint mismatch".to_string());
    }

    let (positions, indices) = generate_icosphere(mesh_level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };
    let geology = envelope.world_core.cells.geology.clone();
    let mut sim_world = world::World::new(mesh, geology);
    sim_world.apply_core(envelope.world_core);
    sim_world.exec_scratch.geology_dynamics = envelope.geology_dynamics_state.clone();
    sim_world.control.geology_params = geology_params.clone();
    sim_world.control.erosion_thickness_coupling = geology_params.erosion_thickness_coupling;
    sim_world.control.deposition_thickness_coupling = geology_params.deposition_thickness_coupling;
    let transport_cache =
        TimelineViewCache::from_world(&sim_world, sim_world.exec_scratch.geology_dynamics.as_ref());

    let cell_count = sim_world.cell_count();
    let mut managed = ManagedWorld {
        world: sim_world,
        hydrology_dynamics: Some(envelope.hydrology_state),
        geology_dynamics: envelope.geology_dynamics_state,
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
        applied_intervention_seq: envelope.applied_intervention_seq,
    };
    if let Some(hydrology_state) = managed.hydrology_dynamics.as_ref() {
        crate::sim::hydrology::apply_hydrology_state_view(&mut managed.world, hydrology_state)?;
    }
    managed.transport_cache =
        TimelineViewCache::from_world(&managed.world, managed.geology_dynamics.as_ref());
    managed.refresh_reduced_metrics();

    let mut timeline = TimelineRuntime::new(TimelineRetentionPolicy::from_config(
        config.timeline.as_ref(),
    ));
    timeline
        .archive_mut()
        .insert_checkpoint(managed.world.clock.tick, managed.checkpoint_snapshot());
    timeline.observe_tick(managed.world.clock.tick);

    let tick = managed.world.clock.tick as f64;
    let era = managed.world.clock.epoch.as_key().to_string();
    let cell_count = managed.world.state.geology.height.len() as u32;
    let world_id = service.insert_world(managed, timeline);

    Ok(InitWorldOutput {
        world_id,
        tick,
        head_tick: tick,
        era,
        cell_count,
    })
}

pub(crate) fn exec_world(
    service: &mut WorldService,
    world_id: &str,
    tick_count: u32,
) -> Result<(), String> {
    let _ = advance_timeline(service, world_id.to_string(), tick_count)?;
    Ok(())
}

pub(crate) fn advance_timeline(
    service: &mut WorldService,
    world_id: String,
    tick_count: u32,
) -> Result<TimelineAdvanceResult, String> {
    if tick_count == 0 {
        let (_, timeline) = service
            .world_and_timeline_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        return Ok(TimelineAdvanceResult {
            world_id,
            tick: timeline.current_tick() as f64,
            head_tick: timeline.head_tick() as f64,
            advanced_ticks: 0,
        });
    }
    let (managed, timeline) = service
        .world_and_timeline_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;
    reset_pending_slice(managed);

    let steps = scaled_step_count(managed.simulation_rate, tick_count);
    let target_tick = managed.world.clock.tick.saturating_add(steps as u64);

    if target_tick <= timeline.head_tick() {
        seek_world_to_tick_internal(managed, timeline, target_tick)?;
    } else {
        if managed.world.clock.tick < timeline.head_tick() {
            seek_world_to_tick_internal(managed, timeline, timeline.head_tick())?;
        }
        let remaining_steps = target_tick.saturating_sub(managed.world.clock.tick);
        advance_runtime_by_steps(managed, timeline, remaining_steps);
    }
    Ok(TimelineAdvanceResult {
        world_id,
        tick: managed.world.clock.tick as f64,
        head_tick: timeline.head_tick() as f64,
        advanced_ticks: steps,
    })
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

    let (managed, timeline) = service
        .world_and_timeline_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;

    let steps = exec_profiled_loop(
        managed,
        timeline,
        scaled_step_count(managed.simulation_rate, tick_count),
    );

    let breakdown = &steps.sim_breakdown;
    Ok(StepWorldProfiledResponse {
        world_id,
        steps: steps.ticks,
        exec_feedback_ms: breakdown.exec_feedback_ms,
        exec_geology_terrain_ms: breakdown.exec_geology_terrain_ms,
        exec_climate_ms: breakdown.exec_climate_ms,
        exec_glaciology_ms: breakdown.exec_glaciology_ms,
        exec_hydrology_ms: breakdown.exec_hydrology_ms,
        exec_ecology_ms: breakdown.exec_ecology_ms,
        exec_society_ms: breakdown.exec_society_ms,
        exec_transition_ms: breakdown.exec_transition_ms,
        step_sync_erosion_ms: steps.step_sync_erosion_ms,
        step_observe_world_change_ms: steps.step_observe_world_change_ms,
        step_history_snapshot_ms: steps.step_history_snapshot_ms,
    })
}

struct ProfiledStepsResult {
    ticks: u32,
    sim_breakdown: ExecWorldBreakdown,
    step_sync_erosion_ms: f64,
    step_observe_world_change_ms: f64,
    step_history_snapshot_ms: f64,
}

fn exec_profiled_loop(
    managed: &mut ManagedWorld,
    timeline: &mut TimelineRuntime,
    steps: u32,
) -> ProfiledStepsResult {
    reset_pending_slice(managed);
    let mut sim_breakdown = ExecWorldBreakdown::default();
    let mut step_sync_erosion_ms = 0.0;
    let mut step_observe_world_change_ms = 0.0;
    let mut step_history_snapshot_ms = 0.0;

    for _ in 0..steps {
        ensure_next_tick_undo_log(managed, timeline);
        timeline
            .archive_mut()
            .apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
        let step_breakdown = managed
            .with_exec_states(exec_world_profiled_detailed_with_feedback_and_states)
            .breakdown;
        sim_breakdown.accumulate(&step_breakdown);
        let profile = run_post_step_profiled(managed, timeline);
        step_sync_erosion_ms += profile.step_sync_erosion_ms;
        step_observe_world_change_ms += profile.step_observe_world_change_ms;
        step_history_snapshot_ms += profile.step_history_snapshot_ms;
    }

    ProfiledStepsResult {
        ticks: steps,
        sim_breakdown,
        step_sync_erosion_ms,
        step_observe_world_change_ms,
        step_history_snapshot_ms,
    }
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

    let (managed, timeline) = service
        .world_and_timeline_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;

    let steps = scaled_step_count(managed.simulation_rate, tick_count);
    reset_pending_slice(managed);

    let mut sim_breakdown = ExecWorldBreakdownDetailed::default();
    let mut step_sync_erosion_ms = 0.0;
    let mut step_observe_world_change_ms = 0.0;
    let mut step_history_snapshot_ms = 0.0;

    for _ in 0..steps {
        ensure_next_tick_undo_log(managed, timeline);
        timeline
            .archive_mut()
            .apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
        let step_breakdown =
            managed.with_exec_states(exec_world_profiled_detailed_with_feedback_and_states);
        sim_breakdown.accumulate(&step_breakdown);
        let profile = run_post_step_profiled(managed, timeline);
        step_sync_erosion_ms += profile.step_sync_erosion_ms;
        step_observe_world_change_ms += profile.step_observe_world_change_ms;
        step_history_snapshot_ms += profile.step_history_snapshot_ms;
    }

    let breakdown = &sim_breakdown.breakdown;
    let river = &sim_breakdown.river;
    Ok(StepWorldProfiledDetailResponse {
        world_id,
        steps,
        exec_feedback_ms: breakdown.exec_feedback_ms,
        exec_geology_terrain_ms: breakdown.exec_geology_terrain_ms,
        exec_climate_ms: breakdown.exec_climate_ms,
        exec_glaciology_ms: breakdown.exec_glaciology_ms,
        exec_hydrology_ms: breakdown.exec_hydrology_ms,
        exec_ecology_ms: breakdown.exec_ecology_ms,
        exec_society_ms: breakdown.exec_society_ms,
        exec_transition_ms: breakdown.exec_transition_ms,
        step_sync_erosion_ms,
        step_observe_world_change_ms,
        step_history_snapshot_ms,
        step_geology_river_prepare_ms: river.step_geology_river_prepare_ms,
        step_geology_river_automaton_ms: river.step_geology_river_automaton_ms,
        step_geology_river_automaton_sink_ms: river.step_geology_river_automaton_sink_ms,
        step_geology_river_automaton_cell_ms: river.step_geology_river_automaton_cell_ms,
        step_geology_river_automaton_queue_ms: river.step_geology_river_automaton_queue_ms,
        step_geology_river_network_ms: river.step_geology_river_network_ms,
        step_geology_river_sync_ms: river.step_geology_river_sync_ms,
        step_geology_river_fallback_ms: river.step_geology_river_fallback_ms,
        river_network_rebuild_count: river.river_network_rebuild_count,
        river_fallback_count: river.river_fallback_count,
        sink_rebuild_full_count: river.sink_rebuild_full_count,
        sink_rebuild_partial_count: river.sink_rebuild_partial_count,
        sink_rebuild_skipped_count: river.sink_rebuild_skipped_count,
        sink_rebuild_fallback_full_count: river.sink_rebuild_fallback_full_count,
        step_geology_river_sink_incremental_rebuild_ms: river
            .step_geology_river_sink_incremental_rebuild_ms,
        step_geology_river_sink_full_rebuild_ms: river.step_geology_river_sink_full_rebuild_ms,
        sink_affected_ratio: river.sink_affected_ratio,
        sink_validation_fail_count: river.sink_validation_fail_count,
    })
}

pub(crate) fn exec_world_slice(
    service: &mut WorldService,
    world_id: String,
    work_budget: u32,
) -> Result<ExecWorldSliceResponse, String> {
    let (managed, timeline) = service
        .world_and_timeline_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;

    let budget = work_budget.max(1);
    if !managed.exec_is_busy() {
        managed.exec_state.remaining_steps = scaled_step_count(managed.simulation_rate, 1);
        managed.exec_state.next_phase = first_phase();
        ensure_next_tick_undo_log(managed, timeline);
    }

    let mut remaining_budget = budget;
    let mut processed_ticks = 0u32;
    while remaining_budget > 0 && managed.exec_is_busy() {
        let next_phase = managed.exec_state.next_phase;
        timeline
            .archive_mut()
            .apply_pending_interventions_for_tick(managed, managed.world.clock.tick);
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
            run_post_step(managed, timeline);
            managed.exec_state.remaining_steps = managed
                .exec_state
                .remaining_steps
                .saturating_sub(slice.ticks_completed);
            processed_ticks = processed_ticks.saturating_add(slice.ticks_completed);
            if managed.exec_is_busy() {
                ensure_next_tick_undo_log(managed, timeline);
            }
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
        head_tick: timeline.head_tick() as f64,
        tick_boundary: TICK_BOUNDARY_COMPLETED_TICK.to_string(),
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
    let (managed, timeline) = service
        .world_and_timeline_mut(world_id)
        .ok_or_else(|| world_not_found_error(world_id))?;
    let _ = timeline.archive_mut().enqueue_intervention(
        managed,
        InterventionCommand::SetSimulationRate { value: rate },
    );
    Ok(())
}

pub(crate) fn seek_world_to_tick_internal(
    managed: &mut ManagedWorld,
    timeline: &mut TimelineRuntime,
    target_tick: u64,
) -> Result<(), String> {
    let replay_target_tick = target_tick.min(timeline.head_tick());
    let checkpoint = timeline
        .archive()
        .checkpoints
        .range(..=replay_target_tick)
        .next_back()
        .map(|(_, snapshot)| snapshot.clone())
        .ok_or_else(|| checkpoint_tick_not_available_error(replay_target_tick))?;

    apply_checkpoint_snapshot(managed, checkpoint);

    let replay_steps = replay_target_tick.saturating_sub(managed.world.clock.tick);
    advance_runtime_by_steps(managed, timeline, replay_steps);

    if target_tick > replay_target_tick {
        let extension_steps = target_tick.saturating_sub(replay_target_tick);
        advance_runtime_by_steps(managed, timeline, extension_steps);
    }
    timeline
        .archive_mut()
        .apply_pending_interventions_for_tick(managed, target_tick);

    managed.transport_cache =
        TimelineViewCache::from_world(&managed.world, managed.geology_dynamics.as_ref());
    managed.refresh_reduced_metrics();
    managed.reset_exec_state();
    managed.observe_after_world_change();
    timeline.observe_tick(managed.world.clock.tick);
    timeline
        .archive_mut()
        .insert_checkpoint(managed.world.clock.tick, managed.checkpoint_snapshot());
    Ok(())
}

pub(crate) fn rewind_world_by_ticks(
    service: &mut WorldService,
    world_id: String,
    tick_count: u32,
) -> Result<RewindWorldResult, String> {
    let (managed, timeline) = service
        .world_and_timeline_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;

    if tick_count == 0 {
        return Ok(RewindWorldResult {
            world_id,
            tick: managed.world.clock.tick as f64,
            head_tick: timeline.head_tick() as f64,
            rewound_ticks: 0,
        });
    }

    let current_tick = managed.world.clock.tick;
    let target_tick = current_tick.saturating_sub(tick_count as u64);
    let can_use_undo_logs =
        ((target_tick + 1)..=current_tick).all(|tick| timeline.undo_logs.contains_key(&tick));

    if can_use_undo_logs {
        while managed.world.clock.tick > target_tick {
            let tick = managed.world.clock.tick;
            let undo_log = timeline
                .undo_logs
                .get(&tick)
                .cloned()
                .ok_or_else(|| undo_log_not_available_error(tick))?;
            apply_core_change_set(managed, &undo_log.core_change_set);
            if let Some(hydrology_dynamics_before) = undo_log.hydrology_dynamics_before {
                managed.hydrology_dynamics = hydrology_dynamics_before;
            }
            if let Some(geology_dynamics_before) = undo_log.geology_dynamics_before {
                managed.geology_dynamics = geology_dynamics_before;
            }
            if let Some(applied_intervention_seq_before) = undo_log.applied_intervention_seq_before
            {
                managed.applied_intervention_seq = applied_intervention_seq_before;
            }
            if managed.hydrology_dynamics.is_none() {
                sync_erosion_state(managed);
            }
        }
        managed.transport_cache =
            TimelineViewCache::from_world(&managed.world, managed.geology_dynamics.as_ref());
        managed.refresh_reduced_metrics();
        managed.reset_exec_state();
        managed.observe_after_world_change();
        timeline.observe_tick(managed.world.clock.tick);
    } else {
        seek_world_to_tick_internal(managed, timeline, target_tick)?;
    }

    Ok(RewindWorldResult {
        world_id,
        tick: managed.world.clock.tick as f64,
        head_tick: timeline.head_tick() as f64,
        rewound_ticks: (current_tick.saturating_sub(managed.world.clock.tick)) as u32,
    })
}

pub(crate) fn seek_world_to_tick(
    service: &mut WorldService,
    world_id: String,
    tick: u64,
) -> Result<SeekWorldResult, String> {
    let (managed, timeline) = service
        .world_and_timeline_mut(&world_id)
        .ok_or_else(|| world_not_found_error(&world_id))?;
    seek_world_to_tick_internal(managed, timeline, tick)?;
    Ok(SeekWorldResult {
        world_id,
        tick: managed.world.clock.tick as f64,
        head_tick: timeline.head_tick() as f64,
    })
}

#[allow(dead_code)]
pub(crate) fn restore_world_to_tick(
    service: &mut WorldService,
    world_id: String,
    tick: u64,
) -> Result<SeekWorldResult, String> {
    seek_world_to_tick(service, world_id, tick)
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
            simulation_rate: None,
            verification_mode: None,
            timeline: None,
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
    fn rewind_world_by_ticks_keeps_metrics_equivalent_to_direct_progress() {
        let mut rewound_service = WorldService::new();
        let mut direct_service = WorldService::new();

        let rewound_world = init_world(
            &mut rewound_service,
            "seed-rewind-equivalence".to_string(),
            1,
            default_init_config(),
        )
        .expect("init rewound world");
        let direct_world = init_world(
            &mut direct_service,
            "seed-rewind-equivalence".to_string(),
            1,
            default_init_config(),
        )
        .expect("init direct world");

        exec_world(&mut rewound_service, &rewound_world.world_id, 12)
            .expect("exec rewound world to 12");
        rewind_world_by_ticks(&mut rewound_service, rewound_world.world_id.clone(), 5)
            .expect("rewind world by 5 ticks");

        exec_world(&mut direct_service, &direct_world.world_id, 7).expect("exec direct world to 7");

        let rewound_metrics =
            world_query_use_cases::get_metrics(&rewound_service, rewound_world.world_id.clone())
                .expect("metrics from rewound world");
        let direct_metrics =
            world_query_use_cases::get_metrics(&direct_service, direct_world.world_id.clone())
                .expect("metrics from direct world");

        assert_eq!(rewound_metrics.tick, 7.0);
        assert_eq!(direct_metrics.tick, 7.0);
        assert_eq!(rewound_metrics.land_cells, direct_metrics.land_cells);
        assert_eq!(
            rewound_metrics.max_river_flux,
            direct_metrics.max_river_flux
        );
        assert_eq!(
            rewound_metrics.top10_river_flux_sum,
            direct_metrics.top10_river_flux_sum
        );
    }

    #[test]
    fn exec_world_records_tick_undo_logs() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-undo-log".to_string(),
            1,
            default_init_config(),
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 3).expect("exec world");
        let timeline = service.timeline(&world.world_id).expect("timeline runtime");
        assert!(timeline.undo_logs.contains_key(&1));
        assert!(timeline.undo_logs.contains_key(&2));
        assert!(timeline.undo_logs.contains_key(&3));
        let log = timeline.undo_logs.get(&1).expect("tick 1 undo log");
        assert!(log.pending_snapshot_before_tick.is_none());
        assert!(
            !log.changed_fields.is_empty()
                || log.core_change_set.clock.is_some()
                || log.core_change_set.geology.is_some()
        );
    }

    #[test]
    fn rewind_preserves_head_tick_and_future_history() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-rewind-history-retain".to_string(),
            1,
            default_init_config(),
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 12).expect("exec world");
        rewind_world_by_ticks(&mut service, world.world_id.clone(), 5).expect("rewind world");

        let timeline = service.timeline(&world.world_id).expect("timeline runtime");
        assert_eq!(timeline.current_tick(), 7);
        assert_eq!(timeline.head_tick(), 12);
        assert!(timeline.undo_logs.contains_key(&12));
    }

    #[test]
    fn retention_budget_preserves_checkpoint_anchors() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-retention-anchor".to_string(),
            1,
            InitWorldConfig {
                geology_params: None,
                simulation_rate: None,
                verification_mode: None,
                timeline: Some(crate::application::world_dto::TimelineConfig {
                    checkpoint_interval: Some(1),
                    checkpoint_limit: Some(32),
                    undo_log_limit: Some(32),
                    undo_future_prune_grace_ticks: None,
                    max_estimated_bytes: Some(1),
                }),
            },
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 4).expect("exec world");
        let timeline = service.timeline(&world.world_id).expect("timeline runtime");
        let checkpoint_ticks = timeline
            .archive
            .checkpoints
            .keys()
            .copied()
            .collect::<Vec<_>>();
        assert!(checkpoint_ticks.contains(&0));
        assert!(checkpoint_ticks.contains(&timeline.head_tick()));
        assert!(timeline.undo_logs.len() <= 1);
    }

    #[test]
    fn retention_budget_keeps_seek_possible_after_pruning() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-retention-seekable".to_string(),
            1,
            InitWorldConfig {
                geology_params: None,
                simulation_rate: None,
                verification_mode: None,
                timeline: Some(crate::application::world_dto::TimelineConfig {
                    checkpoint_interval: Some(1),
                    checkpoint_limit: Some(32),
                    undo_log_limit: Some(32),
                    undo_future_prune_grace_ticks: None,
                    max_estimated_bytes: Some(1),
                }),
            },
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 6).expect("exec world");
        seek_world_to_tick(&mut service, world.world_id.clone(), 0).expect("seek to origin");
        let timeline = service.timeline(&world.world_id).expect("timeline runtime");
        assert_eq!(timeline.current_tick(), 0);
        assert_eq!(timeline.head_tick(), 6);
    }

    #[test]
    fn undo_log_can_store_sparse_selected_field_patches() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-sparse-undo".to_string(),
            1,
            default_init_config(),
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 1).expect("exec world");
        let timeline = service.timeline(&world.world_id).expect("timeline runtime");
        let log = timeline.undo_logs.get(&1).expect("tick 1 undo log");

        let has_sparse_geology = log
            .core_change_set
            .geology
            .as_ref()
            .map(|geology| geology.full.is_none() && geology.height.is_some())
            .unwrap_or(false);
        let has_sparse_climate = log
            .core_change_set
            .climate
            .as_ref()
            .map(|climate| {
                climate.full.is_none()
                    && (climate.temperature.is_some()
                        || climate.precipitation.is_some()
                        || climate.evapotranspiration.is_some()
                        || climate.runoff.is_some()
                        || climate.aridity.is_some()
                        || climate.ocean_temperature.is_some()
                        || climate.precipitable_water.is_some()
                        || climate.cloud_water.is_some()
                        || climate.wind_u.is_some()
                        || climate.wind_v.is_some()
                        || climate.moisture_flux_u.is_some()
                        || climate.moisture_flux_v.is_some())
            })
            .unwrap_or(false);
        let has_sparse_glaciology = log
            .core_change_set
            .glaciology
            .as_ref()
            .map(|glaciology| {
                glaciology.full.is_none()
                    && (glaciology.ice_thickness.is_some()
                        || glaciology.ice_load.is_some()
                        || glaciology.accumulation.is_some()
                        || glaciology.ablation.is_some()
                        || glaciology.isostatic_adjustment.is_some()
                        || glaciology.applied_isostatic_adjustment.is_some()
                        || glaciology.glacial_erosion_rate.is_some()
                        || glaciology.glacial_melt_runoff.is_some())
            })
            .unwrap_or(false);
        let has_sparse_hydrology = log
            .core_change_set
            .hydrology
            .as_ref()
            .map(|hydrology| {
                hydrology.full.is_none()
                    && (hydrology.river_flow.is_some()
                        || hydrology.river_next.is_some()
                        || hydrology.erosion_rate.is_some()
                        || hydrology.deposition_rate.is_some()
                        || hydrology.is_lake.is_some()
                        || hydrology.sink_id.is_some()
                        || hydrology.sink_route_next.is_some()
                        || hydrology.sink_member_offsets.is_some()
                        || hydrology.sink_member_cells.is_some()
                        || hydrology.sink_spill_cell.is_some()
                        || hydrology.sink_spill_to.is_some()
                        || hydrology.sink_spill_level.is_some()
                        || hydrology.sink_capacity_total.is_some()
                        || hydrology.sink_capacity_remaining.is_some()
                        || hydrology.sink_storage_water.is_some()
                        || hydrology.sink_storage_sediment.is_some()
                        || hydrology.sink_overflow_active.is_some())
            })
            .unwrap_or(false);
        let has_sparse_ecology = log
            .core_change_set
            .ecology
            .as_ref()
            .map(|ecology| {
                ecology.full.is_none()
                    && (ecology.biome.is_some()
                        || ecology.tree_cover.is_some()
                        || ecology.ground_cover.is_some()
                        || ecology.disturbance.is_some()
                        || ecology.soil_fertility.is_some())
            })
            .unwrap_or(false);
        let has_full_geology = log
            .core_change_set
            .geology
            .as_ref()
            .map(|geology| geology.full.is_some())
            .unwrap_or(false);
        let has_full_climate = log
            .core_change_set
            .climate
            .as_ref()
            .map(|climate| climate.full.is_some())
            .unwrap_or(false);
        let has_full_glaciology = log
            .core_change_set
            .glaciology
            .as_ref()
            .map(|glaciology| glaciology.full.is_some())
            .unwrap_or(false);
        let has_full_hydrology = log
            .core_change_set
            .hydrology
            .as_ref()
            .map(|hydrology| hydrology.full.is_some())
            .unwrap_or(false);
        let has_full_ecology = log
            .core_change_set
            .ecology
            .as_ref()
            .map(|ecology| ecology.full.is_some())
            .unwrap_or(false);

        if log.core_change_set.geology.is_some() {
            assert!(has_sparse_geology || has_full_geology);
        }
        if log.core_change_set.climate.is_some() {
            assert!(has_sparse_climate || has_full_climate);
        }
        if log.core_change_set.glaciology.is_some() {
            assert!(has_sparse_glaciology || has_full_glaciology);
        }
        if log.core_change_set.hydrology.is_some() {
            assert!(has_sparse_hydrology || has_full_hydrology);
        }
        if log.core_change_set.ecology.is_some() {
            assert!(has_sparse_ecology || has_full_ecology);
        }
    }

    #[test]
    fn seek_preserves_head_tick_and_future_history() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-seek-history-retain".to_string(),
            1,
            default_init_config(),
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 80).expect("exec world");
        seek_world_to_tick(&mut service, world.world_id.clone(), 10).expect("seek world");

        let timeline = service.timeline(&world.world_id).expect("timeline runtime");
        assert_eq!(timeline.current_tick(), 10);
        assert_eq!(timeline.head_tick(), 80);
        assert!(timeline.archive.checkpoints.keys().any(|tick| *tick >= 64));
    }

    #[test]
    fn seek_beyond_head_extends_timeline_and_matches_direct_progress() {
        let mut direct_service = WorldService::new();
        let mut seek_service = WorldService::new();

        let direct_world = init_world(
            &mut direct_service,
            "seed-seek-beyond-head".to_string(),
            1,
            default_init_config(),
        )
        .expect("init direct world");
        let seek_world = init_world(
            &mut seek_service,
            "seed-seek-beyond-head".to_string(),
            1,
            default_init_config(),
        )
        .expect("init seek world");

        exec_world(&mut direct_service, &direct_world.world_id, 24).expect("exec direct world");
        seek_world_to_tick(&mut seek_service, seek_world.world_id.clone(), 24)
            .expect("seek beyond head");

        let direct_metrics =
            world_query_use_cases::get_metrics(&direct_service, direct_world.world_id.clone())
                .expect("direct metrics");
        let seek_metrics =
            world_query_use_cases::get_metrics(&seek_service, seek_world.world_id.clone())
                .expect("seek metrics");
        let seek_timeline = seek_service
            .timeline(&seek_world.world_id)
            .expect("seek timeline runtime");

        assert_eq!(seek_timeline.current_tick(), 24);
        assert_eq!(seek_timeline.head_tick(), 24);
        assert_eq!(seek_metrics.tick, direct_metrics.tick);
        assert_eq!(seek_metrics.mean_height, direct_metrics.mean_height);
        assert_eq!(seek_metrics.max_river_flux, direct_metrics.max_river_flux);
    }

    #[test]
    fn rewind_then_advance_within_head_reuses_existing_timeline() {
        let mut service = WorldService::new();
        let world = init_world(
            &mut service,
            "seed-rewind-advance-reuse".to_string(),
            1,
            default_init_config(),
        )
        .expect("init world");

        exec_world(&mut service, &world.world_id, 20).expect("exec world");
        rewind_world_by_ticks(&mut service, world.world_id.clone(), 8).expect("rewind world");
        let result =
            advance_timeline(&mut service, world.world_id.clone(), 4).expect("advance in head");
        let timeline = service.timeline(&world.world_id).expect("timeline runtime");

        assert_eq!(result.tick, 16.0);
        assert_eq!(result.head_tick, 20.0);
        assert_eq!(timeline.current_tick(), 16);
        assert_eq!(timeline.head_tick(), 20);
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
                simulation_rate: None,
                verification_mode: Some(VerificationMode::HeadlessMetrics),
                timeline: None,
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
                simulation_rate: None,
                verification_mode: Some(VerificationMode::ScientificBenchmark),
                timeline: None,
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
                simulation_rate: None,
                verification_mode: Some(VerificationMode::ScientificBenchmark),
                timeline: None,
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
