use wasm_bindgen::prelude::*;

use crate::sim::hydrology::rebuild_mfd_from_primary;
use crate::sim::world;

use super::super::helpers::{apply_f32, apply_i32, apply_plate_id, sync_erosion_state};
use super::super::state::{ManagedWorld, ManagedWorldExecState, WorldArchive, WorldTransportCache};
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

        let (worlds, archives) = (&mut self.worlds, &mut self.archives);
        let managed = worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let archive = archives
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        archive.append_intervention(managed.world.clock.tick, &ops);

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

        sync_erosion_state(managed);
        managed.reset_exec_state();
        managed.observe_after_world_change();
        archive.save_snapshot_if_needed(managed);

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
            let source_archive = self
                .archives
                .get(&world_id)
                .ok_or_else(|| world_not_found_error(&world_id))?;
            validate_history_tick(tick_u64)?;
            let snapshot = if let Some(found) = source_archive.history.get(&tick_u64) {
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
        let snapshot = snapshot;
        let archive_snapshot = snapshot.clone();
        let snapshot_tick = snapshot.world.clock.tick;
        let new_world_id = self.next_world_id();
        let transport_cache =
            WorldTransportCache::from_world(&snapshot.world, snapshot.geology_dynamics.as_ref());
        let forked = ManagedWorld {
            world: snapshot.world,
            hydrology_dynamics: snapshot.hydrology_dynamics,
            geology_dynamics: snapshot.geology_dynamics,
            feedback: world::FeedbackQueue::new(transport_cache.height.shadow.len()),
            simulation_rate: source_rate,
            geology_params: source_params,
            transport_cache,
            exec_state: ManagedWorldExecState::default(),
        };
        self.worlds.insert(new_world_id.clone(), forked);
        let mut archive = WorldArchive::new();
        archive.insert_snapshot(snapshot_tick, archive_snapshot);
        self.archives.insert(new_world_id.clone(), archive);

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

        let (worlds, archives) = (&mut self.worlds, &mut self.archives);
        let managed = worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let archive = archives
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let restored_snapshot = archive
            .history
            .get(&tick_u64)
            .cloned()
            .ok_or_else(|| history_tick_not_available_error(tick_u64))?;

        managed.world = restored_snapshot.world;
        managed.hydrology_dynamics = restored_snapshot.hydrology_dynamics;
        managed.geology_dynamics = restored_snapshot.geology_dynamics;
        if managed.hydrology_dynamics.is_none() {
            sync_erosion_state(managed);
        }
        managed.transport_cache =
            WorldTransportCache::from_world(&managed.world, managed.geology_dynamics.as_ref());
        managed.reset_exec_state();
        archive.insert_snapshot(managed.world.clock.tick, managed.snapshot_world());

        let result = RestoreWorldResult {
            world_id,
            tick: managed.world.clock.tick as f64,
        };
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize restore world result: {err}"))
        })
    }
}
