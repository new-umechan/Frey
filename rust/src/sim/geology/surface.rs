use super::*;

#[path = "surface/shaping.rs"]
mod shaping;
pub(super) use shaping::*;

#[cfg(target_arch = "wasm32")]
type ProfileClock = f64;
#[cfg(not(target_arch = "wasm32"))]
type ProfileClock = std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_now() -> ProfileClock {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_now() -> ProfileClock {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    js_sys::Date::now() - start
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) -> crate::sim::ErosionAutomatonBreakdown {
    let mut breakdown = crate::sim::ErosionAutomatonBreakdown::default();
    let v_count = state.height.len();
    if v_count == 0 {
        return breakdown;
    }
    if state.prev_river_next.len() != v_count {
        state.prev_river_next = state.river_next.clone();
    }
    if state.flow_heading.len() != v_count {
        state.flow_heading = vec![[0.0, 0.0, 0.0]; v_count];
    }
    if state.groundwater_storage.len() != v_count {
        state.groundwater_storage = vec![0.0; v_count];
    }
    if state.positions.len() != v_count
        || state.nbr_offsets.len() != v_count + 1
        || state.water.len() != v_count
        || state.sediment.len() != v_count
        || state.armor.len() != v_count
        || state.rain.len() != v_count
        || state.river_flux.len() != v_count
        || state.river_next.len() != v_count
        || state.in_queue.len() != v_count
    {
        return breakdown;
    }

    let budget = budget_cells.max(1) as usize;
    state.tick = state.tick.saturating_add(1);
    state.recent_changed.clear();

    let mut changed_mark = std::mem::take(&mut state.scratch_changed_mark);
    if changed_mark.len() != v_count {
        changed_mark = vec![0; v_count];
    } else {
        changed_mark.fill(0);
    }
    let rain_inject_count = ((budget / 2).clamp(16, 256)).min(v_count);
    inject_async_rain(state, rain_inject_count, &mut changed_mark);

    let mut processed = 0usize;
    let cell_phase_start = profile_now();
    while processed < budget {
        let Some(v) = pop_active_vertex(state) else {
            break;
        };
        processed += 1;

        let result = process_async_erosion_cell(state, v);
        if result.topography_changed {
            mark_changed_vertex(v, &mut changed_mark, &mut state.recent_changed);
        }
        if result.deposited_here {
            enqueue_neighbors(state, v);
            mark_neighbors_changed(
                &state.nbr_offsets,
                &state.nbrs,
                v,
                &mut changed_mark,
                &mut state.recent_changed,
            );
        }
        if let Some(n) = result.downstream {
            enqueue_active_vertex(state, n);
        }
        if result.changed {
            enqueue_active_vertex(state, v);
        }
    }
    breakdown.cell_process_ms += profile_elapsed_ms(cell_phase_start);

    let queue_phase_start = profile_now();
    compact_active_queue(state);
    breakdown.queue_update_ms += profile_elapsed_ms(queue_phase_start);
    state.scratch_changed_mark = changed_mark;
    breakdown
}

const SINK_TOPOGRAPHY_CHANGE_EPS: f32 = 5e-4;

#[derive(Default)]
struct AsyncCellUpdateResult {
    changed: bool,
    topography_changed: bool,
    deposited_here: bool,
    downstream: Option<usize>,
}

fn process_async_erosion_cell(
    state: &mut crate::ErosionAutomatonState,
    i: usize,
) -> AsyncCellUpdateResult {
    let mut result = AsyncCellUpdateResult::default();
    let params = &state.params;
    let h_i_before = state.height[i];
    let water_before = state.water[i];
    let sediment_before = state.sediment[i];

    state.armor[i] *= 0.985;

    let (mut next_idx, local_slope, next_h) = find_local_flow_target(
        FlowTargetInput {
            positions: &state.positions,
            nbr_offsets: &state.nbr_offsets,
            nbrs: &state.nbrs,
            height: &state.height,
            prev_next: &state.river_next,
            flow_heading: &state.flow_heading,
            params: &state.params,
        },
        i,
    );
    let downstream_slope = if let Some(n) = next_idx {
        estimate_local_outflow_slope(
            &state.positions,
            &state.nbr_offsets,
            &state.nbrs,
            &state.height,
            n,
        )
    } else {
        0.0
    };
    let flattening = clamp(
        1.0 - downstream_slope / (local_slope.max(params.erosion_min_slope) + 1e-6),
        0.0,
        1.0,
    );
    let h_i = state.height[i];
    let sea_level = state.sea_level_offset;
    let shallow_floor = shallow_sea_floor_height(sea_level, params);
    let source_is_coastal = is_coastal_cell(&state.nbr_offsets, &state.nbrs, &state.height, i);
    let openness = local_open_basin_factor(&state.nbr_offsets, &state.nbrs, &state.height, i);
    let shallow_factor = if is_marine_height(h_i, sea_level) && h_i > shallow_floor {
        let shallow_range = (sea_level - shallow_floor).max(1e-4);
        let depth = clamp(sea_level - h_i, 0.0, shallow_range);
        1.0 - depth / shallow_range
    } else {
        0.0
    };
    let estuary_factor = if is_land_height(h_i, sea_level)
        && next_idx.is_some()
        && is_marine_height(next_h, sea_level)
    {
        1.0
    } else if is_marine_height(h_i, sea_level) && source_is_coastal && state.sediment[i] > 0.0 {
        0.65
    } else {
        0.0
    };

    let mut water = state.water[i];
    let mut sediment = state.sediment[i];

    if water <= 1e-5 && sediment <= 1e-5 && is_marine_height(h_i, sea_level) {
        result.downstream = next_idx;
        return result;
    }

    let flux_term = water.max(1e-4).powf(0.85);
    let slope_term = local_slope.max(params.erosion_min_slope).powf(0.70);
    let transport_context = clamp(
        1.0 + 0.20 * (1.0 - flattening) - 0.35 * openness - 0.55 * estuary_factor
            + 0.10 * downstream_slope,
        0.15,
        2.5,
    );
    let capacity = params.sediment_capacity_gain * flux_term * slope_term * transport_context;

    if is_land_height(h_i, sea_level) {
        let competence = state.params.continent_erodibility_from_competence;
        let erodibility = lerp(1.0, 0.5, clamp(competence, 0.0, 1.0));
        let armor_factor = 1.0 - 0.60 * clamp(state.armor[i], 0.0, 1.0);
        let erosion_demand = (capacity - sediment).max(0.0);
        let erode_amount = clamp(
            params.hydraulic_erosion_rate * erosion_demand * erodibility * armor_factor,
            0.0,
            params.erosion_max_delta_per_iter * 0.50,
        );
        if erode_amount > 0.0 {
            state.height[i] = clamp(state.height[i] - erode_amount, -1.2, 1.2);
            sediment += erode_amount;
            result.changed = true;
            if erode_amount >= SINK_TOPOGRAPHY_CHANGE_EPS {
                result.topography_changed = true;
            }
        }
    }

    let overload = (sediment - capacity).max(0.0);
    if overload > 0.0 {
        let deposit_context = clamp(
            1.0 + 0.85 * flattening
                + 0.55 * openness
                + 0.85 * estuary_factor
                + 0.75 * shallow_factor
                + if next_idx.is_none() && is_land_height(h_i, sea_level) {
                    0.8
                } else {
                    0.0
                },
            0.3,
            4.0,
        );
        let deposit_cap = if is_marine_height(h_i, sea_level) {
            params.erosion_max_delta_per_iter * 1.25
        } else {
            params.erosion_max_delta_per_iter
        };
        let deposit_amount = clamp(
            params.hydraulic_deposit_rate * overload * deposit_context,
            0.0,
            sediment.min(deposit_cap.max(0.0)),
        );
        if deposit_amount > 0.0 {
            let mut deposition_buffer = DirectDepositionBuffer {
                nbr_offsets: &state.nbr_offsets,
                nbrs: &state.nbrs,
                height: &mut state.height,
                armor: &mut state.armor,
                params,
                sea_level_offset: sea_level,
            };
            distribute_deposition_direct_by_context(
                &mut deposition_buffer,
                i,
                deposit_amount,
                DepositionContext {
                    flattening,
                    openness,
                    estuary_factor,
                    shallow_factor,
                },
            );
            sediment -= deposit_amount;
            result.changed = true;
            if deposit_amount >= SINK_TOPOGRAPHY_CHANGE_EPS {
                result.topography_changed = true;
                result.deposited_here = true;
            }
        }
    }

    if h_i <= shallow_floor && sediment > 0.0 {
        let loss = sediment * 0.35;
        sediment = (sediment - loss).max(0.0);
    }

    let sink_before_next = next_idx;
    crate::sim::hydrology::apply_fill_spill_sink_rule_to_erosion_cell(
        state,
        i,
        &mut sediment,
        &mut next_idx,
    );
    if next_idx != sink_before_next {
        result.changed = true;
    }

    let mut outflow_water = 0.0f32;
    let mut outflow_sediment = 0.0f32;
    if let Some(n) = next_idx {
        let uphill = state.height[n] > state.height[i] + 1e-5;
        let move_base = if uphill { 0.08 } else { 0.22 };
        let slope_drive = local_slope / (local_slope + 0.02);
        let water_drive = water / (water + 0.05);
        let move_frac = clamp(
            move_base + 0.55 * slope_drive + 0.20 * water_drive,
            0.03,
            0.92,
        );
        outflow_water = water * move_frac;
        outflow_sediment = sediment * move_frac;
        state.water[n] += outflow_water;
        state.sediment[n] += outflow_sediment;
        result.downstream = Some(n);
    }

    water = (water - outflow_water).max(0.0);
    sediment = (sediment - outflow_sediment).max(0.0);

    let evap = if is_land_height(state.height[i], sea_level) {
        0.30
    } else {
        0.12
    };
    water *= 1.0 - evap;
    if water < 1e-5 {
        water = 0.0;
    }
    if sediment < 1e-6 {
        sediment = 0.0;
    }

    state.water[i] = water;
    state.sediment[i] = sediment;

    if (state.height[i] - h_i_before).abs() > 1e-6
        || (state.water[i] - water_before).abs() > 1e-6
        || (state.sediment[i] - sediment_before).abs() > 1e-6
    {
        result.changed = true;
    }

    result
}
pub(super) fn inject_async_rain(
    state: &mut crate::ErosionAutomatonState,
    count: usize,
    _changed_mark: &mut [u8],
) {
    let v_count = state.height.len();
    if v_count == 0 || count == 0 {
        return;
    }
    let base = state.params.river_rain_base.max(0.0);
    if base <= 0.0 {
        return;
    }

    for _ in 0..count {
        let v = state.rain_cursor % v_count;
        state.rain_cursor = (state.rain_cursor + 1) % v_count;
        if state.height[v] <= params_shallow_cutoff(state.sea_level_offset, &state.params) {
            continue;
        }
        let rain_unit = state.rain[v].max(0.0);
        if rain_unit <= 0.0 {
            continue;
        }
        let add = 0.02 * rain_unit / base.max(1e-4);
        state.water[v] = (state.water[v] + add).min(2.0);
        enqueue_active_vertex(state, v);
    }
}

pub(super) fn params_shallow_cutoff(sea_level_offset: f32, params: &GeologyParams) -> f32 {
    sea_level_offset + (params.shallow_sea_floor * 0.5).min(0.0)
}

pub(super) fn pop_active_vertex(state: &mut crate::ErosionAutomatonState) -> Option<usize> {
    while state.active_head < state.active_queue.len() {
        let v = state.active_queue[state.active_head] as usize;
        state.active_head += 1;
        if v >= state.in_queue.len() {
            continue;
        }
        if state.in_queue[v] == 0 {
            continue;
        }
        state.in_queue[v] = 0;
        return Some(v);
    }
    None
}

pub(super) fn enqueue_active_vertex(state: &mut crate::ErosionAutomatonState, v: usize) {
    if v >= state.in_queue.len() {
        return;
    }
    if state.in_queue[v] != 0 {
        return;
    }
    state.in_queue[v] = 1;
    state.active_queue.push(v as u32);
}

pub(super) fn enqueue_neighbors(state: &mut crate::ErosionAutomatonState, v: usize) {
    let start = state.nbr_offsets[v] as usize;
    let end = state.nbr_offsets[v + 1] as usize;
    for idx in start..end {
        let n = state.nbrs[idx] as usize;
        enqueue_active_vertex(state, n);
    }
}

pub(super) fn compact_active_queue(state: &mut crate::ErosionAutomatonState) {
    if state.active_head == 0 {
        return;
    }
    if state.active_head >= state.active_queue.len() {
        state.active_queue.clear();
        state.active_head = 0;
        return;
    }
    if state.active_head > 4096 || state.active_head * 2 > state.active_queue.len() {
        state.active_queue.drain(0..state.active_head);
        state.active_head = 0;
    }
}

pub(super) fn mark_changed_vertex(
    v: usize,
    changed_mark: &mut [u8],
    recent_changed: &mut Vec<u32>,
) {
    if v >= changed_mark.len() {
        return;
    }
    if changed_mark[v] != 0 {
        return;
    }
    changed_mark[v] = 1;
    recent_changed.push(v as u32);
}

pub(super) fn mark_neighbors_changed(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    v: usize,
    changed_mark: &mut [u8],
    recent_changed: &mut Vec<u32>,
) {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    for &n_u32 in &nbrs[start..end] {
        mark_changed_vertex(n_u32 as usize, changed_mark, recent_changed);
    }
}

pub(super) struct FlowTargetInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub height: &'a [f32],
    pub prev_next: &'a [i32],
    pub flow_heading: &'a [[f32; 3]],
    pub params: &'a GeologyParams,
}

pub(super) fn find_local_flow_target(
    input: FlowTargetInput<'_>,
    v: usize,
) -> (Option<usize>, f32, f32) {
    let positions = input.positions;
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let height = input.height;
    let prev_next = input.prev_next;
    let flow_heading = input.flow_heading;
    let params = input.params;

    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    if end <= start {
        return (None, 0.0, -1.0);
    }

    let mut best = None;
    let mut best_score = f32::NEG_INFINITY;
    let mut best_h = height[v];
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let nh = height[n];
        if nh + 1e-6 >= height[v] {
            continue;
        }

        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let drop = (height[v] - nh).max(0.0);
        let mut score = drop / edge_len;

        if prev_next.get(v).copied().unwrap_or(-1) == n as i32 {
            score += params.river_inertia_gain * 0.4;
        }
        let prev_dir = flow_heading.get(v).copied().unwrap_or([0.0, 0.0, 0.0]);
        let prev_len =
            (prev_dir[0] * prev_dir[0] + prev_dir[1] * prev_dir[1] + prev_dir[2] * prev_dir[2])
                .sqrt();
        if prev_len > 1e-6 {
            let cand_dir = normalize3([
                positions[n][0] - positions[v][0],
                positions[n][1] - positions[v][1],
                positions[n][2] - positions[v][2],
            ]);
            let align = clamp(
                prev_dir[0] * cand_dir[0] + prev_dir[1] * cand_dir[1] + prev_dir[2] * cand_dir[2],
                -1.0,
                1.0,
            );
            score += params.river_inertia_gain * 0.2 * align.max(0.0);
            score -= params.river_curvature_penalty * 0.5 * (1.0 - align).max(0.0);
        }

        if score > best_score {
            best_score = score;
            best_h = nh;
            best = Some(n);
        }
    }

    if let Some(n) = best {
        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let slope = ((height[v] - height[n]).max(0.0)) / edge_len;
        (Some(n), slope, best_h)
    } else {
        (None, 0.0, -1.0)
    }
}

pub(super) fn estimate_local_outflow_slope(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    v: usize,
) -> f32 {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    let mut best_slope = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let slope = ((height[v] - height[n]).max(0.0)) / edge_len;
        if slope > best_slope {
            best_slope = slope;
        }
    }
    best_slope
}

pub(super) fn apply_deposit_direct_to_cell(
    height: &mut [f32],
    armor: &mut [f32],
    params: &GeologyParams,
    v: usize,
    amount: f32,
) {
    if amount <= 0.0 {
        return;
    }
    height[v] = clamp(height[v] + amount, -1.2, 1.2);
    let armor_gain = clamp(
        amount / params.erosion_max_delta_per_iter.max(1e-6),
        0.0,
        1.0,
    );
    armor[v] = clamp(armor[v] + 0.55 * armor_gain, 0.0, 1.0);
}

pub(super) struct DirectDepositionBuffer<'a> {
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub height: &'a mut [f32],
    pub armor: &'a mut [f32],
    pub params: &'a GeologyParams,
    pub sea_level_offset: f32,
}

pub(super) fn distribute_deposition_direct_by_context(
    buffer: &mut DirectDepositionBuffer<'_>,
    center: usize,
    amount: f32,
    context: DepositionContext,
) {
    let nbr_offsets = buffer.nbr_offsets;
    let nbrs = buffer.nbrs;
    let height = &mut *buffer.height;
    let armor = &mut *buffer.armor;
    let params = buffer.params;
    let sea_level_offset = buffer.sea_level_offset;
    let flattening = context.flattening;
    let openness = context.openness;
    let estuary_factor = context.estuary_factor;
    let shallow_factor = context.shallow_factor;

    if amount <= 0.0 {
        return;
    }

    let spread_strength = clamp(
        0.10 + 0.45 * flattening + 0.35 * openness + 0.35 * estuary_factor + 0.30 * shallow_factor,
        0.0,
        0.92,
    );
    let center_amount = amount * (1.0 - spread_strength);
    apply_deposit_direct_to_cell(height, armor, params, center, center_amount);

    let spread_pool = (amount - center_amount).max(0.0);
    if spread_pool <= 1e-8 {
        return;
    }

    let start = nbr_offsets[center] as usize;
    let end = nbr_offsets[center + 1] as usize;
    if end <= start {
        apply_deposit_direct_to_cell(height, armor, params, center, spread_pool);
        return;
    }

    let center_h = height[center];
    let shallow_floor = shallow_sea_floor_height(sea_level_offset, params);
    let shallow_range = (sea_level_offset - shallow_floor).max(1e-4);
    let mut weights = Vec::with_capacity(end - start);
    let mut weight_sum = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let nh = height[n];
        let same_band = 1.0 - clamp((nh - center_h).abs() / 0.08, 0.0, 1.0);
        let lower_pref = if nh <= center_h { 1.0 } else { 0.30 };
        let marine_pref = if is_marine_height(nh, sea_level_offset) {
            1.0
        } else {
            0.40
        };
        let shallow_pref = if is_marine_height(nh, sea_level_offset) && nh > shallow_floor {
            let depth = clamp(sea_level_offset - nh, 0.0, shallow_range);
            0.35 + 0.65 * (1.0 - depth / shallow_range)
        } else if is_land_height(nh, sea_level_offset) {
            0.25
        } else {
            0.10
        };
        let w = (0.05 + 0.55 * lower_pref + 0.35 * same_band)
            * (1.0 + 0.70 * openness * same_band)
            * (1.0 + 0.90 * estuary_factor * marine_pref)
            * (1.0 + 1.10 * shallow_factor * shallow_pref);
        if w > 0.0 {
            weight_sum += w;
            weights.push((n, w));
        }
    }

    if weight_sum <= 1e-8 {
        apply_deposit_direct_to_cell(height, armor, params, center, spread_pool);
        return;
    }

    for (n, w) in weights {
        apply_deposit_direct_to_cell(height, armor, params, n, spread_pool * (w / weight_sum));
    }
}

fn is_land_height(height: f32, sea_level_offset: f32) -> bool {
    height > sea_level_offset
}

fn is_marine_height(height: f32, sea_level_offset: f32) -> bool {
    height <= sea_level_offset
}

fn shallow_sea_floor_height(sea_level_offset: f32, params: &GeologyParams) -> f32 {
    sea_level_offset + params.shallow_sea_floor
}

pub(super) fn compute_river_flux_and_next(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    sea_level_offset: f32,
    river_rain_base: f32,
) -> (Vec<f32>, Vec<i32>) {
    let v_count = positions.len();
    let rain = build_precipitation_map(positions, nbr_offsets, nbrs, height, river_rain_base);
    let (spill_level, spill_steps, overflow_parent) =
        compute_overflow_route_keys(positions, nbr_offsets, nbrs, height, sea_level_offset);
    let mut river_next = vec![-1; v_count];
    let mut river_flux = vec![0.0; v_count];

    for i in 0..v_count {
        if height[i] <= sea_level_offset {
            continue;
        }

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;

        let mut best = -1;
        let mut best_drop = -f32::INFINITY;
        let mut best_is_downhill = false;
        let mut best_spill_level = f32::INFINITY;
        let mut best_spill_steps = u32::MAX;

        for &n in &nbrs[start..end] {
            let n = n as usize;
            if !flow_key_strictly_decreases(
                spill_level[i],
                spill_steps[i],
                spill_level[n],
                spill_steps[n],
            ) {
                continue;
            }

            let drop = height[i] - height[n];
            let is_downhill = drop > 0.0;
            let better = if best < 0 {
                true
            } else if is_downhill != best_is_downhill {
                is_downhill
            } else if is_downhill {
                drop > best_drop + 1e-6
            } else if spill_level[n] + 1e-6 < best_spill_level {
                true
            } else if (spill_level[n] - best_spill_level).abs() <= 1e-6 {
                spill_steps[n] < best_spill_steps
            } else {
                false
            };

            if better {
                best = n as i32;
                best_drop = drop;
                best_is_downhill = is_downhill;
                best_spill_level = spill_level[n];
                best_spill_steps = spill_steps[n];
            }
        }

        river_next[i] = if best >= 0 { best } else { overflow_parent[i] };
    }

    let mut order = (0..v_count).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        spill_level[*b]
            .partial_cmp(&spill_level[*a])
            .unwrap_or(Ordering::Equal)
            .then_with(|| spill_steps[*b].cmp(&spill_steps[*a]))
            .then_with(|| {
                height[*b]
                    .partial_cmp(&height[*a])
                    .unwrap_or(Ordering::Equal)
            })
    });

    for &i in &order {
        river_flux[i] += rain[i];
        let next = river_next[i];
        if next >= 0 {
            river_flux[next as usize] += river_flux[i];
        }
    }

    let max_flux = river_flux
        .iter()
        .copied()
        .fold(0.0_f32, |acc, v| if v > acc { v } else { acc })
        .max(1e-6);

    for value in &mut river_flux {
        *value /= max_flux;
    }

    (river_flux, river_next)
}

#[derive(Clone, Copy)]
struct FlowRouteState {
    vertex: usize,
    spill_level: f32,
    steps: u32,
}

impl Ord for FlowRouteState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .spill_level
            .partial_cmp(&self.spill_level)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.steps.cmp(&self.steps))
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

impl PartialOrd for FlowRouteState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for FlowRouteState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex
            && self.steps == other.steps
            && (self.spill_level - other.spill_level).abs() <= 1e-6
    }
}

impl Eq for FlowRouteState {}

pub(super) fn flow_key_better(level_a: f32, steps_a: u32, level_b: f32, steps_b: u32) -> bool {
    level_a + 1e-6 < level_b || ((level_a - level_b).abs() <= 1e-6 && steps_a < steps_b)
}

pub(super) fn flow_key_strictly_decreases(
    from_level: f32,
    from_steps: u32,
    to_level: f32,
    to_steps: u32,
) -> bool {
    flow_key_better(to_level, to_steps, from_level, from_steps)
}

pub(super) fn compute_overflow_route_keys(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    sea_level_offset: f32,
) -> (Vec<f32>, Vec<u32>, Vec<i32>) {
    let v_count = positions.len();
    let _ = positions;
    let mut spill_level = vec![f32::INFINITY; v_count];
    let mut spill_steps = vec![u32::MAX; v_count];
    let mut overflow_parent = vec![-1; v_count];
    let mut heap = BinaryHeap::<FlowRouteState>::new();

    for i in 0..v_count {
        if height[i] <= sea_level_offset {
            spill_level[i] = height[i];
            spill_steps[i] = 0;
            heap.push(FlowRouteState {
                vertex: i,
                spill_level: height[i],
                steps: 0,
            });
        }
    }

    while let Some(state) = heap.pop() {
        let i = state.vertex;
        if (state.spill_level - spill_level[i]).abs() > 1e-6 || state.steps != spill_steps[i] {
            continue;
        }

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        for &n in &nbrs[start..end] {
            let n = n as usize;
            let cand_level = state.spill_level.max(height[n]);
            let cand_steps = state.steps.saturating_add(1);
            if flow_key_better(cand_level, cand_steps, spill_level[n], spill_steps[n]) {
                spill_level[n] = cand_level;
                spill_steps[n] = cand_steps;
                overflow_parent[n] = i as i32;
                heap.push(FlowRouteState {
                    vertex: n,
                    spill_level: cand_level,
                    steps: cand_steps,
                });
            }
        }
    }

    (spill_level, spill_steps, overflow_parent)
}

pub(super) fn compute_lake_depth_map(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    sea_level_offset: f32,
) -> Vec<f32> {
    let (spill_level, _spill_steps, _overflow_parent) =
        compute_overflow_route_keys(positions, nbr_offsets, nbrs, height, sea_level_offset);
    let mut lake_depth = vec![0.0; height.len()];

    for i in 0..height.len() {
        if height[i] <= sea_level_offset {
            continue;
        }
        let depth = (spill_level[i] - height[i]).max(0.0);
        if depth > 1e-4 {
            lake_depth[i] = depth;
        }
    }

    lake_depth
}

pub(super) fn generate_rivers(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    sea_level_offset: f32,
    river_rain_base: f32,
    river_accumulation_threshold: f32,
) -> (Vec<f32>, Vec<i32>) {
    let (mut river_flux, mut river_next) =
        compute_river_flux_and_next(
            positions,
            nbr_offsets,
            nbrs,
            height,
            sea_level_offset,
            river_rain_base,
        );

    for i in 0..positions.len() {
        if river_flux[i] < river_accumulation_threshold {
            river_flux[i] = 0.0;
        }
        if height[i] <= sea_level_offset {
            river_next[i] = -1;
        }
    }

    (river_flux, river_next)
}

pub(super) fn build_precipitation_map(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    river_rain_base: f32,
) -> Vec<f32> {
    let mut rain = vec![0.0; positions.len()];

    for i in 0..positions.len() {
        let p = positions[i];
        let lat = clamp(p[1], -1.0, 1.0).asin();

        let lat_factor = (1.0 - lat.abs() / (std::f32::consts::PI * 0.5)).max(0.0);
        let altitude_factor = 1.0 + 0.20 * height[i].max(0.0);

        let wind_dir = prevailing_wind_dir(p, lat);
        let (upwind_h, downwind_h) =
            directional_neighbor_heights(i, positions, nbr_offsets, nbrs, height, wind_dir);

        let slope_signal = clamp((downwind_h - upwind_h) / 0.20, -1.0, 1.0);
        let windward_boost = slope_signal.max(0.0);
        let leeward_drop = (-slope_signal).max(0.0);
        let barrier_strength = upwind_h.max(0.0);

        let orographic_factor = clamp(
            1.0 + 0.60 * windward_boost * (1.0 + 0.6 * height[i].max(0.0))
                - 1.10 * leeward_drop * (1.0 + 0.8 * barrier_strength),
            0.12,
            2.20,
        );

        rain[i] = river_rain_base * lat_factor * altitude_factor * orographic_factor;
    }

    rain
}

pub(super) fn prevailing_wind_dir(p: [f32; 3], lat: f32) -> [f32; 3] {
    let abs_lat = lat.abs();
    let zonal_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0
    } else if abs_lat < std::f32::consts::PI / 3.0 {
        1.0
    } else {
        -1.0
    };

    let mut east = [-p[2], 0.0, p[0]];
    if length3(east) < 1e-6 {
        east = project_to_tangent([1.0, 0.0, 0.0], p);
    }
    east = normalize3(east);

    let pole = if lat >= 0.0 {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, -1.0, 0.0]
    };
    let meridional = normalize3(project_to_tangent(pole, p));
    let meridional_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0
    } else {
        0.35
    };

    normalize3(add3(
        mul3(east, zonal_sign),
        mul3(meridional, 0.25 * meridional_sign),
    ))
}

pub(super) fn directional_neighbor_heights(
    i: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    wind_dir: [f32; 3],
) -> (f32, f32) {
    let p = positions[i];
    let start = nbr_offsets[i] as usize;
    let end = nbr_offsets[i + 1] as usize;

    let mut up_sum = 0.0;
    let mut up_w = 0.0;
    let mut down_sum = 0.0;
    let mut down_w = 0.0;

    for &n in &nbrs[start..end] {
        let n = n as usize;
        let edge = sub3(positions[n], p);
        let tangent = project_to_tangent(edge, p);
        let len = length3(tangent);
        if len < 1e-6 {
            continue;
        }
        let dir = [tangent[0] / len, tangent[1] / len, tangent[2] / len];
        let score = dot3(dir, wind_dir);

        if score > 0.15 {
            let w = score * score;
            down_sum += height[n] * w;
            down_w += w;
        } else if score < -0.15 {
            let w = score * score;
            up_sum += height[n] * w;
            up_w += w;
        }
    }

    let upwind_h = if up_w > 0.0 { up_sum / up_w } else { height[i] };
    let downwind_h = if down_w > 0.0 {
        down_sum / down_w
    } else {
        height[i]
    };
    (upwind_h, downwind_h)
}

pub(super) fn earth_preset(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    river_rain_base: f32,
) -> GeologyOutput {
    use crate::sim::geology_types::PlateId;

    let mut height = vec![0.0; positions.len()];
    let mut plate_id = vec![PlateId(0); positions.len()];

    let continents = [
        ([0.90, 0.15, 0.35], 0.55, 0.75),
        ([-0.70, 0.35, 0.10], 0.48, 0.70),
        ([0.10, -0.20, -0.95], 0.52, 0.72),
        ([-0.20, -0.65, 0.70], 0.42, 0.68),
    ];

    for (i, p) in positions.iter().enumerate() {
        let lat = clamp(p[1], -1.0, 1.0).asin();
        let mut land_signal = -0.35 + 0.10 * (2.0 * lat).cos();

        for (center, amp, width) in continents {
            let d = chord_distance(*p, center);
            let influence = amp * (-(d * d) / (2.0 * width * width)).exp();
            land_signal += influence;
        }

        let ridge = 0.08 * (6.0 * p[0]).sin() * (5.0 * p[2]).cos();
        height[i] = clamp(land_signal + ridge, -1.0, 1.0);

        plate_id[i] = if height[i] > 0.35 {
            PlateId(1)
        } else if height[i] > 0.05 {
            PlateId(2)
        } else if p[0] > 0.0 {
            PlateId(0)
        } else {
            PlateId(3)
        };
    }

    let (river_flux, river_next) = generate_rivers(
        positions,
        nbr_offsets,
        nbrs,
        &height,
        0.0,
        river_rain_base,
        0.015,
    );
    let lake_depth = compute_lake_depth_map(positions, nbr_offsets, nbrs, &height, 0.0);
    let plate_count = {
        let mut unique = std::collections::HashSet::with_capacity(plate_id.len());
        for &pid in &plate_id {
            unique.insert(pid);
        }
        unique.len() as u32
    };
    let land_count = height.iter().filter(|&&h| h > 0.0).count();
    let land_ratio = land_count as f32 / (height.len().max(1) as f32);

    GeologyOutput {
        height,
        plate_id,
        plate_count,
        land_ratio,
        river_flux,
        river_next,
        volcanism: vec![0.0; positions.len()],
        vertex_buoyancy: vec![-0.08, 0.14, 0.08, -0.10],
        lake_depth,
        vertex_weight: vec![0.66, 0.24, 0.20, 0.61],
        plate_is_ocean: vec![1, 0, 0, 1],
        plate_base_height: vec![-0.06, 0.14, 0.08, -0.03],
        plate_base_weight: vec![0.66, 0.24, 0.20, 0.61],
        vertex_age_norm: vec![0.4, 0.0, 0.0, 0.6],
        debug_trench_strength: vec![0.0; positions.len()],
        debug_arc_strength: vec![0.0; positions.len()],
        debug_backarc_strength: vec![0.0; positions.len()],
        debug_ocean_ocean_arc_strength: vec![0.0; positions.len()],
    }
}
