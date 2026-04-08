use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::state::World;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldMetrics {
    pub cell_count: u32,
    pub land_cells: u32,
    pub land_ratio: f32,
    pub mean_height: f32,
    pub height_std_dev: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub mean_river_flux: f32,
    pub max_river_flux: f32,
    pub top10_river_flux_sum: f32,
    pub river_active_cells: u32,
    pub river_fragmentation_ratio: f32,
    pub river_ocean_reach_ratio: f32,
    pub river_mainstem_persistence: f32,
    pub river_flux_concentration: f32,
    pub continent_count: u32,
    pub largest_continent_cells: u32,
}

impl World {
    pub fn metrics(&self) -> WorldMetrics {
        let cells = self.cell_store();
        let cell_count = cells.len();
        if cells.is_empty() {
            return WorldMetrics::default();
        }

        let mut land_cells = 0usize;
        let mut min_height = f32::INFINITY;
        let mut max_height = f32::NEG_INFINITY;
        let mut sum_height = 0.0f32;
        let mut sum_height_sq = 0.0f32;
        let mut sum_flux = 0.0f32;
        let mut max_flux = 0.0f32;
        let mut top_fluxes = [0.0f32; 10];
        let mut top_fluxes_len = 0usize;

        for (i, &h) in cells.height.iter().enumerate().take(cell_count) {
            let flux = cells.river_flow.get(i).copied().unwrap_or(0.0).max(0.0);
            if cells.is_land_cell(i, self.sea_level_offset()) {
                land_cells += 1;
            }
            min_height = min_height.min(h);
            max_height = max_height.max(h);
            sum_height += h;
            sum_height_sq += h * h;
            sum_flux += flux;
            max_flux = max_flux.max(flux);
            push_top_flux(&mut top_fluxes, &mut top_fluxes_len, flux);
        }

        let cell_count_f32 = cell_count as f32;
        let mean_height = sum_height / cell_count_f32;
        let variance = (sum_height_sq / cell_count_f32) - (mean_height * mean_height);
        let height_std_dev = variance.max(0.0).sqrt();

        let (continent_count, largest_continent_cells) = continent_stats(self);
        let top10_river_flux_sum = top_fluxes.iter().take(top_fluxes_len).sum::<f32>();
        let (
            river_active_cells,
            river_fragmentation_ratio,
            river_ocean_reach_ratio,
            river_mainstem_persistence,
            river_flux_concentration,
        ) = river_network_metrics(
            cells.height,
            cells.river_flow,
            cells.river_next,
            top10_river_flux_sum,
            sum_flux,
            max_flux,
        );

        WorldMetrics {
            cell_count: cell_count as u32,
            land_cells: land_cells as u32,
            land_ratio: land_cells as f32 / cell_count_f32,
            mean_height,
            height_std_dev,
            min_height: if min_height.is_finite() {
                min_height
            } else {
                0.0
            },
            max_height: if max_height.is_finite() {
                max_height
            } else {
                0.0
            },
            mean_river_flux: sum_flux / cell_count_f32,
            max_river_flux: max_flux,
            top10_river_flux_sum,
            river_active_cells,
            river_fragmentation_ratio,
            river_ocean_reach_ratio,
            river_mainstem_persistence,
            river_flux_concentration,
            continent_count: continent_count as u32,
            largest_continent_cells: largest_continent_cells as u32,
        }
    }
}

fn push_top_flux(top_fluxes: &mut [f32; 10], len: &mut usize, value: f32) {
    if !value.is_finite() || value <= 0.0 {
        return;
    }
    if *len < top_fluxes.len() {
        top_fluxes[*len] = value;
        *len += 1;
        return;
    }
    let mut min_index = 0usize;
    for i in 1..top_fluxes.len() {
        if top_fluxes[i] < top_fluxes[min_index] {
            min_index = i;
        }
    }
    if value > top_fluxes[min_index] {
        top_fluxes[min_index] = value;
    }
}

fn continent_stats(world: &World) -> (usize, usize) {
    let cells = world.cell_store();
    let cell_count = cells.len();
    if cells.is_empty() {
        return (0, 0);
    }
    let min_continent_cells = ((cell_count as f32) * 0.01).ceil().max(1.0) as usize;
    let mut visited = vec![false; cell_count];
    let mut queue = VecDeque::new();
    let mut continent_count = 0usize;
    let mut largest_continent_cells = 0usize;

    for start_index in 0..cell_count {
        if visited[start_index] || !cells.is_land_cell(start_index, world.sea_level_offset()) {
            continue;
        }
        visited[start_index] = true;
        queue.clear();
        queue.push_back(start_index);
        let mut component_size = 0usize;

        while let Some(index) = queue.pop_front() {
            component_size += 1;
            for &neighbor in cells.cell_neighbors(index) {
                let neighbor_index = neighbor as usize;
                if neighbor_index >= cell_count
                    || visited[neighbor_index]
                    || !cells.is_land_cell(neighbor_index, world.sea_level_offset())
                {
                    continue;
                }
                visited[neighbor_index] = true;
                queue.push_back(neighbor_index);
            }
        }

        if component_size >= min_continent_cells {
            continent_count += 1;
            largest_continent_cells = largest_continent_cells.max(component_size);
        }
    }

    (continent_count, largest_continent_cells)
}

fn river_network_metrics(
    height: &[f32],
    flux: &[f32],
    river_next: &[i32],
    top10_river_flux_sum: f32,
    sum_flux: f32,
    max_flux: f32,
) -> (u32, f32, f32, f32, f32) {
    let cell_count = height.len();
    if cell_count == 0 || river_next.len() != cell_count {
        return (0, 0.0, 0.0, 0.0, 0.0);
    }

    let active_threshold = (max_flux * 0.08).max(0.02);
    let mut active = vec![false; cell_count];
    let mut active_cells = 0usize;
    for i in 0..cell_count {
        if height[i] > 0.0 && flux.get(i).copied().unwrap_or(0.0) >= active_threshold {
            active[i] = true;
            active_cells += 1;
        }
    }
    if active_cells == 0 {
        return (0, 0.0, 0.0, 0.0, 0.0);
    }

    let mut upstream_active = vec![0u32; cell_count];
    for i in 0..cell_count {
        if !active[i] {
            continue;
        }
        let next = river_next[i];
        if next < 0 {
            continue;
        }
        let n = next as usize;
        if n < cell_count && active[n] {
            upstream_active[n] = upstream_active[n].saturating_add(1);
        }
    }

    let mut head_cells = Vec::new();
    for i in 0..cell_count {
        if active[i] && upstream_active[i] == 0 {
            head_cells.push(i);
        }
    }
    let fragmentation_ratio = head_cells.len() as f32 / active_cells as f32;

    let mut memo = vec![0u8; cell_count];
    let mut visit_mark = vec![0u32; cell_count];
    let mut run_id = 1u32;
    let mut reaches_ocean_count = 0usize;
    let mut path = Vec::<usize>::new();
    let mut trace_context = RiverTraceContext {
        height,
        river_next,
        active: &active,
        memo: &mut memo,
        visit_mark: &mut visit_mark,
        run_id: &mut run_id,
        path: &mut path,
    };

    for (i, &is_active) in active.iter().enumerate().take(cell_count) {
        if !is_active {
            continue;
        }
        if trace_active_to_ocean(i, &mut trace_context) {
            reaches_ocean_count += 1;
        }
    }

    let mut longest_mainstem = 0usize;
    for &head in &head_cells {
        let mut steps = 0usize;
        let mut current = head;
        let mut guard = 0usize;
        while guard < cell_count {
            guard += 1;
            steps += 1;
            let next = river_next.get(current).copied().unwrap_or(-1);
            if next < 0 {
                break;
            }
            let n = next as usize;
            if n >= cell_count || !active[n] {
                break;
            }
            if n == current {
                break;
            }
            current = n;
        }
        longest_mainstem = longest_mainstem.max(steps);
    }

    let ocean_reach_ratio = reaches_ocean_count as f32 / active_cells as f32;
    let mainstem_persistence = longest_mainstem as f32 / active_cells as f32;
    let flux_concentration = top10_river_flux_sum / sum_flux.max(1e-6);

    (
        active_cells as u32,
        fragmentation_ratio,
        ocean_reach_ratio,
        mainstem_persistence,
        flux_concentration.clamp(0.0, 1.0),
    )
}

fn trace_active_to_ocean(start: usize, context: &mut RiverTraceContext<'_>) -> bool {
    if context.memo.get(start).copied().unwrap_or(0) == 2 {
        return true;
    }
    if context.memo.get(start).copied().unwrap_or(0) == 3 {
        return false;
    }

    context.path.clear();
    let current_run = *context.run_id;
    *context.run_id = (*context.run_id).saturating_add(1).max(1);
    let mut cur = start;
    let mut result = false;

    for _ in 0..context.height.len() {
        if context.memo[cur] == 2 {
            result = true;
            break;
        }
        if context.memo[cur] == 3 || !context.active[cur] {
            result = false;
            break;
        }
        if context.visit_mark[cur] == current_run {
            result = false;
            break;
        }
        context.visit_mark[cur] = current_run;
        context.path.push(cur);

        let next = context.river_next.get(cur).copied().unwrap_or(-1);
        if next < 0 {
            result = false;
            break;
        }
        let n = next as usize;
        if n >= context.height.len() {
            result = false;
            break;
        }
        if context.height[n] <= 0.0 {
            result = true;
            break;
        }
        cur = n;
    }

    let mark = if result { 2 } else { 3 };
    for &v in context.path.iter() {
        context.memo[v] = mark;
    }
    result
}

struct RiverTraceContext<'a> {
    height: &'a [f32],
    river_next: &'a [i32],
    active: &'a [bool],
    memo: &'a mut [u8],
    visit_mark: &'a mut [u32],
    run_id: &'a mut u32,
    path: &'a mut Vec<usize>,
}
