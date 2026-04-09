use wasm_bindgen::prelude::*;

use crate::sim::exec_world_with_feedback_and_states;
use crate::sim::hydrology::rebuild_mfd_from_primary;
use crate::sim::world;

use super::super::helpers::{
    apply_f32, apply_i32, apply_plate_id, post_step_sync_light, sync_erosion_state,
};
use super::super::state::{
    ManagedWorld, ManagedWorldExecState, WorldArchive, WorldHistorySnapshot, WorldTransportCache,
};
use super::super::types::{
    ForkWorldResult, InterventionField, InterventionOp, InterventionResult, RestoreWorldResult,
};
use super::super::WorldSimController;
use super::common::{
    history_tick_not_available_error, validate_integer_tick, validate_non_negative_tick,
    world_not_found_error,
};

fn apply_intervention_batch(managed: &mut ManagedWorld, ops: &[InterventionOp]) -> (u32, u32) {
    let mut applied = 0u32;
    let mut rejected = 0u32;
    let mut river_next_updated = false;
    let mut terrain_projection_updated = false;

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
            if matches!(op.field, InterventionField::Height) {
                terrain_projection_updated = true;
            }
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
    if terrain_projection_updated {
        managed.world.refresh_terrain_state();
    }

    (applied, rejected)
}

fn replay_world_to_tick(
    managed: &mut ManagedWorld,
    archive: &mut WorldArchive,
    target_tick: u64,
) -> Result<(), JsValue> {
    let (checkpoint_tick, checkpoint) = archive
        .history
        .range(..=target_tick)
        .next_back()
        .map(|(tick, snapshot)| (*tick, snapshot.clone()))
        .ok_or_else(|| history_tick_not_available_error(target_tick))?;

    managed.world.apply_core(checkpoint.core);
    managed.world.refresh_terrain_state();
    managed.hydrology_dynamics = checkpoint.hydrology_dynamics;
    managed.geology_dynamics = checkpoint.geology_dynamics;
    if managed.hydrology_dynamics.is_none() {
        sync_erosion_state(managed);
    }

    let replay_entries = archive
        .intervention_log
        .iter()
        .filter(|entry| entry.tick > checkpoint_tick && entry.tick <= target_tick)
        .cloned()
        .collect::<Vec<_>>();
    let mut replay_index = 0usize;

    while managed.world.clock.tick < target_tick {
        managed.with_exec_states(exec_world_with_feedback_and_states);
        post_step_sync_light(managed);
        let current_tick = managed.world.clock.tick;
        while replay_index < replay_entries.len()
            && replay_entries[replay_index].tick == current_tick
        {
            let entry = &replay_entries[replay_index];
            let _ = apply_intervention_batch(managed, &entry.ops);
            replay_index += 1;
        }
        archive.save_snapshot_if_needed(managed);
    }

    managed.transport_cache =
        WorldTransportCache::from_world(&managed.world, managed.geology_dynamics.as_ref());
    managed.reset_exec_state();
    managed.observe_after_world_change();
    archive.insert_snapshot(managed.world.clock.tick, managed.snapshot_world());
    Ok(())
}

fn build_fork_archive(
    source_archive: &WorldArchive,
    target_tick: u64,
    target_snapshot: WorldHistorySnapshot,
) -> WorldArchive {
    let mut archive = WorldArchive::new();
    for (tick, snapshot) in source_archive.history.range(..=target_tick) {
        archive.insert_snapshot(*tick, snapshot.clone());
    }
    archive.insert_snapshot(target_tick, target_snapshot);
    archive.intervention_log = source_archive
        .intervention_log
        .iter()
        .filter(|entry| entry.tick <= target_tick)
        .cloned()
        .collect();
    archive
}

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
        let (applied, rejected) = apply_intervention_batch(managed, &ops);

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
        let (mut forked, source_rate, source_params, source_archive_owned) = {
            let source = self
                .worlds
                .get(&world_id)
                .ok_or_else(|| world_not_found_error(&world_id))?;
            let source_archive = self
                .archives
                .get(&world_id)
                .ok_or_else(|| world_not_found_error(&world_id))?;
            (
                source.clone(),
                source.simulation_rate,
                source.geology_params.clone(),
                source_archive.clone(),
            )
        };
        let mut replay_archive = source_archive_owned.clone();
        replay_world_to_tick(&mut forked, &mut replay_archive, tick_u64)?;
        let snapshot_tick = forked.world.clock.tick;
        let fork_archive = build_fork_archive(
            &source_archive_owned,
            snapshot_tick,
            forked.snapshot_world(),
        );
        let new_world_id = self.next_world_id();
        forked.feedback = world::FeedbackQueue::new(forked.transport_cache.height.shadow.len());
        forked.simulation_rate = source_rate;
        forked.geology_params = source_params;
        forked.exec_state = ManagedWorldExecState::default();
        self.worlds.insert(new_world_id.clone(), forked);
        self.archives.insert(new_world_id.clone(), fork_archive);

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

        let (worlds, archives) = (&mut self.worlds, &mut self.archives);
        let managed = worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let archive = archives
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        replay_world_to_tick(managed, archive, tick_u64)?;

        let result = RestoreWorldResult {
            world_id,
            tick: managed.world.clock.tick as f64,
        };
        serde_wasm_bindgen::to_value(&result).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize restore world result: {err}"))
        })
    }
}
