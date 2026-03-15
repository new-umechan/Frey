use std::collections::BTreeMap;

use wasm_bindgen::prelude::*;

use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::domains;
use crate::sim::{step_world, world};

use super::common::world_not_found_error;
use super::super::helpers::{build_erosion_state, sync_erosion_state};
use super::super::state::{ManagedWorld, WorldSyncState};
use super::super::types::InitWorldConfig;
use super::super::types::InitWorldOutput;
use super::super::WorldSimController;

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
                terrain_params: None,
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

        let mut terrain_params = config.terrain_params.unwrap_or_default();
        terrain_params.level = mesh_level;

        let terrain = domains::build_terrain(&seed, terrain_params.clone());
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

        let geology = world::GeologyState {
            height: terrain.height,
            plate_id,
            river_flux: terrain.river_flux,
            river_next: terrain.river_next,
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
        if let Some(target) = config.target_sea_ratio {
            sim_world.exec.target_sea_ratio = target.clamp(0.02, 0.98);
        }
        sim_world.exec.era = world::EraKind::Crust;

        let erosion_state = build_erosion_state(&sim_world, terrain_params.clone());
        let _ = sim_world.attach_river_erosion_state(erosion_state);
        let sync_state = WorldSyncState::from_world(&sim_world);

        let mut managed = ManagedWorld {
            world: sim_world,
            simulation_rate: config.simulation_rate.unwrap_or(1.0).clamp(0.1, 32.0),
            terrain_params,
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

    #[wasm_bindgen(js_name = step_world)]
    pub fn step_world_js(&mut self, world_id: String, tick_count: u32) -> Result<(), JsValue> {
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
            step_world(&mut managed.world);
            sync_erosion_state(&mut managed.world, &managed.terrain_params);
            managed.observe_after_world_change();
            managed.save_history_snapshot_if_needed();
        }

        Ok(())
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
