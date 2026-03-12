use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::sim::{step_world, world};
use crate::types::TerrainParams;
use crate::{domains, ErosionAutomatonState};

const DEFAULT_HISTORY_LIMIT: usize = 512;

#[derive(Clone)]
struct ManagedWorld {
    world: world::World,
    simulation_rate: f32,
    terrain_params: TerrainParams,
    history: BTreeMap<u64, world::World>,
}

#[derive(Clone)]
struct SnapshotEntry {
    tick: u64,
    world: world::World,
}

#[derive(Deserialize)]
struct InitWorldConfig {
    #[serde(default)]
    terrain_params: Option<TerrainParams>,
    #[serde(default)]
    target_sea_ratio: Option<f32>,
    #[serde(default)]
    simulation_rate: Option<f32>,
}

#[derive(Deserialize)]
struct InterventionOp {
    cell_id: u32,
    field: String,
    value: f64,
}

#[derive(Serialize)]
struct InitWorldOutput {
    world_id: String,
    tick: f64,
    era: String,
    cell_count: u32,
}

#[derive(Serialize)]
struct FieldResponse {
    field_kind: String,
    stride: u32,
    cell_count: u32,
    sampled_count: u32,
    f32_data: Option<Vec<f32>>,
    u32_data: Option<Vec<u32>>,
    i32_data: Option<Vec<i32>>,
}

#[derive(Serialize)]
struct MetricsResponse {
    world_id: String,
    tick: f64,
    era: String,
    simulation_rate: f32,
    cell_count: u32,
    land_ratio: f32,
    mean_height: f32,
    mean_river_flux: f32,
    max_height: f32,
    min_height: f32,
    max_river_flux: f32,
}

#[derive(Serialize)]
struct PlateStat {
    plate_id: u32,
    cell_count: u32,
    mean_height: f32,
    land_ratio: f32,
    mean_river_flux: f32,
}

#[derive(Serialize)]
struct PlateStatsResponse {
    world_id: String,
    tick: f64,
    plate_count: u32,
    stats: Vec<PlateStat>,
}

#[derive(Serialize)]
struct InterventionResult {
    world_id: String,
    applied: u32,
    rejected: u32,
}

#[derive(Serialize)]
struct ForkWorldResult {
    source_world_id: String,
    world_id: String,
    tick: f64,
}

#[derive(Serialize)]
struct CheckpointResult {
    snapshot_id: String,
    world_id: String,
    tick: f64,
}

#[derive(Serialize)]
struct LoadCheckpointResult {
    source_snapshot_id: String,
    world_id: String,
    tick: f64,
}

#[wasm_bindgen]
pub struct WorldSimController {
    worlds: HashMap<String, ManagedWorld>,
    snapshots: HashMap<String, SnapshotEntry>,
    next_world_seq: u64,
    next_snapshot_seq: u64,
}

impl Default for WorldSimController {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WorldSimController {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WorldSimController {
        WorldSimController {
            worlds: HashMap::new(),
            snapshots: HashMap::new(),
            next_world_seq: 1,
            next_snapshot_seq: 1,
        }
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

        let core = world::CoreCells {
            height: terrain.height,
            plate_id,
            river_flux: terrain.river_flux,
            river_next: terrain.river_next,
        };

        let mesh = world::WorldMesh {
            positions,
            nbr_offsets,
            nbrs,
        };

        let mut sim_world = world::World::new(mesh, core);
        if let Some(target) = config.target_sea_ratio {
            sim_world.target_sea_ratio = target.clamp(0.02, 0.98);
        }
        sim_world.era = world::EraKind::Crust;

        let erosion_state = build_erosion_state(&sim_world, terrain_params.clone());
        let _ = sim_world.attach_river_erosion_state(erosion_state);

        let mut managed = ManagedWorld {
            world: sim_world,
            simulation_rate: config.simulation_rate.unwrap_or(1.0).clamp(0.1, 32.0),
            terrain_params,
            history: BTreeMap::new(),
        };
        managed
            .history
            .insert(managed.world.tick, managed.world.clone());

        let world_id = self.next_world_id();
        let output = InitWorldOutput {
            world_id: world_id.clone(),
            tick: managed.world.tick as f64,
            era: managed.world.era.as_key().to_string(),
            cell_count: managed.world.core.height.len() as u32,
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
            .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;

        let scaled_ticks = ((tick_count as f32) * managed.simulation_rate).round() as u32;
        let steps = scaled_ticks.max(1);

        for _ in 0..steps {
            step_world(&mut managed.world);
            sync_erosion_state(&mut managed.world, &managed.terrain_params);
            managed
                .history
                .insert(managed.world.tick, managed.world.clone());
            trim_history(&mut managed.history, DEFAULT_HISTORY_LIMIT);
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
            .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;
        managed.simulation_rate = rate.clamp(0.1, 32.0);
        Ok(())
    }

    #[wasm_bindgen(js_name = get_field)]
    pub fn get_field_js(
        &self,
        world_id: String,
        field_kind: String,
        lod: u32,
    ) -> Result<JsValue, JsValue> {
        let managed = self
            .worlds
            .get(&world_id)
            .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;
        let stride = lod.max(1);
        let world_ref = &managed.world;

        let response = match field_kind.as_str() {
            "height" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.core.height.len() as u32,
                sampled_count: sampled_len(world_ref.core.height.len(), stride),
                f32_data: Some(sample_f32(&world_ref.core.height, stride)),
                u32_data: None,
                i32_data: None,
            },
            "river_flux" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.core.river_flux.len() as u32,
                sampled_count: sampled_len(world_ref.core.river_flux.len(), stride),
                f32_data: Some(sample_f32(&world_ref.core.river_flux, stride)),
                u32_data: None,
                i32_data: None,
            },
            "plate_id" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.core.plate_id.len() as u32,
                sampled_count: sampled_len(world_ref.core.plate_id.len(), stride),
                f32_data: None,
                u32_data: Some(sample_u32_from_u16(&world_ref.core.plate_id, stride)),
                i32_data: None,
            },
            "river_next" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.core.river_next.len() as u32,
                sampled_count: sampled_len(world_ref.core.river_next.len(), stride),
                f32_data: None,
                u32_data: None,
                i32_data: Some(sample_i32(&world_ref.core.river_next, stride)),
            },
            "mantle_heat" => {
                let default_mantle_heat = vec![0.5; world_ref.core.height.len()];
                let mantle_heat = world_ref
                    .terrain_dynamics
                    .as_ref()
                    .map(|dynamics| dynamics.mantle_heat.as_slice())
                    .filter(|data| data.len() == world_ref.core.height.len())
                    .unwrap_or(default_mantle_heat.as_slice());
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: mantle_heat.len() as u32,
                    sampled_count: sampled_len(mantle_heat.len(), stride),
                    f32_data: Some(sample_f32(mantle_heat, stride)),
                    u32_data: None,
                    i32_data: None,
                }
            }
            _ => {
                return Err(JsValue::from_str(&format!(
                    "invalid field kind: {field_kind}"
                )))
            }
        };

        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize field response: {err}")))
    }

    #[wasm_bindgen(js_name = get_metrics)]
    pub fn get_metrics_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let managed = self
            .worlds
            .get(&world_id)
            .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;
        let w = &managed.world;

        let cell_count = w.core.height.len().max(1) as f32;
        let mut land_cells = 0usize;
        let mut sum_height = 0.0f32;
        let mut sum_flux = 0.0f32;
        let mut max_height = f32::NEG_INFINITY;
        let mut min_height = f32::INFINITY;
        let mut max_flux = 0.0f32;

        for i in 0..w.core.height.len() {
            let h = w.core.height[i];
            let flux = w.core.river_flux.get(i).copied().unwrap_or(0.0);
            if h > 0.0 {
                land_cells += 1;
            }
            sum_height += h;
            sum_flux += flux;
            max_height = max_height.max(h);
            min_height = min_height.min(h);
            max_flux = max_flux.max(flux);
        }

        let response = MetricsResponse {
            world_id,
            tick: w.tick as f64,
            era: w.era.as_key().to_string(),
            simulation_rate: managed.simulation_rate,
            cell_count: w.core.height.len() as u32,
            land_ratio: land_cells as f32 / cell_count,
            mean_height: sum_height / cell_count,
            mean_river_flux: sum_flux / cell_count,
            max_height: if max_height.is_finite() {
                max_height
            } else {
                0.0
            },
            min_height: if min_height.is_finite() {
                min_height
            } else {
                0.0
            },
            max_river_flux: max_flux,
        };

        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize metrics: {err}")))
    }

    #[wasm_bindgen(js_name = get_plate_stats)]
    pub fn get_plate_stats_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let managed = self
            .worlds
            .get(&world_id)
            .ok_or_else(|| JsValue::from_str(&format!("world not found: {world_id}")))?;
        let w = &managed.world;

        let plate_count = w
            .core
            .plate_id
            .iter()
            .copied()
            .max()
            .map(|v| v as usize + 1)
            .unwrap_or(0);

        let mut counts = vec![0u32; plate_count];
        let mut land_counts = vec![0u32; plate_count];
        let mut height_sums = vec![0.0f32; plate_count];
        let mut flux_sums = vec![0.0f32; plate_count];

        for i in 0..w.core.plate_id.len() {
            let pid = w.core.plate_id[i] as usize;
            if pid >= plate_count {
                continue;
            }
            counts[pid] = counts[pid].saturating_add(1);
            let h = w.core.height.get(i).copied().unwrap_or(0.0);
            let flux = w.core.river_flux.get(i).copied().unwrap_or(0.0);
            if h > 0.0 {
                land_counts[pid] = land_counts[pid].saturating_add(1);
            }
            height_sums[pid] += h;
            flux_sums[pid] += flux;
        }

        let mut stats = Vec::with_capacity(plate_count);
        for pid in 0..plate_count {
            let count = counts[pid].max(1);
            stats.push(PlateStat {
                plate_id: pid as u32,
                cell_count: counts[pid],
                mean_height: height_sums[pid] / count as f32,
                land_ratio: land_counts[pid] as f32 / count as f32,
                mean_river_flux: flux_sums[pid] / count as f32,
            });
        }

        let response = PlateStatsResponse {
            world_id,
            tick: w.tick as f64,
            plate_count: plate_count as u32,
            stats,
        };

        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize plate stats: {err}")))
    }

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
                "height" => apply_f32(&mut managed.world.core.height, idx, op.value as f32),
                "river_flux" => apply_f32(
                    &mut managed.world.core.river_flux,
                    idx,
                    (op.value as f32).max(0.0),
                ),
                "river_next" => apply_i32(&mut managed.world.core.river_next, idx, op.value as i32),
                "plate_id" => {
                    if op.value < 0.0 || op.value > u16::MAX as f64 {
                        false
                    } else {
                        apply_u16(&mut managed.world.core.plate_id, idx, op.value as u16)
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
            .insert(managed.world.tick, managed.world.clone());
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
        history.insert(snapshot.tick, snapshot.clone());
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
            (managed.world.clone(), managed.world.tick)
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

impl WorldSimController {
    fn next_world_id(&mut self) -> String {
        let id = format!("world-{:06}", self.next_world_seq);
        self.next_world_seq = self.next_world_seq.saturating_add(1);
        id
    }

    fn next_snapshot_id(&mut self) -> String {
        let id = format!("snapshot-{:06}", self.next_snapshot_seq);
        self.next_snapshot_seq = self.next_snapshot_seq.saturating_add(1);
        id
    }
}

fn build_erosion_state(world: &world::World, params: TerrainParams) -> ErosionAutomatonState {
    let cell_count = world.core.height.len();
    ErosionAutomatonState {
        positions: world.mesh.positions.clone(),
        nbr_offsets: world.mesh.nbr_offsets.clone(),
        nbrs: world.mesh.nbrs.clone(),
        height: world.core.height.clone(),
        water: vec![0.0; cell_count],
        sediment: vec![0.0; cell_count],
        armor: vec![0.0; cell_count],
        rain: vec![0.5; cell_count],
        river_flux: world.core.river_flux.clone(),
        river_next: world.core.river_next.clone(),
        active_queue: (0..cell_count as u32).collect(),
        active_head: 0,
        in_queue: vec![1; cell_count],
        rain_cursor: 0,
        tick: world.tick,
        recent_changed: Vec::new(),
        params,
    }
}

fn sync_erosion_state(world: &mut world::World, params: &TerrainParams) {
    let state = build_erosion_state(world, params.clone());
    let _ = world.attach_river_erosion_state(state);
}

fn trim_history(history: &mut BTreeMap<u64, world::World>, max_entries: usize) {
    while history.len() > max_entries {
        if let Some(oldest) = history.keys().next().copied() {
            history.remove(&oldest);
        } else {
            break;
        }
    }
}

fn sampled_len(total_len: usize, stride: u32) -> u32 {
    if total_len == 0 {
        return 0;
    }
    let step = stride.max(1) as usize;
    total_len.div_ceil(step) as u32
}

fn sample_f32(values: &[f32], stride: u32) -> Vec<f32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .copied()
        .collect()
}

fn sample_u32_from_u16(values: &[u16], stride: u32) -> Vec<u32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .map(|&v| v as u32)
        .collect()
}

fn sample_i32(values: &[i32], stride: u32) -> Vec<i32> {
    values
        .iter()
        .step_by(stride.max(1) as usize)
        .copied()
        .collect()
}

fn apply_f32(values: &mut [f32], index: usize, value: f32) -> bool {
    if index >= values.len() || !value.is_finite() {
        return false;
    }
    values[index] = value;
    true
}

fn apply_i32(values: &mut [i32], index: usize, value: i32) -> bool {
    if index >= values.len() {
        return false;
    }
    values[index] = value;
    true
}

fn apply_u16(values: &mut [u16], index: usize, value: u16) -> bool {
    if index >= values.len() {
        return false;
    }
    values[index] = value;
    true
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::WorldSimController;
    use serde::Deserialize;
    use wasm_bindgen::JsValue;

    #[derive(Deserialize)]
    struct InitResponse {
        world_id: String,
    }

    #[derive(Deserialize)]
    struct MetricsResponse {
        tick: f64,
    }

    #[test]
    fn init_step_and_metrics_work() {
        let mut controller = WorldSimController::new();
        let init = controller
            .init_world_js("seed-a".to_string(), 1, JsValue::NULL)
            .expect("init world");
        let init_data: InitResponse = serde_wasm_bindgen::from_value(init).expect("parse init");
        let world_id = init_data.world_id;

        controller
            .step_world_js(world_id.clone(), 3)
            .expect("step world");
        let metrics = controller.get_metrics_js(world_id).expect("get metrics");
        let metrics_data: MetricsResponse =
            serde_wasm_bindgen::from_value(metrics).expect("parse metrics");
        assert!(metrics_data.tick >= 1.0);
    }
}
