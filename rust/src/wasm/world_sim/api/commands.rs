use std::collections::BTreeMap;

use wasm_bindgen::prelude::*;

use crate::domains::types::TerrainParams;

use super::super::helpers::{apply_f32, apply_i32, apply_u16, sync_erosion_state};
use super::super::state::{ManagedWorld, SnapshotEntry, WorldSyncState};
use super::super::types::{
    CheckpointResult, ForkWorldResult, InterventionOp, InterventionResult, LoadCheckpointResult,
    RestoreWorldResult,
};
use super::super::WorldSimController;
use super::common::{
    history_tick_not_available_error, validate_checkpoint_tick, validate_integer_tick,
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

        for op in ops {
            let idx = op.cell_id as usize;
            let ok = match op.field.as_str() {
                "height" => apply_f32(
                    &mut managed.world.state.geology.height,
                    idx,
                    op.value as f32,
                ),
                "river_flux" => apply_f32(
                    &mut managed.world.state.geology.river_flux,
                    idx,
                    (op.value as f32).max(0.0),
                ),
                "river_next" => apply_i32(
                    &mut managed.world.state.geology.river_next,
                    idx,
                    op.value as i32,
                ),
                "plate_id" => {
                    if op.value < 0.0 || op.value > u16::MAX as f64 {
                        false
                    } else {
                        apply_u16(
                            &mut managed.world.state.geology.plate_id,
                            idx,
                            op.value as u16,
                        )
                    }
                }
                _ => false,
            };
            if ok {
                applied = applied.saturating_add(1);
            } else {
                rejected = rejected.saturating_add(1);
            }
        }

        sync_erosion_state(&mut managed.world, &managed.terrain_params);
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
        let (snapshot, source_rate, source_params) = {
            let source = self
                .worlds
                .get(&world_id)
                .ok_or_else(|| world_not_found_error(&world_id))?;
            validate_checkpoint_tick(tick_u64)?;
            let snapshot = if let Some(found) = source.history.get(&tick_u64) {
                found.clone()
            } else {
                return Err(history_tick_not_available_error(tick_u64));
            };
            (
                snapshot,
                source.simulation_rate,
                source.terrain_params.clone(),
            )
        };
        let new_world_id = self.next_world_id();
        let mut history = BTreeMap::new();
        history.insert(snapshot.exec.tick, snapshot.clone());
        let sync_state = WorldSyncState::from_world(&snapshot);
        let forked = ManagedWorld {
            world: snapshot,
            simulation_rate: source_rate,
            terrain_params: source_params,
            sync_state,
            history,
        };
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
        validate_checkpoint_tick(tick_u64)?;

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
        sync_erosion_state(&mut managed.world, &managed.terrain_params);
        managed.sync_state = WorldSyncState::from_world(&managed.world);
        managed
            .history
            .insert(managed.world.exec.tick, managed.world.clone());

        let result = RestoreWorldResult {
            world_id,
            tick: managed.world.exec.tick as f64,
        };
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize restore world result: {err}"))
        })
    }

    #[wasm_bindgen(js_name = save_checkpoint)]
    pub fn save_checkpoint_js(&mut self, world_id: String) -> Result<JsValue, JsValue> {
        let (world_clone, tick) = {
            let managed = self
                .worlds
                .get(&world_id)
                .ok_or_else(|| world_not_found_error(&world_id))?;
            (managed.world.clone(), managed.world.exec.tick)
        };
        let snapshot_id = self.next_snapshot_id();
        let entry = SnapshotEntry {
            tick,
            world: world_clone,
        };
        self.snapshots.insert(snapshot_id.clone(), entry);

        let result = CheckpointResult {
            snapshot_id,
            world_id,
            tick: tick as f64,
        };
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize checkpoint result: {err}"))
        })
    }

    #[wasm_bindgen(js_name = load_checkpoint)]
    pub fn load_checkpoint_js(&mut self, snapshot_id: String) -> Result<JsValue, JsValue> {
        let snapshot =
            self.snapshots.get(&snapshot_id).cloned().ok_or_else(|| {
                JsValue::from_str(&format!("checkpoint not found: {snapshot_id}"))
            })?;

        let world_id = self.next_world_id();
        let mut history = BTreeMap::new();
        history.insert(snapshot.tick, snapshot.world.clone());
        self.worlds.insert(
            world_id.clone(),
            ManagedWorld {
                sync_state: WorldSyncState::from_world(&snapshot.world),
                world: snapshot.world,
                simulation_rate: 1.0,
                terrain_params: TerrainParams::default(),
                history,
            },
        );

        let result = LoadCheckpointResult {
            source_snapshot_id: snapshot_id,
            world_id,
            tick: snapshot.tick as f64,
        };
        serde_wasm_bindgen::to_value(&result)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize load result: {err}")))
    }
}
