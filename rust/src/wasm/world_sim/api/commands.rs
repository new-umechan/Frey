use std::collections::BTreeMap;

use wasm_bindgen::prelude::*;

use crate::domains::types::TerrainParams;

use super::super::helpers::{apply_f32, apply_i32, apply_u16, sync_erosion_state, trim_history};
use super::super::state::{ManagedWorld, SnapshotEntry, DEFAULT_HISTORY_LIMIT};
use super::super::types::{
    CheckpointResult,
    ForkWorldResult,
    InterventionOp,
    InterventionResult,
    LoadCheckpointResult,
};
use super::super::WorldSimController;

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
            .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;

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
        managed
            .history
            .insert(managed.world.exec.tick, managed.world.clone());
        trim_history(&mut managed.history, DEFAULT_HISTORY_LIMIT);

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
        if !tick.is_finite() || tick < 0.0 {
            return Err(JsValue::from_str(
                "tick must be a non-negative finite value",
            ));
        }
        let tick_u64 = tick.round() as u64;
        let (snapshot, source_rate, source_params) = {
            let source = self
                .worlds
                .get(&world_id)
                .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;
            let snapshot = if let Some(found) = source.history.get(&tick_u64) {
                found.clone()
            } else {
                return Err(JsValue::from_str(&format!(
                    "tick {tick_u64} is not available in history"
                )));
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
        let forked = ManagedWorld {
            world: snapshot,
            simulation_rate: source_rate,
            terrain_params: source_params,
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

    #[wasm_bindgen(js_name = save_checkpoint)]
    pub fn save_checkpoint_js(&mut self, world_id: String) -> Result<JsValue, JsValue> {
        let (world_clone, tick) = {
            let managed = self
                .worlds
                .get(&world_id)
                .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;
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
