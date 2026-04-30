use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::sim::erosion::ErosionAutomatonState;
use crate::sim::world::HydrologyState;
use crate::GeologyParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FillSpillRebuildMode {
    Full { validation_failed: bool },
    Incremental,
    Skip,
}

#[derive(Clone, Copy)]
struct RouteState {
    vertex: usize,
    cost: f32,
    steps: u32,
}

impl Ord for RouteState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.steps.cmp(&self.steps))
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

impl PartialOrd for RouteState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for RouteState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex
            && self.steps == other.steps
            && (self.cost - other.cost).abs() <= 1e-6
    }
}

impl Eq for RouteState {}

pub(crate) fn rebuild_fill_spill_state(
    hydrology: &mut HydrologyState,
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    params: &GeologyParams,
    water: Option<&[f32]>,
    sediment: Option<&[f32]>,
) {
    let v_count = height.len();
    if v_count == 0 || nbr_offsets.len() != v_count + 1 {
        reset_fill_spill(hydrology, v_count);
        return;
    }

    hydrology.sink_id.resize(v_count, -1);
    hydrology.sink_id.fill(-1);
    hydrology.sink_route_next.resize(v_count, -1);
    hydrology.sink_route_next.fill(-1);

    let old_state = snapshot_sink_state(hydrology);
    let downhill = compute_downhill_links(height, nbr_offsets, nbrs);
    let mut terminal = vec![-2_i32; v_count];
    for i in 0..v_count {
        terminal[i] = trace_terminal(i, height, &downhill, &mut terminal);
    }

    let sink_members = build_sink_members(height, &terminal, &mut hydrology.sink_id);
    let sink_count = sink_members.len();
    resize_sink_state_arrays(hydrology, sink_count);
    rebuild_membership_csr(hydrology, &sink_members);

    for (sid, members) in sink_members.iter().enumerate() {
        update_sink_for_sid(
            hydrology,
            sid,
            members,
            height,
            nbr_offsets,
            nbrs,
            params,
            &old_state,
        );
    }

    recompute_sink_storage_water(hydrology, height, water, params);
    recompute_sink_storage_sediment(hydrology, height, sediment, params);
}

pub(crate) fn fill_spill_buffers_ready(hydrology: &HydrologyState, cell_count: usize) -> bool {
    hydrology.sink_id.len() == cell_count
        && hydrology.sink_route_next.len() == cell_count
        && hydrology.sink_member_offsets.last().copied().unwrap_or(0) as usize
            == hydrology.sink_member_cells.len()
        && hydrology.sink_spill_cell.len() == hydrology.sink_spill_to.len()
        && hydrology.sink_spill_cell.len() == hydrology.sink_spill_level.len()
        && hydrology.sink_spill_cell.len() == hydrology.sink_capacity_total.len()
        && hydrology.sink_spill_cell.len() == hydrology.sink_capacity_remaining.len()
        && hydrology.sink_spill_cell.len() == hydrology.sink_storage_water.len()
        && hydrology.sink_spill_cell.len() == hydrology.sink_storage_sediment.len()
        && hydrology.sink_spill_cell.len() == hydrology.sink_overflow_active.len()
}

pub(crate) fn should_rebuild_fill_spill(
    hydrology: &HydrologyState,
    state: &ErosionAutomatonState,
) -> FillSpillRebuildMode {
    let cell_count = state.height.len();
    if state.tick <= 1 || !fill_spill_buffers_ready(hydrology, cell_count) {
        return FillSpillRebuildMode::Full {
            validation_failed: true,
        };
    }
    if !validate_fill_spill_topology(hydrology, cell_count) {
        return FillSpillRebuildMode::Full {
            validation_failed: true,
        };
    }
    let changed_ratio = if cell_count > 0 {
        state.recent_changed.len() as f32 / cell_count as f32
    } else {
        0.0
    };
    if changed_ratio >= state.params.sink_full_rebuild_changed_ratio.max(0.0) {
        return FillSpillRebuildMode::Full {
            validation_failed: false,
        };
    }
    if state.tick.saturating_sub(state.last_sink_full_rebuild_tick)
        >= state.params.sink_full_rebuild_interval_ticks.max(1) as u64
    {
        return FillSpillRebuildMode::Full {
            validation_failed: false,
        };
    }
    if state.recent_changed.is_empty() {
        FillSpillRebuildMode::Skip
    } else {
        FillSpillRebuildMode::Incremental
    }
}

pub(crate) fn rebuild_fill_spill_state_incremental(
    hydrology: &mut HydrologyState,
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    params: &GeologyParams,
    changed_cells: &[u32],
    neighbor_hops: u32,
) -> usize {
    let cell_count = height.len();
    if cell_count == 0 {
        return 0;
    }
    let mut affected_mask = vec![0u8; cell_count];
    for &cell_u32 in changed_cells {
        let start = cell_u32 as usize;
        if start >= cell_count {
            continue;
        }
        mark_neighbors_within_hops(start, neighbor_hops, nbr_offsets, nbrs, &mut affected_mask);
    }
    let mut affected_sinks = Vec::<usize>::new();
    let mut seen_sink = vec![0u8; hydrology.sink_spill_cell.len()];
    for (cell, &is_affected) in affected_mask.iter().enumerate() {
        if is_affected == 0 {
            continue;
        }
        let sid_raw = hydrology.sink_id.get(cell).copied().unwrap_or(-1);
        if sid_raw < 0 {
            continue;
        }
        let sid = sid_raw as usize;
        if sid >= seen_sink.len() {
            continue;
        }
        if seen_sink[sid] == 0 {
            seen_sink[sid] = 1;
            affected_sinks.push(sid);
        }
    }
    if affected_sinks.is_empty() {
        return 0;
    }

    let old_state = snapshot_sink_state(hydrology);
    for sid in affected_sinks.iter().copied() {
        let members = sink_members_for_sid(hydrology, sid);
        if members.is_empty() {
            continue;
        }
        update_sink_for_sid(
            hydrology,
            sid,
            &members,
            height,
            nbr_offsets,
            nbrs,
            params,
            &old_state,
        );
    }
    affected_sinks.len()
}

pub(crate) fn refresh_fill_spill_storage_and_lakes(
    hydrology: &mut HydrologyState,
    height: &[f32],
    water: Option<&[f32]>,
    sediment: Option<&[f32]>,
    params: &GeologyParams,
) {
    recompute_sink_storage_water(hydrology, height, water, params);
    recompute_sink_storage_sediment(hydrology, height, sediment, params);
    update_public_lake_flags(hydrology, height, params);
}

pub(crate) fn sync_fill_spill_to_erosion(
    state: &mut ErosionAutomatonState,
    hydrology: &HydrologyState,
) {
    if state.sink_id.len() == hydrology.sink_id.len() {
        state.sink_id.copy_from_slice(&hydrology.sink_id);
    } else {
        state.sink_id.clone_from(&hydrology.sink_id);
    }
    if state.sink_route_next.len() == hydrology.sink_route_next.len() {
        state
            .sink_route_next
            .copy_from_slice(&hydrology.sink_route_next);
    } else {
        state.sink_route_next.clone_from(&hydrology.sink_route_next);
    }
    if state.sink_spill_cell.len() == hydrology.sink_spill_cell.len() {
        state
            .sink_spill_cell
            .copy_from_slice(&hydrology.sink_spill_cell);
    } else {
        state.sink_spill_cell.clone_from(&hydrology.sink_spill_cell);
    }
    if state.sink_spill_to.len() == hydrology.sink_spill_to.len() {
        state
            .sink_spill_to
            .copy_from_slice(&hydrology.sink_spill_to);
    } else {
        state.sink_spill_to.clone_from(&hydrology.sink_spill_to);
    }
    if state.sink_capacity_total.len() == hydrology.sink_capacity_total.len() {
        state
            .sink_capacity_total
            .copy_from_slice(&hydrology.sink_capacity_total);
    } else {
        state
            .sink_capacity_total
            .clone_from(&hydrology.sink_capacity_total);
    }
    if state.sink_capacity_remaining.len() == hydrology.sink_capacity_remaining.len() {
        state
            .sink_capacity_remaining
            .copy_from_slice(&hydrology.sink_capacity_remaining);
    } else {
        state
            .sink_capacity_remaining
            .clone_from(&hydrology.sink_capacity_remaining);
    }
    if state.sink_storage_sediment.len() == hydrology.sink_storage_sediment.len() {
        state
            .sink_storage_sediment
            .copy_from_slice(&hydrology.sink_storage_sediment);
    } else {
        state
            .sink_storage_sediment
            .clone_from(&hydrology.sink_storage_sediment);
    }
    if state.sink_spill_level.len() == hydrology.sink_spill_level.len() {
        state
            .sink_spill_level
            .copy_from_slice(&hydrology.sink_spill_level);
    } else {
        state
            .sink_spill_level
            .clone_from(&hydrology.sink_spill_level);
    }
    if state.sink_overflow_active.len() == hydrology.sink_overflow_active.len() {
        state
            .sink_overflow_active
            .copy_from_slice(&hydrology.sink_overflow_active);
    } else {
        state
            .sink_overflow_active
            .clone_from(&hydrology.sink_overflow_active);
    }
    if state.sink_dirty.len() != hydrology.sink_id.len() {
        state.sink_dirty = vec![1; hydrology.sink_id.len()];
    } else {
        state.sink_dirty.fill(1);
    }
}

pub(crate) fn sync_fill_spill_from_erosion(
    hydrology: &mut HydrologyState,
    height: &[f32],
    params: &GeologyParams,
    state: &ErosionAutomatonState,
) {
    hydrology.sink_id.clone_from(&state.sink_id);
    hydrology.sink_route_next.clone_from(&state.sink_route_next);
    hydrology.sink_spill_cell.clone_from(&state.sink_spill_cell);
    hydrology.sink_spill_to.clone_from(&state.sink_spill_to);
    hydrology
        .sink_capacity_total
        .clone_from(&state.sink_capacity_total);
    hydrology
        .sink_capacity_remaining
        .clone_from(&state.sink_capacity_remaining);
    hydrology
        .sink_storage_sediment
        .clone_from(&state.sink_storage_sediment);
    hydrology
        .sink_spill_level
        .clone_from(&state.sink_spill_level);
    hydrology
        .sink_overflow_active
        .clone_from(&state.sink_overflow_active);
    recompute_sink_storage_water(hydrology, height, Some(&state.water), params);
}

pub(crate) fn update_public_lake_flags(
    hydrology: &mut HydrologyState,
    height: &[f32],
    params: &GeologyParams,
) {
    let cell_count = height.len();
    if hydrology.is_lake.len() != cell_count {
        hydrology.is_lake = vec![false; cell_count];
    } else {
        hydrology.is_lake.fill(false);
    }

    let hysteresis = params.sink_overflow_hysteresis.max(0.0);
    #[allow(clippy::needless_range_loop)]
    for cell in 0..cell_count {
        if height[cell] <= 0.0 {
            continue;
        }
        let sid_raw = hydrology.sink_id.get(cell).copied().unwrap_or(-1);
        if sid_raw < 0 {
            continue;
        }
        let sid = sid_raw as usize;
        if hydrology
            .sink_overflow_active
            .get(sid)
            .copied()
            .unwrap_or(1)
            != 0
        {
            continue;
        }
        let spill_level = hydrology
            .sink_spill_level
            .get(sid)
            .copied()
            .unwrap_or(f32::INFINITY);
        if height[cell] <= spill_level + hysteresis {
            hydrology.is_lake[cell] = true;
        }
    }
}

pub(crate) fn apply_fill_spill_sink_rule_to_erosion_cell(
    state: &mut ErosionAutomatonState,
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

    let spill_level = state
        .sink_spill_level
        .get(sid)
        .copied()
        .unwrap_or(f32::INFINITY);
    let hysteresis = state.params.sink_overflow_hysteresis.max(0.0);
    if state.height[cell] > spill_level + hysteresis {
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
                *sediment = (*sediment - capture).max(0.0);
                state.sink_capacity_remaining[sid] = (remain - capture).max(0.0);
                state.sink_storage_sediment[sid] += capture;
            }
        }

        if state.sink_capacity_remaining[sid] <= hysteresis {
            state.sink_overflow_active[sid] = 1;
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

fn reset_fill_spill(hydrology: &mut HydrologyState, cell_count: usize) {
    hydrology.sink_id = vec![-1; cell_count];
    hydrology.sink_route_next = vec![-1; cell_count];
    hydrology.sink_member_offsets = vec![0];
    hydrology.sink_member_cells.clear();
    hydrology.sink_spill_cell.clear();
    hydrology.sink_spill_to.clear();
    hydrology.sink_spill_level.clear();
    hydrology.sink_capacity_total.clear();
    hydrology.sink_capacity_remaining.clear();
    hydrology.sink_storage_water.clear();
    hydrology.sink_storage_sediment.clear();
    hydrology.sink_overflow_active.clear();
}

fn snapshot_sink_state(hydrology: &HydrologyState) -> HashMap<(i32, i32), (f32, f32, f32, u8)> {
    let mut old_state = HashMap::new();
    for sid in 0..hydrology.sink_spill_cell.len() {
        old_state.insert(
            (
                hydrology.sink_spill_cell[sid],
                hydrology.sink_spill_to.get(sid).copied().unwrap_or(-1),
            ),
            (
                hydrology
                    .sink_capacity_remaining
                    .get(sid)
                    .copied()
                    .unwrap_or(0.0),
                hydrology
                    .sink_storage_water
                    .get(sid)
                    .copied()
                    .unwrap_or(0.0),
                hydrology
                    .sink_storage_sediment
                    .get(sid)
                    .copied()
                    .unwrap_or(0.0),
                hydrology
                    .sink_overflow_active
                    .get(sid)
                    .copied()
                    .unwrap_or(0),
            ),
        );
    }
    old_state
}

fn resize_sink_state_arrays(hydrology: &mut HydrologyState, sink_count: usize) {
    hydrology.sink_spill_cell = vec![-1; sink_count];
    hydrology.sink_spill_to = vec![-1; sink_count];
    hydrology.sink_spill_level = vec![0.0; sink_count];
    hydrology.sink_capacity_total = vec![0.0; sink_count];
    hydrology.sink_capacity_remaining = vec![0.0; sink_count];
    hydrology.sink_storage_water = vec![0.0; sink_count];
    hydrology.sink_storage_sediment = vec![0.0; sink_count];
    hydrology.sink_overflow_active = vec![0; sink_count];
}

fn rebuild_membership_csr(hydrology: &mut HydrologyState, sink_members: &[Vec<usize>]) {
    hydrology.sink_member_offsets.clear();
    hydrology.sink_member_cells.clear();
    hydrology
        .sink_member_offsets
        .reserve(sink_members.len() + 1);
    hydrology.sink_member_offsets.push(0);
    for members in sink_members {
        for &cell in members {
            hydrology.sink_member_cells.push(cell as u32);
        }
        hydrology
            .sink_member_offsets
            .push(hydrology.sink_member_cells.len() as u32);
    }
}

fn compute_downhill_links(height: &[f32], nbr_offsets: &[u32], nbrs: &[u32]) -> Vec<i32> {
    let mut downhill = vec![-1; height.len()];
    for i in 0..height.len() {
        if height[i] <= 0.0 {
            continue;
        }
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let mut best = -1;
        let mut best_height = height[i];
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            let nh = height.get(n).copied().unwrap_or(height[i]);
            if nh + 1e-6 < best_height {
                best_height = nh;
                best = n as i32;
            }
        }
        downhill[i] = best;
    }
    downhill
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

fn build_sink_members(height: &[f32], terminal: &[i32], sink_id: &mut [i32]) -> Vec<Vec<usize>> {
    let mut root_to_sink = HashMap::<usize, usize>::new();
    let mut sink_members = Vec::<Vec<usize>>::new();
    for (cell, &root) in terminal.iter().enumerate() {
        if root < 0 {
            continue;
        }
        let root_index = root as usize;
        if height.get(root_index).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let sid = *root_to_sink.entry(root_index).or_insert_with(|| {
            sink_members.push(Vec::new());
            sink_members.len() - 1
        });
        sink_id[cell] = sid as i32;
        sink_members[sid].push(cell);
    }
    sink_members
}

#[allow(clippy::too_many_arguments)]
fn update_sink_for_sid(
    hydrology: &mut HydrologyState,
    sid: usize,
    members: &[usize],
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    params: &GeologyParams,
    old_state: &HashMap<(i32, i32), (f32, f32, f32, u8)>,
) {
    if members.is_empty() {
        hydrology.sink_overflow_active[sid] = 1;
        return;
    }

    let (spill_level, spill_from, spill_to) =
        find_sink_spill_edge(height, nbr_offsets, nbrs, &hydrology.sink_id, sid, members);
    hydrology.sink_spill_cell[sid] = spill_from;
    hydrology.sink_spill_to[sid] = spill_to;
    hydrology.sink_spill_level[sid] = spill_level;

    if spill_from < 0 {
        hydrology.sink_overflow_active[sid] = 1;
        return;
    }

    let cap = sink_capacity(height, members, spill_level, params.sink_min_capacity);
    hydrology.sink_capacity_total[sid] = cap;
    if let Some((remain, water, sediment, active)) = old_state.get(&(spill_from, spill_to)) {
        hydrology.sink_capacity_remaining[sid] = remain.clamp(0.0, cap);
        hydrology.sink_storage_water[sid] = water.max(0.0);
        hydrology.sink_storage_sediment[sid] = sediment.max(0.0);
        hydrology.sink_overflow_active[sid] = *active;
    } else {
        hydrology.sink_capacity_remaining[sid] = cap;
        hydrology.sink_storage_water[sid] = 0.0;
        hydrology.sink_storage_sediment[sid] = 0.0;
        hydrology.sink_overflow_active[sid] = 0;
    }

    rebuild_sink_route_for_sid(hydrology, sid, members, height, nbr_offsets, nbrs);
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
    let mut best_from = -1;
    let mut best_to = -1;
    for &cell in members {
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if sink_id.get(n).copied().unwrap_or(-1) == sid as i32 {
                continue;
            }
            let cand_level = height[cell].max(height.get(n).copied().unwrap_or(height[cell]));
            if cand_level + 1e-6 < best_level {
                best_level = cand_level;
                best_from = cell as i32;
                best_to = n as i32;
            }
        }
    }
    (best_level, best_from, best_to)
}

fn sink_capacity(
    height: &[f32],
    members: &[usize],
    spill_level: f32,
    sink_min_capacity: f32,
) -> f32 {
    let mut capacity = 0.0;
    for &cell in members {
        capacity += (spill_level - height[cell]).max(0.0);
    }
    capacity.max(sink_min_capacity.max(0.0))
}

fn rebuild_sink_route_for_sid(
    hydrology: &mut HydrologyState,
    sid: usize,
    members: &[usize],
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) {
    let spill_from = hydrology.sink_spill_cell[sid];
    for &cell in members {
        hydrology.sink_route_next[cell] = -1;
    }
    if spill_from < 0 {
        return;
    }

    let source = spill_from as usize;
    let mut is_member = vec![0u8; height.len()];
    for &cell in members {
        is_member[cell] = 1;
    }

    let mut dist = vec![f32::INFINITY; height.len()];
    let mut heap = BinaryHeap::<RouteState>::new();
    dist[source] = 0.0;
    heap.push(RouteState {
        vertex: source,
        cost: 0.0,
        steps: 0,
    });

    while let Some(cur) = heap.pop() {
        let v = cur.vertex;
        if cur.cost > dist[v] + 1e-6 {
            continue;
        }
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if is_member[n] == 0 {
                continue;
            }
            let uphill = (height[n] - height[v]).max(0.0);
            let cand = cur.cost + 1.0 + uphill * 8.0;
            if cand + 1e-6 < dist[n] {
                dist[n] = cand;
                heap.push(RouteState {
                    vertex: n,
                    cost: cand,
                    steps: cur.steps.saturating_add(1),
                });
            }
        }
    }

    for &cell in members {
        if cell == source || !dist[cell].is_finite() {
            continue;
        }
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        let mut best = -1;
        let mut best_dist = dist[cell];
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if is_member[n] == 0 {
                continue;
            }
            if dist[n] + 1e-6 < best_dist {
                best_dist = dist[n];
                best = n as i32;
            }
        }
        hydrology.sink_route_next[cell] = best;
    }
}

fn recompute_sink_storage_water(
    hydrology: &mut HydrologyState,
    height: &[f32],
    water: Option<&[f32]>,
    params: &GeologyParams,
) {
    if hydrology.sink_storage_water.len() != hydrology.sink_spill_cell.len() {
        hydrology.sink_storage_water = vec![0.0; hydrology.sink_spill_cell.len()];
    } else {
        hydrology.sink_storage_water.fill(0.0);
    }
    let Some(water) = water else {
        return;
    };
    let hysteresis = params.sink_overflow_hysteresis.max(0.0);
    for cell in 0..height.len().min(water.len()).min(hydrology.sink_id.len()) {
        let sid_raw = hydrology.sink_id[cell];
        if sid_raw < 0 {
            continue;
        }
        let sid = sid_raw as usize;
        if sid >= hydrology.sink_storage_water.len() {
            continue;
        }
        if hydrology
            .sink_overflow_active
            .get(sid)
            .copied()
            .unwrap_or(1)
            != 0
        {
            continue;
        }
        let spill_level = hydrology
            .sink_spill_level
            .get(sid)
            .copied()
            .unwrap_or(f32::INFINITY);
        if height[cell] <= spill_level + hysteresis {
            hydrology.sink_storage_water[sid] += water[cell].max(0.0);
        }
    }
}

fn recompute_sink_storage_sediment(
    hydrology: &mut HydrologyState,
    height: &[f32],
    sediment: Option<&[f32]>,
    params: &GeologyParams,
) {
    let Some(sediment) = sediment else {
        return;
    };
    if hydrology.sink_storage_sediment.len() != hydrology.sink_spill_cell.len() {
        hydrology.sink_storage_sediment = vec![0.0; hydrology.sink_spill_cell.len()];
    } else {
        hydrology.sink_storage_sediment.fill(0.0);
    }
    let hysteresis = params.sink_overflow_hysteresis.max(0.0);
    for cell in 0..height
        .len()
        .min(sediment.len())
        .min(hydrology.sink_id.len())
    {
        let sid_raw = hydrology.sink_id[cell];
        if sid_raw < 0 {
            continue;
        }
        let sid = sid_raw as usize;
        if sid >= hydrology.sink_storage_sediment.len() {
            continue;
        }
        if hydrology
            .sink_overflow_active
            .get(sid)
            .copied()
            .unwrap_or(1)
            != 0
        {
            continue;
        }
        let spill_level = hydrology
            .sink_spill_level
            .get(sid)
            .copied()
            .unwrap_or(f32::INFINITY);
        if height[cell] <= spill_level + hysteresis {
            hydrology.sink_storage_sediment[sid] += sediment[cell].max(0.0);
        }
    }
}

fn validate_fill_spill_topology(hydrology: &HydrologyState, cell_count: usize) -> bool {
    if !fill_spill_buffers_ready(hydrology, cell_count) {
        return false;
    }
    for &sid_raw in &hydrology.sink_id {
        if sid_raw < -1 {
            return false;
        }
        if sid_raw >= 0 && (sid_raw as usize) >= hydrology.sink_spill_cell.len() {
            return false;
        }
    }
    for &next in &hydrology.sink_route_next {
        if next < -1 {
            return false;
        }
        if next >= 0 && (next as usize) >= cell_count {
            return false;
        }
    }
    true
}

fn mark_neighbors_within_hops(
    start: usize,
    hops: u32,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    mark: &mut [u8],
) {
    if start >= mark.len() {
        return;
    }
    if mark[start] == 0 {
        mark[start] = 1;
    }
    let mut frontier = vec![start];
    for _ in 0..hops {
        let mut next_frontier = Vec::<usize>::new();
        for &cell in &frontier {
            let begin = nbr_offsets.get(cell).copied().unwrap_or(0) as usize;
            let end = nbr_offsets.get(cell + 1).copied().unwrap_or(begin as u32) as usize;
            for &n_u32 in &nbrs[begin.min(nbrs.len())..end.min(nbrs.len())] {
                let n = n_u32 as usize;
                if n >= mark.len() || mark[n] != 0 {
                    continue;
                }
                mark[n] = 1;
                next_frontier.push(n);
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }
}

fn sink_members_for_sid(hydrology: &HydrologyState, sid: usize) -> Vec<usize> {
    if sid + 1 >= hydrology.sink_member_offsets.len() {
        return Vec::new();
    }
    let begin = hydrology.sink_member_offsets[sid] as usize;
    let end = hydrology.sink_member_offsets[sid + 1] as usize;
    if begin >= end || end > hydrology.sink_member_cells.len() {
        return Vec::new();
    }
    let mut members = Vec::with_capacity(end - begin);
    for &cell in &hydrology.sink_member_cells[begin..end] {
        members.push(cell as usize);
    }
    members
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    fn test_hydrology(cell_count: usize) -> HydrologyState {
        HydrologyState {
            river_downstream: vec![SmallVec::new(); cell_count],
            river_next: vec![-1; cell_count],
            river_flow: vec![0.0; cell_count],
            erosion_rate: vec![0.0; cell_count],
            deposition_rate: vec![0.0; cell_count],
            river_transport_cost: vec![1.0; cell_count],
            is_lake: vec![false; cell_count],
            sink_id: vec![-1; cell_count],
            sink_route_next: vec![-1; cell_count],
            sink_member_offsets: vec![0],
            sink_member_cells: Vec::new(),
            sink_spill_cell: Vec::new(),
            sink_spill_to: Vec::new(),
            sink_spill_level: Vec::new(),
            sink_capacity_total: Vec::new(),
            sink_capacity_remaining: Vec::new(),
            sink_storage_water: Vec::new(),
            sink_storage_sediment: Vec::new(),
            sink_overflow_active: Vec::new(),
        }
    }

    #[test]
    fn rebuild_fill_spill_state_detects_sink_and_spill_edge() {
        let mut hydrology = test_hydrology(5);
        let height = vec![0.8, 0.2, 0.7, 0.6, -0.1];
        let nbr_offsets = vec![0, 1, 4, 5, 7, 8];
        let nbrs = vec![1, 0, 2, 3, 1, 1, 4, 3];
        let params = GeologyParams::default();

        rebuild_fill_spill_state(
            &mut hydrology,
            &height,
            &nbr_offsets,
            &nbrs,
            &params,
            None,
            None,
        );

        assert_eq!(hydrology.sink_spill_cell.len(), 1);
        assert_eq!(hydrology.sink_spill_cell[0], 1);
        assert_eq!(hydrology.sink_spill_to[0], 3);
        assert!((hydrology.sink_spill_level[0] - 0.6).abs() <= 1e-6);
        assert_eq!(hydrology.sink_id, vec![0, 0, 0, -1, -1]);
        assert_eq!(hydrology.sink_route_next[0], 1);
        assert_eq!(hydrology.sink_route_next[2], 1);
    }

    #[test]
    fn update_public_lake_flags_marks_only_ponded_cells() {
        let mut hydrology = test_hydrology(5);
        let height = vec![0.8, 0.2, 0.7, 0.6, -0.1];
        let nbr_offsets = vec![0, 1, 4, 5, 7, 8];
        let nbrs = vec![1, 0, 2, 3, 1, 1, 4, 3];
        let params = GeologyParams::default();

        rebuild_fill_spill_state(
            &mut hydrology,
            &height,
            &nbr_offsets,
            &nbrs,
            &params,
            None,
            None,
        );
        update_public_lake_flags(&mut hydrology, &height, &params);
        assert_eq!(hydrology.is_lake, vec![false, true, false, false, false]);

        hydrology.sink_overflow_active[0] = 1;
        update_public_lake_flags(&mut hydrology, &height, &params);
        assert_eq!(hydrology.is_lake, vec![false; 5]);
    }
}
