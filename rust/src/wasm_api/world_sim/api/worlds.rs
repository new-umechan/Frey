use std::collections::BTreeMap;

use wasm_bindgen::prelude::*;

use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::sim;
use crate::sim::{
    exec_world,
    exec_world_profiled,
    exec_world_profiled_detailed,
    world,
    ExecWorldBreakdown,
    ExecWorldBreakdownDetailed,
};

use super::super::helpers::{build_erosion_state, post_step_sync_light};
use super::super::state::{ManagedWorld, WorldSyncState};
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

fn run_post_step(managed: &mut ManagedWorld) {
    post_step_sync_light(&mut managed.world, &managed.geology_params);
    managed.observe_after_world_change();
    managed.save_history_snapshot_if_needed();
}

#[wasm_bindgen]
impl WorldSimController {
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

        let terrain = sim::build_geology(&seed, geology_params.clone());
        let (positions, indices) = generate_icosphere(mesh_level);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);

        if terrain.height.len() != positions.len() || terrain.plate_id.len() != positions.len() {
            return Err(JsValue::from_str(
                "terrain output does not match mesh vertex count",
            ));
        }

        let plate_id = terrain
            .plate_id
            .iter()
            .map(|&v| u16::try_from(v).map_err(|_| JsValue::from_str("plate id exceeds u16 range")))
            .collect::<Result<Vec<_>, _>>()?;

        let river_flow = terrain.river_flux;
        let river_path = terrain.river_next;
        let geology = world::GeologyState {
            height: terrain.height,
            plate_id,
            erosion_rate: vec![0.0; positions.len()],
            deposition_rate: vec![0.0; positions.len()],
            boundary_condition: vec![0.0; positions.len()],
        };

        let mesh = world::WorldMesh {
            positions,
            nbr_offsets,
            nbrs,
        };

        let mut sim_world = world::World::new(mesh, geology);
        sim_world.state.hydrology.river_flow = river_flow;
        sim_world.state.hydrology.river_path = river_path;
        if let Some(target) = config.target_sea_ratio {
            sim_world.exec.target_sea_ratio = target.clamp(0.02, 0.98);
        }
        sim_world.exec.era = world::EraKind::Crust;

        let erosion_state = build_erosion_state(&sim_world, geology_params.clone());
        let _ = sim_world.attach_hydrology_dynamics(erosion_state);
        let sync_state = WorldSyncState::from_world(&sim_world);

        let mut managed = ManagedWorld {
            world: sim_world,
            simulation_rate: config.simulation_rate.unwrap_or(1.0).clamp(0.1, 32.0),
            geology_params,
            sync_state,
            history: BTreeMap::new(),
        };
        managed
            .history
            .insert(managed.world.exec.tick, managed.world.clone());

        let world_id = self.next_world_id();
        let output = InitWorldOutput {
            world_id: world_id.clone(),
            tick: managed.world.exec.tick as f64,
            era: managed.world.exec.era.as_key().to_string(),
            cell_count: managed.world.state.geology.height.len() as u32,
        };
        self.worlds.insert(world_id, managed);

        serde_wasm_bindgen::to_value(&output)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize init result: {err}")))
    }

    #[wasm_bindgen(js_name = exec_world)]
    pub fn exec_world_js(&mut self, world_id: String, tick_count: u32) -> Result<(), JsValue> {
        if tick_count == 0 {
            return Ok(());
        }
        let managed = self
            .worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;

        let scaled_ticks = ((tick_count as f32) * managed.simulation_rate).round() as u32;
        let steps = scaled_ticks.max(1);

        for _ in 0..steps {
            exec_world(&mut managed.world);
            run_post_step(managed);
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
        let managed = self
            .worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;

        let scaled_ticks = ((tick_count as f32) * managed.simulation_rate).round() as u32;
        let steps = scaled_ticks.max(1);
        let mut sim_breakdown = ExecWorldBreakdown::default();
        let mut step_sync_erosion_ms = 0.0;
        let mut step_observe_world_change_ms = 0.0;
        let mut step_history_snapshot_ms = 0.0;

        for _ in 0..steps {
            let step_breakdown = exec_world_profiled(&mut managed.world);
            sim_breakdown.accumulate(&step_breakdown);

            let phase_start = profile_now_ms();
            post_step_sync_light(&mut managed.world, &managed.geology_params);
            step_sync_erosion_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            managed.observe_after_world_change();
            step_observe_world_change_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            managed.save_history_snapshot_if_needed();
            step_history_snapshot_ms += profile_elapsed_ms(phase_start);
        }

        let response = StepWorldProfiledResponse {
            world_id,
            steps,
            exec_feedback_ms: sim_breakdown.exec_feedback_ms,
            exec_geology_terrain_ms: sim_breakdown.exec_geology_terrain_ms,
            exec_climate_ms: sim_breakdown.exec_climate_ms,
            exec_hydrology_ms: sim_breakdown.exec_hydrology_ms,
            exec_ecology_ms: sim_breakdown.exec_ecology_ms,
            exec_society_ms: sim_breakdown.exec_society_ms,
            exec_transition_ms: sim_breakdown.exec_transition_ms,
            step_sync_erosion_ms,
            step_observe_world_change_ms,
            step_history_snapshot_ms,
        };
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize exec_world_profiled: {err}")))
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
        let managed = self
            .worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;

        let scaled_ticks = ((tick_count as f32) * managed.simulation_rate).round() as u32;
        let steps = scaled_ticks.max(1);
        let mut sim_breakdown = ExecWorldBreakdownDetailed::default();
        let mut step_sync_erosion_ms = 0.0;
        let mut step_observe_world_change_ms = 0.0;
        let mut step_history_snapshot_ms = 0.0;

        for _ in 0..steps {
            let step_breakdown = exec_world_profiled_detailed(&mut managed.world);
            sim_breakdown.accumulate(&step_breakdown);

            let phase_start = profile_now_ms();
            post_step_sync_light(&mut managed.world, &managed.geology_params);
            step_sync_erosion_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            managed.observe_after_world_change();
            step_observe_world_change_ms += profile_elapsed_ms(phase_start);

            let phase_start = profile_now_ms();
            managed.save_history_snapshot_if_needed();
            step_history_snapshot_ms += profile_elapsed_ms(phase_start);
        }

        let response = StepWorldProfiledDetailResponse {
            world_id,
            steps,
            exec_feedback_ms: sim_breakdown.breakdown.exec_feedback_ms,
            exec_geology_terrain_ms: sim_breakdown.breakdown.exec_geology_terrain_ms,
            exec_climate_ms: sim_breakdown.breakdown.exec_climate_ms,
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
            sink_rebuild_fallback_full_count: sim_breakdown
                .river
                .sink_rebuild_fallback_full_count,
        };
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize exec_world_profiled_detail: {err}"
            ))
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
