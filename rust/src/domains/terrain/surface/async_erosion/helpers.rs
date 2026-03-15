fn inject_async_rain(
    state: &mut crate::ErosionAutomatonState,
    count: usize,
    changed_mark: &mut [u8],
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
        if state.height[v] <= params_shallow_cutoff(&state.params) {
            continue;
        }
        let rain_unit = state.rain[v].max(0.0);
        if rain_unit <= 0.0 {
            continue;
        }
        let add = 0.02 * rain_unit / base.max(1e-4);
        state.water[v] = (state.water[v] + add).min(2.0);
        enqueue_active_vertex(state, v);
        mark_changed_vertex(v, changed_mark, &mut state.recent_changed);
    }
}

fn params_shallow_cutoff(params: &TerrainParams) -> f32 {
    (params.shallow_sea_floor * 0.5).min(0.0)
}

fn pop_active_vertex(state: &mut crate::ErosionAutomatonState) -> Option<usize> {
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

fn enqueue_active_vertex(state: &mut crate::ErosionAutomatonState, v: usize) {
    if v >= state.in_queue.len() {
        return;
    }
    if state.in_queue[v] != 0 {
        return;
    }
    state.in_queue[v] = 1;
    state.active_queue.push(v as u32);
}

fn enqueue_neighbors(state: &mut crate::ErosionAutomatonState, v: usize) {
    let start = state.nbr_offsets[v] as usize;
    let end = state.nbr_offsets[v + 1] as usize;
    let neighbors = state.nbrs[start..end].to_vec();
    for n_u32 in neighbors {
        enqueue_active_vertex(state, n_u32 as usize);
    }
}

fn compact_active_queue(state: &mut crate::ErosionAutomatonState) {
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

fn mark_changed_vertex(v: usize, changed_mark: &mut [u8], recent_changed: &mut Vec<u32>) {
    if v >= changed_mark.len() {
        return;
    }
    if changed_mark[v] != 0 {
        return;
    }
    changed_mark[v] = 1;
    recent_changed.push(v as u32);
}

fn mark_neighbors_changed(
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

fn find_local_flow_target(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    v: usize,
) -> (Option<usize>, f32, f32) {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    if end <= start {
        return (None, 0.0, -1.0);
    }

    let mut best = None;
    let mut best_h = f32::INFINITY;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let nh = height[n];
        if nh + 1e-6 < best_h {
            best_h = nh;
            best = Some(n);
        }
    }

    if let Some(n) = best {
        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let slope = ((height[v] - height[n]).max(0.0)) / edge_len;
        (Some(n), slope, height[n])
    } else {
        (None, 0.0, -1.0)
    }
}

fn estimate_local_outflow_slope(
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

fn apply_deposit_direct_to_cell(
    height: &mut [f32],
    armor: &mut [f32],
    params: &TerrainParams,
    v: usize,
    amount: f32,
) {
    if amount <= 0.0 {
        return;
    }
    height[v] = clamp(height[v] + amount, -1.2, 1.2);
    let armor_gain = clamp(amount / params.erosion_max_delta_per_iter.max(1e-6), 0.0, 1.0);
    armor[v] = clamp(armor[v] + 0.55 * armor_gain, 0.0, 1.0);
}

fn distribute_deposition_direct_by_context(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &mut [f32],
    armor: &mut [f32],
    params: &TerrainParams,
    center: usize,
    amount: f32,
    flattening: f32,
    openness: f32,
    estuary_factor: f32,
    shallow_factor: f32,
) {
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
    let shallow_range = (0.0 - params.shallow_sea_floor).max(1e-4);
    let mut weights = Vec::with_capacity(end - start);
    let mut weight_sum = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let nh = height[n];
        let same_band = 1.0 - clamp((nh - center_h).abs() / 0.08, 0.0, 1.0);
        let lower_pref = if nh <= center_h { 1.0 } else { 0.30 };
        let marine_pref = if nh <= 0.0 { 1.0 } else { 0.40 };
        let shallow_pref = if nh <= 0.0 && nh > params.shallow_sea_floor {
            let depth = clamp(-nh, 0.0, shallow_range);
            0.35 + 0.65 * (1.0 - depth / shallow_range)
        } else if nh > 0.0 {
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

