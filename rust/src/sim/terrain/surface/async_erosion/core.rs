#[cfg(target_arch = "wasm32")]
type ProfileClock = f64;
#[cfg(not(target_arch = "wasm32"))]
type ProfileClock = std::time::Instant;

#[cfg(target_arch = "wasm32")]
fn profile_now() -> ProfileClock {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_now() -> ProfileClock {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    js_sys::Date::now() - start
}

#[cfg(not(target_arch = "wasm32"))]
fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

const FULL_REBUILD_INTERVAL_TICKS: u64 = 8;
const FULL_REBUILD_CHANGED_RATIO: f32 = 0.02;

fn sink_buffers_ready(state: &crate::ErosionAutomatonState, v_count: usize) -> bool {
    state.sink_id.len() == v_count
        && state.sink_route_next.len() == v_count
        && state.sink_dirty.len() == v_count
        && state.sink_spill_cell.len() == state.sink_spill_to.len()
        && state.sink_spill_cell.len() == state.sink_spill_level.len()
        && state.sink_spill_cell.len() == state.sink_capacity_total.len()
        && state.sink_spill_cell.len() == state.sink_capacity_remaining.len()
        && state.sink_spill_cell.len() == state.sink_storage_sediment.len()
        && state.sink_spill_cell.len() == state.sink_overflow_active.len()
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

    let mut previous_changed = std::mem::take(&mut state.recent_changed);
    let phase_start = profile_now();
    let changed_ratio = if v_count > 0 {
        previous_changed.len() as f32 / v_count as f32
    } else {
        0.0
    };
    let force_full_rebuild = state.tick <= 1
        || !sink_buffers_ready(state, v_count)
        || changed_ratio >= FULL_REBUILD_CHANGED_RATIO
        || state
            .tick
            .saturating_sub(state.last_sink_full_rebuild_tick)
            >= FULL_REBUILD_INTERVAL_TICKS;
    let sink_rebuild_stats = rebuild_sink_state(state, &previous_changed, force_full_rebuild);
    breakdown.sink_rebuild_ms += profile_elapsed_ms(phase_start);
    breakdown.sink_rebuild_full_count = sink_rebuild_stats.full_count;
    breakdown.sink_rebuild_partial_count = sink_rebuild_stats.partial_count;
    breakdown.sink_rebuild_skipped_count = sink_rebuild_stats.skipped_count;
    breakdown.sink_rebuild_fallback_full_count = sink_rebuild_stats.fallback_full_count;
    previous_changed.clear();
    state.recent_changed = previous_changed;

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
        if result.changed {
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
            if result.changed {
                mark_changed_vertex(n, &mut changed_mark, &mut state.recent_changed);
            }
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

#[derive(Default)]
struct AsyncCellUpdateResult {
    changed: bool,
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
        &state.positions,
        &state.nbr_offsets,
        &state.nbrs,
        &state.height,
        &state.river_next,
        &state.flow_heading,
        &state.params,
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
    let source_is_coastal = is_coastal_cell(&state.nbr_offsets, &state.nbrs, &state.height, i);
    let openness = local_open_basin_factor(&state.nbr_offsets, &state.nbrs, &state.height, i);
    let shallow_factor = if h_i <= 0.0 && h_i > params.shallow_sea_floor {
        let shallow_range = (0.0 - params.shallow_sea_floor).max(1e-4);
        let depth = clamp(-h_i, 0.0, shallow_range);
        1.0 - depth / shallow_range
    } else {
        0.0
    };
    let estuary_factor = if h_i > 0.0
        && next_idx.is_some()
        && next_h <= 0.0
    {
        1.0
    } else if h_i <= 0.0 && source_is_coastal && state.sediment[i] > 0.0 {
        0.65
    } else {
        0.0
    };

    let mut water = state.water[i];
    let mut sediment = state.sediment[i];

    if water <= 1e-5 && sediment <= 1e-5 && h_i <= 0.0 {
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

    if h_i > 0.0 {
        let competence = state
            .params
            .continent_erodibility_from_competence;
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
        }
    }

    let overload = (sediment - capacity).max(0.0);
    if overload > 0.0 {
        let deposit_context = clamp(
            1.0
                + 0.85 * flattening
                + 0.55 * openness
                + 0.85 * estuary_factor
                + 0.75 * shallow_factor
                + if next_idx.is_none() && h_i > 0.0 { 0.8 } else { 0.0 },
            0.3,
            4.0,
        );
        let deposit_cap = if h_i <= 0.0 {
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
            distribute_deposition_direct_by_context(
                &state.nbr_offsets,
                &state.nbrs,
                &mut state.height,
                &mut state.armor,
                params,
                i,
                deposit_amount,
                flattening,
                openness,
                estuary_factor,
                shallow_factor,
            );
            sediment -= deposit_amount;
            result.changed = true;
            result.deposited_here = true;
        }
    }

    if h_i <= params.shallow_sea_floor && sediment > 0.0 {
        let loss = sediment * 0.35;
        sediment = (sediment - loss).max(0.0);
    }

    let sink_before_next = next_idx;
    apply_sink_capacity_rule(state, i, &mut sediment, &mut next_idx);
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
        let move_frac = clamp(move_base + 0.55 * slope_drive + 0.20 * water_drive, 0.03, 0.92);
        outflow_water = water * move_frac;
        outflow_sediment = sediment * move_frac;
        state.water[n] += outflow_water;
        state.sediment[n] += outflow_sediment;
        result.downstream = Some(n);
    }

    water = (water - outflow_water).max(0.0);
    sediment = (sediment - outflow_sediment).max(0.0);

    let evap = if state.height[i] > 0.0 { 0.30 } else { 0.12 };
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
