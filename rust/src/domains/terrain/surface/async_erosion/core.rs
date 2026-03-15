pub(crate) fn init_async_erosion_automaton(
    seed: &str,
    mut params: TerrainParams,
) -> crate::ErosionAutomatonState {
    sanitize_params(&mut params);

    let terrain = generate(seed, params.clone());
    let (positions, indices) = generate_icosphere(params.level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let rain = build_precipitation_map(
        &positions,
        &nbr_offsets,
        &nbrs,
        &terrain.height,
        params.river_rain_base,
    );

    let v_count = terrain.height.len();
    let active_queue = (0..v_count as u32).collect::<Vec<_>>();
    let mut in_queue = vec![0u8; v_count];
    in_queue.fill(1);

    crate::ErosionAutomatonState {
        positions,
        nbr_offsets,
        nbrs,
        height: terrain.height,
        water: vec![0.0; v_count],
        sediment: vec![0.0; v_count],
        armor: vec![0.0; v_count],
        rain,
        river_flux: terrain.river_flux,
        river_next: terrain.river_next,
        active_queue,
        active_head: 0,
        in_queue,
        rain_cursor: 0,
        tick: 0,
        recent_changed: Vec::new(),
        params,
    }
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) {
    let v_count = state.height.len();
    if v_count == 0 {
        return;
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
        return;
    }

    let budget = budget_cells.max(1) as usize;
    state.tick = state.tick.saturating_add(1);
    state.recent_changed.clear();

    let mut changed_mark = vec![0u8; v_count];
    let rain_inject_count = ((budget / 2).clamp(16, 256)).min(v_count);
    inject_async_rain(state, rain_inject_count, &mut changed_mark);

    let mut processed = 0usize;
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

    compact_active_queue(state);
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
    let river_next_before = state.river_next[i];
    let river_flux_before = state.river_flux[i];

    state.armor[i] *= 0.985;

    let (next_idx, local_slope, next_h) = find_local_flow_target(
        &state.positions,
        &state.nbr_offsets,
        &state.nbrs,
        &state.height,
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
        state.river_next[i] = next_idx.map(|n| n as i32).unwrap_or(-1);
        state.river_flux[i] *= 0.95;
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
        state.river_next[i] = n as i32;
    } else {
        state.river_next[i] = -1;
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
    state.river_flux[i] = clamp(
        state.river_flux[i] * 0.85 + outflow_water * 0.35 + water * 0.15,
        0.0,
        1.0,
    );

    if (state.height[i] - h_i_before).abs() > 1e-6
        || (state.water[i] - water_before).abs() > 1e-6
        || (state.sediment[i] - sediment_before).abs() > 1e-6
        || state.river_next[i] != river_next_before
        || (state.river_flux[i] - river_flux_before).abs() > 1e-6
    {
        result.changed = true;
    }

    result
}

