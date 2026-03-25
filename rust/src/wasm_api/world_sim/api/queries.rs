use std::collections::HashSet;

use wasm_bindgen::prelude::*;

use super::super::helpers::{sample_f32, sample_i32, sample_u32_from_plate_id, sampled_len};
use super::super::state::HISTORY_SNAPSHOT_INTERVAL;
use super::super::types::{
    BudgetSummary, FieldResponse, HistoryTicksResponse, MetricsResponse, PlateStat,
    PlateStatsResponse, WorldDeltaQuery, WorldDeltaResponse,
};
use super::super::WorldSimController;
use super::common::world_not_found_error;

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
            .ok_or_else(|| world_not_found_error(&world_id))?;
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
                cell_count: world_ref.state.hydrology.river_flow.len() as u32,
                sampled_count: sampled_len(world_ref.state.hydrology.river_flow.len(), stride),
                f32_data: Some(sample_f32(&world_ref.state.hydrology.river_flow, stride)),
                u32_data: None,
                i32_data: None,
            },
            "temperature" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.climate.temperature.len() as u32,
                sampled_count: sampled_len(world_ref.state.climate.temperature.len(), stride),
                f32_data: Some(sample_f32(&world_ref.state.climate.temperature, stride)),
                u32_data: None,
                i32_data: None,
            },
            "precipitation" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.climate.precipitation.len() as u32,
                sampled_count: sampled_len(world_ref.state.climate.precipitation.len(), stride),
                f32_data: Some(sample_f32(&world_ref.state.climate.precipitation, stride)),
                u32_data: None,
                i32_data: None,
            },
            "runoff" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.climate.runoff.len() as u32,
                sampled_count: sampled_len(world_ref.state.climate.runoff.len(), stride),
                f32_data: Some(sample_f32(&world_ref.state.climate.runoff, stride)),
                u32_data: None,
                i32_data: None,
            },
            "ocean_temperature" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.climate.ocean_temperature.len() as u32,
                sampled_count: sampled_len(world_ref.state.climate.ocean_temperature.len(), stride),
                f32_data: Some(sample_f32(
                    &world_ref.state.climate.ocean_temperature,
                    stride,
                )),
                u32_data: None,
                i32_data: None,
            },
            "plate_id" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.geology.plate_id.len() as u32,
                sampled_count: sampled_len(world_ref.state.geology.plate_id.len(), stride),
                f32_data: None,
                u32_data: Some(sample_u32_from_plate_id(
                    &world_ref.state.geology.plate_id,
                    stride,
                )),
                i32_data: None,
            },
            "river_next" => FieldResponse {
                field_kind,
                stride,
                cell_count: world_ref.state.hydrology.river_next.len() as u32,
                sampled_count: sampled_len(world_ref.state.hydrology.river_next.len(), stride),
                f32_data: None,
                u32_data: None,
                i32_data: Some(sample_i32(&world_ref.state.hydrology.river_next, stride)),
            },
            "river_downstream_offset" => {
                let (offsets, _, _) =
                    hydrology_downstream_to_csr(&world_ref.state.hydrology.river_downstream);
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: offsets.len() as u32,
                    sampled_count: sampled_len(offsets.len(), stride),
                    f32_data: None,
                    u32_data: Some(
                        offsets
                            .into_iter()
                            .step_by(stride.max(1) as usize)
                            .collect(),
                    ),
                    i32_data: None,
                }
            }
            "river_downstream_cell" => {
                let (_, cells, _) =
                    hydrology_downstream_to_csr(&world_ref.state.hydrology.river_downstream);
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: cells.len() as u32,
                    sampled_count: sampled_len(cells.len(), stride),
                    f32_data: None,
                    u32_data: Some(cells.into_iter().step_by(stride.max(1) as usize).collect()),
                    i32_data: None,
                }
            }
            "river_downstream_weight" => {
                let (_, _, weights) =
                    hydrology_downstream_to_csr(&world_ref.state.hydrology.river_downstream);
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: weights.len() as u32,
                    sampled_count: sampled_len(weights.len(), stride),
                    f32_data: Some(sample_f32(&weights, stride)),
                    u32_data: None,
                    i32_data: None,
                }
            }
            "sink_id" => {
                let values = sink_id_values_by_cell(world_ref);
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: values.len() as u32,
                    sampled_count: sampled_len(values.len(), stride),
                    f32_data: None,
                    u32_data: None,
                    i32_data: Some(sample_i32(values.as_slice(), stride)),
                }
            }
            "sink_spill_to" => {
                let spill_by_cell = sink_spill_to_values_by_cell(world_ref);
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: spill_by_cell.len() as u32,
                    sampled_count: sampled_len(spill_by_cell.len(), stride),
                    f32_data: None,
                    u32_data: None,
                    i32_data: Some(sample_i32(spill_by_cell.as_slice(), stride)),
                }
            }
            "sink_capacity_remaining" => {
                let values = sink_capacity_remaining_values_by_cell(world_ref);
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: values.len() as u32,
                    sampled_count: sampled_len(values.len(), stride),
                    f32_data: Some(sample_f32(values.as_slice(), stride)),
                    u32_data: None,
                    i32_data: None,
                }
            }
            "sink_fill_ratio" => {
                let values = sink_fill_ratio_values_by_cell(world_ref);
                FieldResponse {
                    field_kind,
                    stride,
                    cell_count: values.len() as u32,
                    sampled_count: sampled_len(values.len(), stride),
                    f32_data: Some(sample_f32(values.as_slice(), stride)),
                    u32_data: None,
                    i32_data: None,
                }
            }
            "mantle_heat" => {
                let default_mantle_heat = vec![0.5; world_ref.state.geology.height.len()];
                let mantle_heat = world_ref
                    .runtime
                    .geology_dynamics
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
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let w = &managed.world;
        let metrics = w.metrics();

        let response = MetricsResponse {
            world_id,
            tick: w.clock.tick as f64,
            era: w.clock.epoch.as_key().to_string(),
            simulation_rate: managed.simulation_rate,
            real_years_per_tick: w.clock.real_years_per_tick,
            runtime_tick_ms: w.clock.runtime_tick_ms,
            budgets: BudgetSummary {
                geology: w.clock.budgets.geology,
                climate: w.clock.budgets.climate,
                ecology: w.clock.budgets.ecology,
                civilization: w.clock.budgets.civilization,
            },
            cell_count: metrics.cell_count,
            land_cells: metrics.land_cells,
            land_ratio: metrics.land_ratio,
            mean_height: metrics.mean_height,
            height_std_dev: metrics.height_std_dev,
            mean_river_flux: metrics.mean_river_flux,
            max_height: metrics.max_height,
            min_height: metrics.min_height,
            max_river_flux: metrics.max_river_flux,
            top10_river_flux_sum: metrics.top10_river_flux_sum,
            river_active_cells: metrics.river_active_cells,
            river_fragmentation_ratio: metrics.river_fragmentation_ratio,
            river_ocean_reach_ratio: metrics.river_ocean_reach_ratio,
            river_mainstem_persistence: metrics.river_mainstem_persistence,
            river_flux_concentration: metrics.river_flux_concentration,
            continent_count: metrics.continent_count,
            largest_continent_cells: metrics.largest_continent_cells,
        };

        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize metrics: {err}")))
    }

    #[wasm_bindgen(js_name = get_world_delta)]
    pub fn get_world_delta_js(
        &mut self,
        world_id: String,
        options_js: JsValue,
    ) -> Result<JsValue, JsValue> {
        let include_fields = if options_js.is_undefined() || options_js.is_null() {
            None
        } else {
            let query = serde_wasm_bindgen::from_value::<WorldDeltaQuery>(options_js)
                .map_err(|err| JsValue::from_str(&format!("invalid world delta query: {err}")))?;
            query
                .include_fields
                .map(|fields| fields.into_iter().collect::<HashSet<String>>())
        };

        let managed = self
            .worlds
            .get_mut(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let w = &managed.world;
        let response = WorldDeltaResponse {
            world_id,
            tick: w.clock.tick as f64,
            era: w.clock.epoch.as_key().to_string(),
            real_years_per_tick: w.clock.real_years_per_tick,
            runtime_tick_ms: w.clock.runtime_tick_ms,
            budgets: BudgetSummary {
                geology: w.clock.budgets.geology,
                climate: w.clock.budgets.climate,
                ecology: w.clock.budgets.ecology,
                civilization: w.clock.budgets.civilization,
            },
            deltas: managed.sync_state.take_world_field_deltas(|field_kind| {
                include_fields
                    .as_ref()
                    .map(|fields| fields.contains(field_kind))
                    .unwrap_or(true)
            }),
        };
        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize world delta: {err}")))
    }

    #[wasm_bindgen(js_name = get_plate_stats)]
    pub fn get_plate_stats_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let managed = self
            .worlds
            .get(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let w = &managed.world;

        let plate_count = w
            .state
            .geology
            .plate_id
            .iter()
            .copied()
            .max()
            .map(|v| v.as_usize() + 1)
            .unwrap_or(0);

        let mut counts = vec![0u32; plate_count];
        let mut land_counts = vec![0u32; plate_count];
        let mut height_sums = vec![0.0f32; plate_count];
        let mut flux_sums = vec![0.0f32; plate_count];

        for i in 0..w.state.geology.plate_id.len() {
            let pid = w.state.geology.plate_id[i].as_usize();
            if pid >= plate_count {
                continue;
            }
            counts[pid] = counts[pid].saturating_add(1);
            let h = w.state.geology.height.get(i).copied().unwrap_or(0.0);
            let flux = w.state.hydrology.river_flow.get(i).copied().unwrap_or(0.0);
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
            tick: w.clock.tick as f64,
            plate_count: plate_count as u32,
            stats,
        };

        serde_wasm_bindgen::to_value(&response)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize plate stats: {err}")))
    }

    #[wasm_bindgen(js_name = list_history_ticks)]
    pub fn list_history_ticks_js(&self, world_id: String) -> Result<JsValue, JsValue> {
        let managed = self
            .worlds
            .get(&world_id)
            .ok_or_else(|| world_not_found_error(&world_id))?;
        let ticks = managed
            .history
            .keys()
            .copied()
            .map(|tick| tick as f64)
            .collect::<Vec<_>>();
        let response = HistoryTicksResponse {
            world_id,
            interval: HISTORY_SNAPSHOT_INTERVAL as u32,
            ticks,
        };
        serde_wasm_bindgen::to_value(&response).map_err(|err| {
            JsValue::from_str(&format!(
                "failed to serialize history ticks response: {err}"
            ))
        })
    }
}

fn matched_sink_state(
    world: &crate::sim::world::World,
) -> Option<&crate::sim::erosion::ErosionAutomatonState> {
    world
        .runtime
        .hydrology_dynamics
        .as_ref()
        .filter(|state| state.sink_id.len() == world.state.geology.height.len())
}

fn sink_id_values_by_cell(world: &crate::sim::world::World) -> Vec<i32> {
    if let Some(state) = matched_sink_state(world) {
        return state.sink_id.clone();
    }
    vec![-1; world.state.geology.height.len()]
}

fn map_sink_i32_by_cell(
    world: &crate::sim::world::World,
    default_value: i32,
    mapper: impl Fn(&crate::sim::erosion::ErosionAutomatonState, usize, usize) -> i32,
) -> Vec<i32> {
    let cell_count = world.state.geology.height.len();
    let Some(state) = matched_sink_state(world) else {
        return vec![default_value; cell_count];
    };
    let mut out = vec![default_value; cell_count];
    for (i, value) in out.iter_mut().enumerate() {
        let sid = state.sink_id.get(i).copied().unwrap_or(-1);
        if sid < 0 {
            continue;
        }
        let sink_index = sid as usize;
        *value = mapper(state, i, sink_index);
    }
    out
}

fn map_sink_f32_by_cell(
    world: &crate::sim::world::World,
    default_value: f32,
    mapper: impl Fn(&crate::sim::erosion::ErosionAutomatonState, usize, usize) -> f32,
) -> Vec<f32> {
    let cell_count = world.state.geology.height.len();
    let Some(state) = matched_sink_state(world) else {
        return vec![default_value; cell_count];
    };
    let mut out = vec![default_value; cell_count];
    for (i, value) in out.iter_mut().enumerate() {
        let sid = state.sink_id.get(i).copied().unwrap_or(-1);
        if sid < 0 {
            continue;
        }
        let sink_index = sid as usize;
        *value = mapper(state, i, sink_index);
    }
    out
}

fn sink_spill_to_values_by_cell(world: &crate::sim::world::World) -> Vec<i32> {
    map_sink_i32_by_cell(world, -1, |state, _, sid| {
        state.sink_spill_to.get(sid).copied().unwrap_or(-1)
    })
}

fn sink_capacity_remaining_values_by_cell(world: &crate::sim::world::World) -> Vec<f32> {
    map_sink_f32_by_cell(world, 0.0, |state, _, sid| {
        state
            .sink_capacity_remaining
            .get(sid)
            .copied()
            .unwrap_or(0.0)
    })
}

fn sink_fill_ratio_values_by_cell(world: &crate::sim::world::World) -> Vec<f32> {
    map_sink_f32_by_cell(world, 0.0, |state, _, sid| {
        let total = state.sink_capacity_total.get(sid).copied().unwrap_or(0.0);
        let remain = state
            .sink_capacity_remaining
            .get(sid)
            .copied()
            .unwrap_or(total)
            .clamp(0.0, total.max(0.0));
        if total > 1e-6 {
            (1.0 - remain / total).clamp(0.0, 1.0)
        } else {
            0.0
        }
    })
}

fn hydrology_downstream_to_csr(
    routes: &[smallvec::SmallVec<[(u32, f32); 3]>],
) -> (Vec<u32>, Vec<u32>, Vec<f32>) {
    let mut offsets = Vec::with_capacity(routes.len() + 1);
    let mut cells = Vec::new();
    let mut weights = Vec::new();
    offsets.push(0);
    for route in routes {
        for &(cell, weight) in route {
            cells.push(cell);
            weights.push(weight);
        }
        offsets.push(cells.len() as u32);
    }
    (offsets, cells, weights)
}
