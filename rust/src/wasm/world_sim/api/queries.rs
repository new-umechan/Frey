use wasm_bindgen::prelude::*;

use super::super::helpers::{
    sample_f32,
    sample_i32,
    sample_u32_from_u16,
    sampled_len,
};
use super::super::types::{BudgetSummary, FieldResponse, MetricsResponse, PlateStat, PlateStatsResponse};
use super::super::WorldSimController;

#[wasm_bindgen]
impl WorldSimController {
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
                cell_count: world_ref.state.geology.height.len() as u32,
                sampled_count: sampled_len(world_ref.state.geology.height.len(), stride),
                f32_data: Some(sample_f32(&world_ref.state.geology.height, stride)),
                u32_data: None,
                i32_data: None,
            },
            "river_flux" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.geology.river_flux.len() as u32,
                sampled_count: sampled_len(world_ref.state.geology.river_flux.len(), stride),
                f32_data: Some(sample_f32(&world_ref.state.geology.river_flux, stride)),
                u32_data: None,
                i32_data: None,
            },
            "plate_id" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.geology.plate_id.len() as u32,
                sampled_count: sampled_len(world_ref.state.geology.plate_id.len(), stride),
                f32_data: None,
                u32_data: Some(sample_u32_from_u16(
                    &world_ref.state.geology.plate_id,
                    stride,
                )),
                i32_data: None,
            },
            "river_next" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.geology.river_next.len() as u32,
                sampled_count: sampled_len(world_ref.state.geology.river_next.len(), stride),
                f32_data: None,
                u32_data: None,
                i32_data: Some(sample_i32(&world_ref.state.geology.river_next, stride)),
            },
            "mantle_heat" => {
                let default_mantle_heat = vec![0.5; world_ref.state.geology.height.len()];
                let mantle_heat = world_ref
                    .exec
                    .terrain_dynamics
                    .as_ref()
                    .map(|dynamics| dynamics.mantle_heat.as_slice())
                    .filter(|data| data.len() == world_ref.state.geology.height.len())
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

        let cell_count = w.state.geology.height.len().max(1) as f32;
        let mut land_cells = 0usize;
        let mut sum_height = 0.0f32;
        let mut sum_flux = 0.0f32;
        let mut max_height = f32::NEG_INFINITY;
        let mut min_height = f32::INFINITY;
        let mut max_flux = 0.0f32;

        for i in 0..w.state.geology.height.len() {
            let h = w.state.geology.height[i];
            let flux = w.state.geology.river_flux.get(i).copied().unwrap_or(0.0);
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
            tick: w.exec.tick as f64,
            era: w.exec.era.as_key().to_string(),
            simulation_rate: managed.simulation_rate,
            real_years_per_tick: w.exec.real_years_per_tick,
            runtime_tick_ms: w.exec.runtime_tick_ms,
            budgets: BudgetSummary {
                geology: w.exec.budgets.geology,
                climate: w.exec.budgets.climate,
                ecology: w.exec.budgets.ecology,
                civilization: w.exec.budgets.civilization,
            },
            cell_count: w.state.geology.height.len() as u32,
            land_ratio: land_cells as f32 / cell_count,
            mean_height: sum_height / cell_count,
            mean_river_flux: sum_flux / cell_count,
            max_height: if max_height.is_finite() { max_height } else { 0.0 },
            min_height: if min_height.is_finite() { min_height } else { 0.0 },
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
            .state
            .geology
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

        for i in 0..w.state.geology.plate_id.len() {
            let pid = w.state.geology.plate_id[i] as usize;
            if pid >= plate_count {
                continue;
            }
            counts[pid] = counts[pid].saturating_add(1);
            let h = w.state.geology.height.get(i).copied().unwrap_or(0.0);
            let flux = w.state.geology.river_flux.get(i).copied().unwrap_or(0.0);
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
            tick: w.exec.tick as f64,
            plate_count: plate_count as u32,
            stats,
        };

        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize plate stats: {err}")))
    }
}
