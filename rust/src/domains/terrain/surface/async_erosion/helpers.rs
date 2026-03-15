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
    prev_next: &[i32],
    flow_heading: &[[f32; 3]],
    params: &TerrainParams,
    v: usize,
) -> (Option<usize>, f32, f32) {
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
        let prev_len = (prev_dir[0] * prev_dir[0] + prev_dir[1] * prev_dir[1] + prev_dir[2] * prev_dir[2]).sqrt();
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

fn rebuild_sink_state(state: &mut crate::ErosionAutomatonState, _changed: &[u32]) {
    let v_count = state.height.len();
    if v_count == 0 {
        return;
    }
    reset_sink_buffers(state, v_count);

    let downhill = compute_downhill_links(
        &state.height,
        &state.nbr_offsets,
        &state.nbrs,
    );

    let mut terminal = vec![-2_i32; v_count];
    for i in 0..v_count {
        terminal[i] = trace_terminal(i, &state.height, &downhill, &mut terminal);
    }

    let sink_members = build_sink_members(&state.height, &terminal, &mut state.sink_id);
    let old_state = snapshot_sink_state(state);
    let sink_count = sink_members.len();
    resize_sink_state_arrays(state, sink_count);

    for (sid, members) in sink_members.iter().enumerate() {
        if members.is_empty() {
            continue;
        }
        let (best_level, best_from, best_to) = find_sink_spill_edge(
            &state.height,
            &state.nbr_offsets,
            &state.nbrs,
            &state.sink_id,
            sid,
            members,
        );

        state.sink_spill_cell[sid] = best_from;
        state.sink_spill_to[sid] = best_to;
        state.sink_spill_level[sid] = best_level;
        if best_from < 0 {
            state.sink_overflow_active[sid] = 1;
            continue;
        }

        let cap = sink_capacity(&state.height, members, best_level, state.params.sink_min_capacity);
        state.sink_capacity_total[sid] = cap;

        restore_sink_snapshot(state, sid, cap, best_from, best_to, &old_state);

        rebuild_sink_route_for_sid(state, sid, members);
    }
}

fn reset_sink_buffers(state: &mut crate::ErosionAutomatonState, v_count: usize) {
    if state.sink_id.len() != v_count {
        state.sink_id = vec![-1; v_count];
    } else {
        state.sink_id.fill(-1);
    }
    if state.sink_route_next.len() != v_count {
        state.sink_route_next = vec![-1; v_count];
    } else {
        state.sink_route_next.fill(-1);
    }
    if state.sink_dirty.len() != v_count {
        state.sink_dirty = vec![0; v_count];
    } else {
        state.sink_dirty.fill(0);
    }
}

fn compute_downhill_links(height: &[f32], nbr_offsets: &[u32], nbrs: &[u32]) -> Vec<i32> {
    let mut downhill = vec![-1_i32; height.len()];
    for i in 0..height.len() {
        if height[i] <= 0.0 {
            continue;
        }
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let mut best = -1_i32;
        let mut best_h = height[i];
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            let nh = height[n];
            if nh + 1e-6 < best_h {
                best_h = nh;
                best = n as i32;
            }
        }
        downhill[i] = best;
    }
    downhill
}

fn build_sink_members(height: &[f32], terminal: &[i32], sink_id: &mut [i32]) -> Vec<Vec<usize>> {
    let mut root_to_sink = std::collections::HashMap::<usize, usize>::new();
    let mut sink_members = Vec::<Vec<usize>>::new();
    for i in 0..terminal.len() {
        let root = terminal[i];
        if root < 0 {
            continue;
        }
        let r = root as usize;
        if height[r] <= 0.0 {
            continue;
        }
        let sid = *root_to_sink.entry(r).or_insert_with(|| {
            sink_members.push(Vec::new());
            sink_members.len() - 1
        });
        sink_id[i] = sid as i32;
        sink_members[sid].push(i);
    }
    sink_members
}

fn snapshot_sink_state(
    state: &crate::ErosionAutomatonState,
) -> std::collections::HashMap<(i32, i32), (f32, f32, u8)> {
    let mut old_state = std::collections::HashMap::<(i32, i32), (f32, f32, u8)>::new();
    for sid in 0..state.sink_spill_cell.len() {
        let spill_cell = state.sink_spill_cell[sid];
        let spill_to = state.sink_spill_to.get(sid).copied().unwrap_or(-1);
        let remain = state.sink_capacity_remaining.get(sid).copied().unwrap_or(0.0);
        let storage = state.sink_storage_sediment.get(sid).copied().unwrap_or(0.0);
        let active = state.sink_overflow_active.get(sid).copied().unwrap_or(0);
        old_state.insert((spill_cell, spill_to), (remain, storage, active));
    }
    old_state
}

fn resize_sink_state_arrays(state: &mut crate::ErosionAutomatonState, sink_count: usize) {
    state.sink_spill_cell = vec![-1; sink_count];
    state.sink_spill_to = vec![-1; sink_count];
    state.sink_spill_level = vec![0.0; sink_count];
    state.sink_capacity_total = vec![0.0; sink_count];
    state.sink_capacity_remaining = vec![0.0; sink_count];
    state.sink_storage_sediment = vec![0.0; sink_count];
    state.sink_overflow_active = vec![0; sink_count];
}

fn find_sink_spill_edge(
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    sink_id: &[i32],
    sid: usize,
    members: &[usize],
) -> (f32, i32, i32) {
    let mut best_level = f32::INFINITY;
    let mut best_from = -1_i32;
    let mut best_to = -1_i32;
    for &v in members {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if sink_id[n] == sid as i32 {
                continue;
            }
            let cand_level = height[v].max(height[n]);
            if cand_level + 1e-6 < best_level {
                best_level = cand_level;
                best_from = v as i32;
                best_to = n as i32;
            }
        }
    }
    (best_level, best_from, best_to)
}

fn sink_capacity(height: &[f32], members: &[usize], spill_level: f32, sink_min_capacity: f32) -> f32 {
    let mut capacity = 0.0f32;
    for &v in members {
        capacity += (spill_level - height[v]).max(0.0);
    }
    capacity.max(sink_min_capacity.max(0.0))
}

fn restore_sink_snapshot(
    state: &mut crate::ErosionAutomatonState,
    sid: usize,
    cap: f32,
    best_from: i32,
    best_to: i32,
    old_state: &std::collections::HashMap<(i32, i32), (f32, f32, u8)>,
) {
    if let Some((old_remain, old_storage, old_active)) = old_state.get(&(best_from, best_to)) {
        state.sink_capacity_remaining[sid] = old_remain.clamp(0.0, cap);
        state.sink_storage_sediment[sid] = old_storage.max(0.0);
        state.sink_overflow_active[sid] = *old_active;
    } else {
        state.sink_capacity_remaining[sid] = cap;
        state.sink_storage_sediment[sid] = 0.0;
        state.sink_overflow_active[sid] = 0;
    }
}

fn trace_terminal(i: usize, height: &[f32], downhill: &[i32], terminal: &mut [i32]) -> i32 {
    if i >= downhill.len() {
        return -1;
    }
    if height[i] <= 0.0 {
        terminal[i] = -1;
        return -1;
    }
    if terminal[i] >= -1 {
        return terminal[i];
    }
    terminal[i] = -3;
    let next = downhill[i];
    let out = if next < 0 {
        i as i32
    } else {
        let n = next as usize;
        if n >= downhill.len() {
            -1
        } else {
            let traced = trace_terminal(n, height, downhill, terminal);
            if traced == -3 {
                i as i32
            } else {
                traced
            }
        }
    };
    terminal[i] = out;
    out
}

fn rebuild_sink_route_for_sid(
    state: &mut crate::ErosionAutomatonState,
    sid: usize,
    members: &[usize],
) {
    let spill_from = state.sink_spill_cell[sid];
    if spill_from < 0 {
        return;
    }
    let source = spill_from as usize;
    let mut is_member = vec![0u8; state.height.len()];
    for &v in members {
        is_member[v] = 1;
        state.sink_route_next[v] = -1;
    }

    let mut dist = vec![f32::INFINITY; state.height.len()];
    let mut heap = BinaryHeap::<FlowRouteState>::new();
    dist[source] = 0.0;
    heap.push(FlowRouteState {
        vertex: source,
        spill_level: 0.0,
        steps: 0,
    });

    while let Some(cur) = heap.pop() {
        let v = cur.vertex;
        let d = -cur.spill_level;
        if d > dist[v] + 1e-6 {
            continue;
        }
        let start = state.nbr_offsets[v] as usize;
        let end = state.nbr_offsets[v + 1] as usize;
        for &n_u32 in &state.nbrs[start..end] {
            let n = n_u32 as usize;
            if is_member[n] == 0 {
                continue;
            }
            let uphill = (state.height[n] - state.height[v]).max(0.0);
            let cand = d + 1.0 + uphill * 8.0;
            if cand + 1e-6 < dist[n] {
                dist[n] = cand;
                heap.push(FlowRouteState {
                    vertex: n,
                    spill_level: -cand,
                    steps: cur.steps.saturating_add(1),
                });
            }
        }
    }

    for &v in members {
        if v == source || !dist[v].is_finite() {
            continue;
        }
        let start = state.nbr_offsets[v] as usize;
        let end = state.nbr_offsets[v + 1] as usize;
        let mut best = -1_i32;
        let mut best_dist = dist[v];
        for &n_u32 in &state.nbrs[start..end] {
            let n = n_u32 as usize;
            if is_member[n] == 0 {
                continue;
            }
            if dist[n] + 1e-6 < best_dist {
                best_dist = dist[n];
                best = n as i32;
            }
        }
        state.sink_route_next[v] = best;
    }
}

fn apply_sink_capacity_rule(
    state: &mut crate::ErosionAutomatonState,
    cell: usize,
    sediment: &mut f32,
    next_idx: &mut Option<usize>,
) {
    if cell >= state.sink_id.len() {
        return;
    }
    let sid_raw = state.sink_id[cell];
    if sid_raw < 0 {
        return;
    }
    let sid = sid_raw as usize;
    if sid >= state.sink_capacity_remaining.len()
        || sid >= state.sink_overflow_active.len()
        || sid >= state.sink_spill_cell.len()
    {
        return;
    }
    let spill_level = state.sink_spill_level.get(sid).copied().unwrap_or(f32::INFINITY);
    let hysteresis = state.params.sink_overflow_hysteresis.max(0.0);
    let is_pond_cell = state.height[cell] <= spill_level + hysteresis;
    if !is_pond_cell {
        return;
    }

    if state.sink_overflow_active[sid] == 0 {
        if let Some(next) = *next_idx {
            if next < state.sink_id.len() && state.sink_id[next] != sid_raw {
                *next_idx = None;
            }
        }

        let remain = state.sink_capacity_remaining[sid];
        if remain > hysteresis && *sediment > 0.0 {
            let capture = (*sediment * state.params.hydraulic_deposit_rate.max(0.0))
                .max((*sediment * 0.10).min(*sediment))
                .min(remain)
                .min(*sediment);
            if capture > 0.0 {
                apply_deposit_direct_to_cell(
                    &mut state.height,
                    &mut state.armor,
                    &state.params,
                    cell,
                    capture,
                );
                *sediment = (*sediment - capture).max(0.0);
                state.sink_capacity_remaining[sid] = (remain - capture).max(0.0);
                state.sink_storage_sediment[sid] += capture;
            }
        }

        if state.sink_capacity_remaining[sid] <= hysteresis {
            state.sink_overflow_active[sid] = 1;
            enqueue_sink_local_area(state, sid);
        }
        return;
    }

    let spill_from = state.sink_spill_cell[sid];
    let spill_to = state.sink_spill_to.get(sid).copied().unwrap_or(-1);
    if spill_from >= 0 && spill_to >= 0 && cell == spill_from as usize {
        *next_idx = Some(spill_to as usize);
        return;
    }

    let route = state.sink_route_next.get(cell).copied().unwrap_or(-1);
    if route >= 0 {
        *next_idx = Some(route as usize);
    }
}

fn enqueue_sink_local_area(state: &mut crate::ErosionAutomatonState, sid: usize) {
    let spill = state.sink_spill_cell.get(sid).copied().unwrap_or(-1);
    if spill < 0 {
        return;
    }
    let start = spill as usize;
    let max_depth = state.params.sink_local_rebuild_radius.max(1) as usize;
    let mut seen = vec![0u8; state.height.len()];
    let mut queue = std::collections::VecDeque::<(usize, usize)>::new();
    queue.push_back((start, 0));
    seen[start] = 1;
    while let Some((v, depth)) = queue.pop_front() {
        enqueue_active_vertex(state, v);
        if depth >= max_depth {
            continue;
        }
        let s = state.nbr_offsets[v] as usize;
        let e = state.nbr_offsets[v + 1] as usize;
        for &n_u32 in &state.nbrs[s..e] {
            let n = n_u32 as usize;
            if seen[n] != 0 {
                continue;
            }
            seen[n] = 1;
            queue.push_back((n, depth + 1));
        }
    }
}
