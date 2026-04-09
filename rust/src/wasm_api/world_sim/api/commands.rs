use wasm_bindgen::prelude::*;

use crate::sim::exec_world_with_feedback_and_states;

use super::super::helpers::{post_step_sync_light, sync_erosion_state};
use super::super::state::{ManagedWorld, WorldArchive, WorldTransportCache};
use super::super::types::RestoreWorldResult;
use super::super::WorldSimController;
use super::common::{
    history_tick_not_available_error, validate_integer_tick, validate_non_negative_tick, world_not_found_error,
};

fn replay_world_to_tick(
    managed: &mut ManagedWorld,
    archive: &mut WorldArchive,
    target_tick: u64,
) -> Result<(), JsValue> {
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
    if managed.hydrology_dynamics.is_none() {
        sync_erosion_state(managed);
    }

    while managed.world.clock.tick < target_tick {
        managed.with_exec_states(exec_world_with_feedback_and_states);
        post_step_sync_light(managed);
        archive.save_snapshot_if_needed(managed);
    }

    managed.transport_cache =
        WorldTransportCache::from_world(&managed.world, managed.geology_dynamics.as_ref());
    managed.reset_exec_state();
    managed.observe_after_world_change();
    archive.insert_snapshot(managed.world.clock.tick, managed.snapshot_world());
    Ok(())
}

#[wasm_bindgen]
impl WorldSimController {
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
