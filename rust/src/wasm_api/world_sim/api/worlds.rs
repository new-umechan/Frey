use wasm_bindgen::prelude::*;

use crate::sim;
use crate::sim::geology_types::GeologyInternal;
use crate::sim::{
    display_group_key, exec_world_profiled_detailed_with_feedback_and_states,
    exec_world_slice_with_states, exec_world_with_feedback_and_states, first_phase,
    module_doc_records, module_graph_record, phase_display_group, world, ExecWorldBreakdown,
    ExecWorldBreakdownDetailed, ExecWorldPhase,
};

use super::super::helpers::{build_erosion_state, post_step_sync_light};
use super::super::state::{ManagedWorld, ManagedWorldExecState, WorldArchive, WorldSyncState};
use super::super::types::ExecWorldSliceResponse;
use super::super::types::InitWorldConfig;
use super::super::types::InitWorldOutput;
use super::super::types::StepWorldProfiledDetailResponse;
use super::super::types::StepWorldProfiledResponse;
use super::super::WorldSimController;
use super::common::world_not_found_error;

#[cfg(target_arch = "wasm32")]
fn profile_now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_now_ms() -> std::time::Instant {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
fn profile_elapsed_ms(start: f64) -> f64 {
    js_sys::Date::now() - start
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_elapsed_ms(start: std::time::Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn run_post_step(managed: &mut ManagedWorld, archive: &mut WorldArchive) {
    post_step_sync_light(managed);
    managed.observe_after_world_change();
    archive.save_snapshot_if_needed(managed);
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

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(js_name = exec_modules)]
    pub fn exec_modules_js(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&module_doc_records())
            .map_err(|err| JsValue::from_str(&format!("failed to serialize exec modules: {err}")))
    }

    #[wasm_bindgen(js_name = exec_module_graph)]
    pub fn exec_module_graph_js(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&module_graph_record()).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize exec module graph: {err}"))
        })
    }

    #[wasm_bindgen(js_name = init_world)]
    pub fn init_world_js(
        &mut self,
        seed: String,
        mesh_level: u32,
        config_js: JsValue,
    ) -> Result<JsValue, JsValue> {
        let config = if config_js.is_undefined() || config_js.is_null() {
            InitWorldConfig {
                geology_params: None,
                target_sea_ratio: None,
                simulation_rate: None,
            }
        } else {
            serde_wasm_bindgen::from_value::<InitWorldConfig>(config_js)
                .map_err(|err| JsValue::from_str(&format!("invalid init config: {err}")))?
        };

        if mesh_level > 8 {
            return Err(JsValue::from_str("mesh_level must be between 0 and 8"));
        }

        let mut geology_params = config.geology_params.unwrap_or_default();
        geology_params.level = mesh_level;

        let (terrain, positions, nbr_offsets, nbrs) =
            sim::build_geology_with_mesh(&seed, geology_params.clone());

        if terrain.height.len() != positions.len() || terrain.plate_id.len() != positions.len() {
            return Err(JsValue::from_str(
                "terrain output does not match mesh vertex count",
            ));
        }

        let plate_id = terrain.plate_id;

        let river_flow = terrain.river_flux;
        let river_next = terrain.river_next;
        let volcanism = terrain.volcanism;
        let vertex_buoyancy = terrain.vertex_buoyancy;
        let lake_depth = terrain.lake_depth;
        let geology = world::GeologyState {
            height: terrain.height,
            lake_depth,
            plate_id,
            erosion_rate: vec![0.0; positions.len()],
            deposition_rate: vec![0.0; positions.len()],
            volcanism,
            vertex_buoyancy,
            geology_internal: vec![GeologyInternal::default(); positions.len()],
            boundary_condition: vec![0.0; positions.len()],
        };

        let mesh = world::WorldMesh {
            positions,
            nbr_offsets,
            nbrs,
        };

        let mut sim_world = world::World::new(mesh, geology);
        sim_world.state.hydrology.river_flow = river_flow;
        sim_world.state.hydrology.river_next = river_next;
        crate::sim::hydrology::rebuild_mfd_from_primary(&mut sim_world.state.hydrology);
        if let Some(target) = config.target_sea_ratio {
            sim_world.control.target_sea_ratio = target.clamp(0.02, 0.98);
        }
        sim_world.control.geology_params = geology_params.clone();
        sim_world.control.erosion_thickness_coupling = geology_params.erosion_thickness_coupling;
        sim_world.control.deposition_thickness_coupling =
            geology_params.deposition_thickness_coupling;
        sim_world.clock.epoch = world::EraKind::Crust;

        let erosion_state = build_erosion_state(&sim_world, geology_params.clone());
        let _ = crate::sim::hydrology::apply_hydrology_state_view(&mut sim_world, &erosion_state);
        let geology_dynamics = sim_world.exec_scratch.geology_dynamics.take();
        let sync_state = WorldSyncState::from_world(&sim_world, geology_dynamics.as_ref());
        let hydrology_dynamics = Some(erosion_state);

        let feedback = world::FeedbackQueue::new(sim_world.cell_count());
        let managed = ManagedWorld {
            world: sim_world,
            hydrology_dynamics,
            geology_dynamics,
            feedback,
            simulation_rate: config.simulation_rate.unwrap_or(1.0).clamp(0.1, 32.0),
            geology_params,
            sync_state,
            exec_state: ManagedWorldExecState::default(),
        };
        let mut archive = WorldArchive::new();
        archive.insert_snapshot(managed.world.clock.tick, managed.snapshot_world());

        let world_id = self.next_world_id();
        let output = InitWorldOutput {
            world_id: world_id.clone(),
            tick: managed.world.clock.tick as f64,
            era: managed.world.clock.epoch.as_key().to_string(),
            cell_count: managed.world.state.geology.height.len() as u32,
        };
        self.archives.insert(world_id.clone(), archive);
        self.worlds.insert(world_id, managed);

        serde_wasm_bindgen::to_value(&output)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize init result: {err}")))
    }

    #[wasm_bindgen(js_name = exec_world)]
    pub fn exec_world_js(&mut self, world_id: String, tick_count: u32) -> Result<(), JsValue> {
        if tick_count == 0 {
            return Ok(());
        }
        let (worlds, archives) = (&mut self.worlds, &mut self.archives);
        let managed = worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let archive = archives
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        reset_pending_slice(managed);

        let steps = scaled_step_count(managed.simulation_rate, tick_count);

        for _ in 0..steps {
            managed.with_exec_states(exec_world_with_feedback_and_states);
            run_post_step(managed, archive);
        }

        Ok(())
    }

    #[wasm_bindgen(js_name = exec_world_profiled)]
    pub fn exec_world_profiled_js(
        &mut self,
        world_id: String,
        tick_count: u32,
    ) -> Result<JsValue, JsValue> {
        if tick_count == 0 {
            let response = StepWorldProfiledResponse {
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
            };
            return serde_wasm_bindgen::to_value(&response).map_err(|err| {
                JsValue::from_str(&format!("failed to serialize exec_world_profiled: {err}"))
            });
        }
        let (worlds, archives) = (&mut self.worlds, &mut self.archives);
        let managed = worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let archive = archives
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        reset_pending_slice(managed);

        let steps = scaled_step_count(managed.simulation_rate, tick_count);
        let mut sim_breakdown = ExecWorldBreakdown::default();
        let mut step_sync_erosion_ms = 0.0;
        let mut step_observe_world_change_ms = 0.0;
        let mut step_history_snapshot_ms = 0.0;

        for _ in 0..steps {
            let step_breakdown = managed
                .with_exec_states(exec_world_profiled_detailed_with_feedback_and_states)
                .breakdown;
            sim_breakdown.accumulate(&step_breakdown);

            let phase_start = profile_now_ms();
            post_step_sync_light(managed);
            step_sync_erosion_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            managed.observe_after_world_change();
            step_observe_world_change_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            archive.save_snapshot_if_needed(managed);
            step_history_snapshot_ms += profile_elapsed_ms(phase_start);
        }

        let response = StepWorldProfiledResponse {
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
        };
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize exec_world_profiled: {err}"))
        })
    }

    #[wasm_bindgen(js_name = exec_world_profiled_detail)]
    pub fn exec_world_profiled_detail_js(
        &mut self,
        world_id: String,
        tick_count: u32,
    ) -> Result<JsValue, JsValue> {
        if tick_count == 0 {
            let response = StepWorldProfiledDetailResponse {
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
            };
            return serde_wasm_bindgen::to_value(&response).map_err(|err| {
                JsValue::from_str(&format!(
                    "failed to serialize exec_world_profiled_detail: {err}"
                ))
            });
        }
        let (worlds, archives) = (&mut self.worlds, &mut self.archives);
        let managed = worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let archive = archives
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        reset_pending_slice(managed);

        let steps = scaled_step_count(managed.simulation_rate, tick_count);
        let mut sim_breakdown = ExecWorldBreakdownDetailed::default();
        let mut step_sync_erosion_ms = 0.0;
        let mut step_observe_world_change_ms = 0.0;
        let mut step_history_snapshot_ms = 0.0;

        for _ in 0..steps {
            let step_breakdown =
                managed.with_exec_states(exec_world_profiled_detailed_with_feedback_and_states);
            sim_breakdown.accumulate(&step_breakdown);

            let phase_start = profile_now_ms();
            post_step_sync_light(managed);
            step_sync_erosion_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            managed.observe_after_world_change();
            step_observe_world_change_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            archive.save_snapshot_if_needed(managed);
            step_history_snapshot_ms += profile_elapsed_ms(phase_start);
        }

        let response = StepWorldProfiledDetailResponse {
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
        };
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize exec_world_profiled_detail: {err}"
            ))
        })
    }

    #[wasm_bindgen(js_name = exec_world_slice)]
    pub fn exec_world_slice_js(
        &mut self,
        world_id: String,
        work_budget: u32,
    ) -> Result<JsValue, JsValue> {
        let (worlds, archives) = (&mut self.worlds, &mut self.archives);
        let managed = worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let archive = archives
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;

        let budget = work_budget.max(1);
        if !managed.exec_is_busy() {
            managed.exec_state.remaining_steps = scaled_step_count(managed.simulation_rate, 1);
            managed.exec_state.next_phase = first_phase();
        }

        let mut remaining_budget = budget;
        let mut processed_ticks = 0u32;
        while remaining_budget > 0 && managed.exec_is_busy() {
            if managed.exec_state.pending_post_step {
                run_post_step(managed, archive);
                managed.exec_state.pending_post_step = false;
                managed.exec_state.remaining_steps =
                    managed.exec_state.remaining_steps.saturating_sub(1);
                processed_ticks = processed_ticks.saturating_add(1);
                remaining_budget = remaining_budget.saturating_sub(1);
                continue;
            }

            let next_phase = managed.exec_state.next_phase;
            let slice =
                managed.with_exec_states(|world, feedback, geology_state, hydrology_state| {
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
                managed.exec_state.pending_post_step = true;
            }
            if slice.work_units_consumed == 0 {
                break;
            }
        }

        let phase = if managed.exec_state.pending_post_step {
            "post_step".to_string()
        } else {
            exec_phase_label(managed.exec_state.next_phase).to_string()
        };
        let response = ExecWorldSliceResponse {
            world_id,
            processed_ticks,
            busy: managed.exec_is_busy(),
            phase,
            tick: managed.world.clock.tick as f64,
        };
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize exec_world_slice: {err}"))
        })
    }

    #[wasm_bindgen(js_name = set_simulation_rate)]
    pub fn set_simulation_rate_js(&mut self, world_id: String, rate: f32) -> Result<(), JsValue> {
        if !rate.is_finite() {
            return Err(JsValue::from_str("rate must be finite"));
        }
        let managed = self
            .worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        managed.simulation_rate = rate.clamp(0.1, 32.0);
        Ok(())
    }
}
