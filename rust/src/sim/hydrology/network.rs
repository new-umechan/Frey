use super::*;

#[derive(Clone, Copy)]
pub(super) struct FlowRouteState {
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

pub(super) struct RiverNetworkBuildOutput {
    pub flux: Vec<f32>,
    pub primary_next: Vec<i32>,
    pub heading: Vec<[f32; 3]>,
    pub downstream_offsets: Vec<u32>,
    pub downstream_cells: Vec<u32>,
    pub downstream_weights: Vec<f32>,
}

pub(super) fn build_river_network(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    runoff: &[f32],
    params: &GeologyParams,
    state: Option<&crate::ErosionAutomatonState>,
) -> RiverNetworkBuildOutput {
    let v_count = height.len();
    if positions.len() != v_count || nbr_offsets.len() != v_count + 1 {
        return RiverNetworkBuildOutput {
            flux: vec![0.0; v_count],
            primary_next: vec![-1; v_count],
            heading: vec![[0.0, 0.0, 0.0]; v_count],
            downstream_offsets: vec![0; v_count + 1],
            downstream_cells: Vec::new(),
            downstream_weights: Vec::new(),
        };
    }

    let (spill_level, spill_steps, overflow_parent) =
        compute_overflow_route_keys(positions, nbr_offsets, nbrs, height);

    let prev_next = state.map(|s| s.prev_river_next.as_slice());
    let prev_heading = state.map(|s| s.flow_heading.as_slice());

    let mut routes = vec![Vec::<(u32, f32)>::new(); v_count];
    for i in 0..v_count {
        if height[i] <= 0.0 {
            continue;
        }

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let mut candidates = Vec::<(usize, f32)>::new();

        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if !flow_key_strictly_decreases(
                spill_level[i],
                spill_steps[i],
                spill_level[n],
                spill_steps[n],
            ) {
                continue;
            }

            let drop = (height[i] - height[n]).max(0.0);
            let spill_gain = (spill_level[i] - spill_level[n]).max(0.0);
            let mut score = drop * 32.0 + spill_gain * 0.8;

            if let Some(prev) = prev_next {
                if prev.get(i).copied().unwrap_or(-1) == n as i32 {
                    score += params.river_inertia_gain;
                }
            }

            if let Some(prev_h) = prev_heading {
                let prev_vec = prev_h.get(i).copied().unwrap_or([0.0, 0.0, 0.0]);
                let prev_len = length3(prev_vec);
                if prev_len > 1e-6 {
                    let cand_vec = flow_direction(positions, i, n);
                    let align = dot3(prev_vec, cand_vec).clamp(-1.0, 1.0);
                    score += params.river_inertia_gain * 0.5 * align.max(0.0);
                    score -= params.river_curvature_penalty * (1.0 - align).max(0.0);
                }
            }

            candidates.push((n, score));
        }

        candidates.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let mut selected = candidates
            .iter()
            .take(3)
            .filter(|(_, score)| score.is_finite() && *score > 0.0)
            .map(|(n, score)| (*n as u32, *score))
            .collect::<Vec<_>>();

        if selected.is_empty() {
            let fallback = overflow_parent[i];
            if fallback >= 0 {
                selected.push((fallback as u32, 1.0));
            }
        } else {
            let sum = selected
                .iter()
                .map(|(_, score)| *score)
                .sum::<f32>()
                .max(1e-6);
            for (_, weight) in &mut selected {
                *weight /= sum;
            }
        }

        routes[i] = selected;
    }

    if let Some(state) = state {
        enforce_sink_overflow_routes(state, &mut routes);
    }

    let mut primary_next = vec![-1; v_count];
    for i in 0..v_count {
        if let Some((target, _)) = routes[i].first().copied() {
            primary_next[i] = target as i32;
        }
    }

    let mut heading = vec![[0.0, 0.0, 0.0]; v_count];
    align_flow_heading(positions, &mut heading, &primary_next);

    let mut order = (0..v_count).collect::<Vec<_>>();
    order.sort_unstable_by(|a, b| {
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

    let mut flux = vec![0.0; v_count];
    for &i in &order {
        let local = runoff.get(i).copied().unwrap_or(0.0).max(0.0);
        flux[i] += local;
        for &(target, weight) in &routes[i] {
            let n = target as usize;
            if n < v_count && weight > 0.0 {
                flux[n] += flux[i] * weight;
            }
        }
    }

    let mut downstream_offsets = Vec::with_capacity(v_count + 1);
    let mut downstream_cells = Vec::<u32>::new();
    let mut downstream_weights = Vec::<f32>::new();
    downstream_offsets.push(0);
    for route in &routes {
        for &(target, weight) in route {
            downstream_cells.push(target);
            downstream_weights.push(weight);
        }
        downstream_offsets.push(downstream_cells.len() as u32);
    }

    RiverNetworkBuildOutput {
        flux,
        primary_next,
        heading,
        downstream_offsets,
        downstream_cells,
        downstream_weights,
    }
}

pub(super) fn enforce_sink_overflow_routes(
    state: &crate::ErosionAutomatonState,
    routes: &mut [Vec<(u32, f32)>],
) {
    let v_count = routes.len();
    if state.sink_id.len() != v_count || state.sink_overflow_active.is_empty() {
        return;
    }

    for (i, route_cells) in routes.iter_mut().enumerate().take(v_count) {
        let sid_raw = state.sink_id[i];
        if sid_raw < 0 {
            continue;
        }
        let sid = sid_raw as usize;
        if sid >= state.sink_overflow_active.len() || state.sink_overflow_active[sid] == 0 {
            continue;
        }

        let spill_from = state.sink_spill_cell.get(sid).copied().unwrap_or(-1);
        let spill_to = state.sink_spill_to.get(sid).copied().unwrap_or(-1);
        if spill_from == i as i32 && spill_to >= 0 {
            route_cells.clear();
            route_cells.push((spill_to as u32, 1.0));
            continue;
        }

        let route = state.sink_route_next.get(i).copied().unwrap_or(-1);
        if route >= 0 {
            route_cells.clear();
            route_cells.push((route as u32, 1.0));
        }
    }
}

pub(super) struct RiverNetworkConstraintInput<'a> {
    pub height: &'a [f32],
    pub previous_flux: &'a [f32],
    pub accumulation_threshold: f32,
}

pub(super) struct RiverNetworkConstraintBuffers<'a> {
    pub flux: &'a mut [f32],
    pub primary_next: &'a mut [i32],
    pub downstream_offsets: &'a mut [u32],
    pub downstream_cells: &'a mut Vec<u32>,
    pub downstream_weights: &'a mut Vec<f32>,
}

pub(super) fn apply_river_network_constraints(
    input: RiverNetworkConstraintInput<'_>,
    buffers: &mut RiverNetworkConstraintBuffers<'_>,
) {
    let height = input.height;
    let previous_flux = input.previous_flux;
    let accumulation_threshold = input.accumulation_threshold;
    let flux = &mut *buffers.flux;
    let primary_next = &mut *buffers.primary_next;
    let downstream_offsets = &mut *buffers.downstream_offsets;
    let downstream_cells = &mut *buffers.downstream_cells;
    let downstream_weights = &mut *buffers.downstream_weights;

    let threshold_on = accumulation_threshold.max(0.0);
    let threshold_off = (threshold_on * ACTIVE_OFF_THRESHOLD_SCALE).max(0.0);

    for i in 0..height.len() {
        let prev_active = previous_flux.get(i).copied().unwrap_or(0.0) >= threshold_off;
        let required = if prev_active {
            threshold_off
        } else {
            threshold_on
        };

        if flux[i] < required {
            flux[i] = 0.0;
            primary_next[i] = -1;
        }
        if height[i] <= 0.0 {
            primary_next[i] = -1;
            flux[i] = 0.0;
        }
    }

    let mut next_cells = Vec::<u32>::new();
    let mut next_weights = Vec::<f32>::new();
    let mut next_offsets = Vec::<u32>::with_capacity(height.len() + 1);
    next_offsets.push(0);
    for i in 0..height.len() {
        let start = downstream_offsets.get(i).copied().unwrap_or(0) as usize;
        let end = downstream_offsets
            .get(i + 1)
            .copied()
            .unwrap_or(start as u32) as usize;
        let cell_start = next_cells.len();
        if flux[i] > 0.0 && primary_next[i] >= 0 {
            for idx in start..end {
                let target = downstream_cells.get(idx).copied().unwrap_or(u32::MAX);
                let weight = downstream_weights.get(idx).copied().unwrap_or(0.0);
                if weight > 0.0 && (target as usize) < height.len() {
                    next_cells.push(target);
                    next_weights.push(weight);
                }
            }
        }
        let cell_end = next_cells.len();
        if cell_end > cell_start {
            let mut best_idx = cell_start;
            for idx in (cell_start + 1)..cell_end {
                if next_weights[idx] > next_weights[best_idx] {
                    best_idx = idx;
                }
            }
            primary_next[i] = next_cells[best_idx] as i32;
        } else {
            primary_next[i] = -1;
        }
        next_offsets.push(next_cells.len() as u32);
    }
    downstream_cells.clear();
    downstream_cells.extend_from_slice(&next_cells);
    downstream_weights.clear();
    downstream_weights.extend_from_slice(&next_weights);
    downstream_offsets.clone_from_slice(&next_offsets);
}

pub(super) fn align_flow_heading(
    positions: &[[f32; 3]],
    heading: &mut [[f32; 3]],
    river_next: &[i32],
) {
    for (i, heading_i) in heading.iter_mut().enumerate() {
        let next = river_next.get(i).copied().unwrap_or(-1);
        if next < 0 {
            *heading_i = [0.0, 0.0, 0.0];
            continue;
        }
        let n = next as usize;
        if n >= positions.len() {
            *heading_i = [0.0, 0.0, 0.0];
            continue;
        }
        *heading_i = flow_direction(positions, i, n);
    }
}

pub(super) fn smooth_and_normalize_flux(
    flux: &mut [f32],
    previous_flux: &[f32],
    flux_scale_ema: &mut f32,
    scratch_flux_samples: &mut Vec<f32>,
) {
    let target_scale = robust_flux_scale(flux, scratch_flux_samples).max(1e-6);
    if !flux_scale_ema.is_finite() || *flux_scale_ema <= 0.0 {
        *flux_scale_ema = target_scale;
    } else {
        *flux_scale_ema = (*flux_scale_ema * (1.0 - FLUX_SCALE_EMA_ALPHA)
            + target_scale * FLUX_SCALE_EMA_ALPHA)
            .max(1e-6);
    }

    for (i, value) in flux.iter_mut().enumerate() {
        let normalized = (*value / *flux_scale_ema).clamp(0.0, 1.0);
        let prev = previous_flux.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        *value =
            (prev * (1.0 - NETWORK_BLEND_ALPHA) + normalized * NETWORK_BLEND_ALPHA).clamp(0.0, 1.0);
    }
}

pub(super) fn robust_flux_scale(flux: &[f32], scratch_flux_samples: &mut Vec<f32>) -> f32 {
    let mut max_flux = 0.0f32;
    scratch_flux_samples.clear();
    if scratch_flux_samples.capacity() < flux.len() / 2 {
        scratch_flux_samples
            .reserve((flux.len() / 2).saturating_sub(scratch_flux_samples.capacity()));
    }
    for &value in flux {
        if value.is_finite() && value > 0.0 {
            max_flux = max_flux.max(value);
            scratch_flux_samples.push(value);
        }
    }
    if scratch_flux_samples.is_empty() {
        return 1.0;
    }

    let idx = ((scratch_flux_samples.len() as f32) * 0.995).floor() as usize;
    let nth = idx.min(scratch_flux_samples.len() - 1);
    scratch_flux_samples
        .select_nth_unstable_by(nth, |a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let q = scratch_flux_samples[nth];
    q.max(max_flux * 0.5).max(1e-6)
}

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
) -> (Vec<f32>, Vec<u32>, Vec<i32>) {
    let v_count = positions.len();
    let mut spill_level = vec![f32::INFINITY; v_count];
    let mut spill_steps = vec![u32::MAX; v_count];
    let mut overflow_parent = vec![-1; v_count];
    let mut heap = BinaryHeap::<FlowRouteState>::new();

    for i in 0..v_count {
        if height[i] <= 0.0 {
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
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
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

pub(super) fn flow_direction(positions: &[[f32; 3]], from: usize, to: usize) -> [f32; 3] {
    let p = positions.get(from).copied().unwrap_or([0.0, 0.0, 1.0]);
    let q = positions.get(to).copied().unwrap_or([0.0, 0.0, 1.0]);
    let raw = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    let proj = project_to_tangent(raw, p);
    normalize3(proj)
}

pub(super) fn project_to_tangent(v: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let dot = dot3(v, normal);
    [
        v[0] - normal[0] * dot,
        v[1] - normal[1] * dot,
        v[2] - normal[2] * dot,
    ]
}

pub(super) fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = length3(v);
    if len <= 1e-6 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

pub(super) fn length3(v: [f32; 3]) -> f32 {
    dot3(v, v).sqrt()
}

pub(super) fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
