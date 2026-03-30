use std::collections::BTreeMap;

use wasm_bindgen::prelude::*;

use crate::sim::hydrology::rebuild_mfd_from_primary;

use super::super::helpers::{apply_f32, apply_i32, apply_plate_id, sync_erosion_state};
use super::super::state::{ManagedWorld, ManagedWorldExecState, WorldSyncState};
use super::super::types::{
    ForkWorldResult, InterventionField, InterventionOp, InterventionResult, RestoreWorldResult,
};
use super::super::WorldSimController;
use super::common::{
    history_tick_not_available_error, validate_history_tick, validate_integer_tick,
    validate_non_negative_tick, world_not_found_error,
};

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(js_name = apply_intervention)]
    pub fn apply_intervention_js(
        &mut self,
        world_id: String,
        op_batch_js: JsValue,
    ) -> Result<JsValue, JsValue> {
        let ops = serde_wasm_bindgen::from_value::<Vec<InterventionOp>>(op_batch_js)
            .map_err(|err| JsValue::from_str(&format!("invalid intervention batch: {err}")))?;

        let managed = self
            .worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;

        let mut applied = 0u32;
        let mut rejected = 0u32;
        let mut river_next_updated = false;

        for op in ops {
            let idx = op.cell_id as usize;
            let ok = match op.field {
                InterventionField::Height => apply_f32(
                    &mut managed.world.state.geology.height,
                    idx,
                    op.value as f32,
                ),
                InterventionField::RiverFlux => apply_f32(
                    &mut managed.world.state.hydrology.river_flow,
                    idx,
                    (op.value as f32).max(0.0),
                ),
                InterventionField::RiverNext => apply_i32(
                    &mut managed.world.state.hydrology.river_next,
                    idx,
                    op.value as i32,
                ),
                InterventionField::PlateId => {
                    if op.value < 0.0 || op.value > u32::MAX as f64 {
                        false
                    } else {
                        apply_plate_id(
                            &mut managed.world.state.geology.plate_id,
                            idx,
                            crate::sim::geology_types::PlateId(op.value as u32),
                        )
                    }
                }
            };
            if ok {
                if matches!(op.field, InterventionField::RiverNext) {
                    river_next_updated = true;
                }
                applied = applied.saturating_add(1);
            } else {
                rejected = rejected.saturating_add(1);
            }
        }

        if river_next_updated {
            rebuild_mfd_from_primary(&mut managed.world.state.hydrology);
        }

        sync_erosion_state(&mut managed.world, &managed.geology_params);
        managed.reset_exec_state();
        managed.observe_after_world_change();
        managed.save_history_snapshot_if_needed();

        let result = InterventionResult {
            world_id,
            applied,
            rejected,
        };
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize intervention result: {err}"))
        })
    }

    #[wasm_bindgen(js_name = fork_world)]
    pub fn fork_world_js(&mut self, world_id: String, tick: f64) -> Result<JsValue, JsValue> {
        let tick_u64 = validate_non_negative_tick(tick)?;
        validate_integer_tick(tick, tick_u64)?;
        let (snapshot, source_rate, source_params) = {
            let source = self
                .worlds
                .get(&world_id)
                .ok_or_else(|| world_not_found_error(&world_id))?;
            validate_history_tick(tick_u64)?;
            let snapshot = if let Some(found) = source.history.get(&tick_u64) {
                found.clone()
            } else {
                return Err(history_tick_not_available_error(tick_u64));
            };
            (
                snapshot,
                source.simulation_rate,
                source.geology_params.clone(),
            )
        };
        let new_world_id = self.next_world_id();
        let mut history = BTreeMap::new();
        history.insert(snapshot.clock.tick, snapshot.clone());
        let sync_state = WorldSyncState::from_world(&snapshot);
        let mut forked = ManagedWorld {
            world: snapshot,
            simulation_rate: source_rate,
            geology_params: source_params,
            sync_state,
            history,
            exec_state: ManagedWorldExecState::default(),
        };
        forked
            .world
            .archive
            .history_ticks
            .insert(tick_u64, "fork".to_string());
        self.worlds.insert(new_world_id.clone(), forked);

        let result = ForkWorldResult {
            source_world_id: world_id,
            world_id: new_world_id,
            tick: tick_u64 as f64,
        };

        serde_wasm_bindgen::to_value(&result)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize fork result: {err}")))
    }

    #[wasm_bindgen(js_name = restore_world_to_tick)]
    pub fn restore_world_to_tick_js(
        &mut self,
        world_id: String,
        tick: f64,
    ) -> Result<JsValue, JsValue> {
        let tick_u64 = validate_non_negative_tick(tick)?;
        validate_integer_tick(tick, tick_u64)?;
        validate_history_tick(tick_u64)?;

        let managed = self
            .worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let restored_world = managed
            .history
            .get(&tick_u64)
            .cloned()
            .ok_or_else(|| history_tick_not_available_error(tick_u64))?;

        managed.world = restored_world;
        sync_erosion_state(&mut managed.world, &managed.geology_params);
        managed.sync_state = WorldSyncState::from_world(&managed.world);
        managed.reset_exec_state();
        managed
            .world
            .archive
            .history_ticks
            .insert(managed.world.clock.tick, "restore".to_string());
        managed
            .history
            .insert(managed.world.clock.tick, managed.world.clone());

        let result = RestoreWorldResult {
            world_id,
            tick: managed.world.clock.tick as f64,
        };
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize restore world result: {err}"))
        })
    }
}
