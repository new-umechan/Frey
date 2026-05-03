use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::state::World;
use crate::sim::geology_types::CrustType;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldMetrics {
    pub cell_count: u32,
    pub land_cells: u32,
    pub land_ratio: f32,
    pub sea_level_offset: f32,
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
    pub global_sediment_export: f32,
    pub marine_sediment_mass: f32,
    pub solid_earth_mass_proxy: f32,
    pub solid_earth_mass_proxy_drift: f32,
    pub ocean_water_inventory: f32,
    pub ocean_water_inventory_drift: f32,
    pub ice_inventory: f32,
    pub smoothing_limited_cells_ratio: f32,
    pub mean_smoothing_factor: f32,
    pub zero_mean_adjusted_cells_ratio: f32,
    pub zero_mean_mean_abs_correction: f32,
    pub zero_mean_std_delta: f32,
    pub geology_activity: f32,
    pub boundary_activity: f32,
    pub uplift_rate: f32,
    pub subsidence_rate: f32,
    pub mean_compressive: f32,
    pub mean_tensile: f32,
    pub mean_abs_diffusive_raw: f32,
    pub mean_abs_isostatic_raw: f32,
    pub mean_thickness: f32,
    pub std_thickness: f32,
    pub mean_density: f32,
    pub std_density: f32,
    pub mean_rigidity: f32,
    pub std_rigidity: f32,
    pub oceanic_cell_ratio: f32,
    pub continental_cell_ratio: f32,
    pub mean_thickness_oceanic: f32,
    pub mean_thickness_continental: f32,
    pub mean_rigidity_oceanic: f32,
    pub mean_rigidity_continental: f32,
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
        let cached_geology = self.exec_scratch.geology_dynamics.as_ref().map(|state| {
            let metrics = state.cached_metrics;
            (
                finite_or(metrics.geology_activity),
                finite_or(metrics.boundary_activity),
                finite_or(metrics.uplift_rate),
                finite_or(metrics.subsidence_rate),
                finite_or(metrics.mean_compressive),
                finite_or(metrics.mean_tensile),
                finite_or(metrics.mean_abs_diffusive_raw),
                finite_or(metrics.mean_abs_isostatic_raw),
            )
        });
        let (
            mean_thickness,
            std_thickness,
            mean_density,
            std_density,
            mean_rigidity,
            std_rigidity,
            oceanic_cell_ratio,
            continental_cell_ratio,
            mean_thickness_oceanic,
            mean_thickness_continental,
            mean_rigidity_oceanic,
            mean_rigidity_continental,
        ) = geology_internal_stats(&self.state.geology.geology_internal);

        WorldMetrics {
            cell_count: cell_count as u32,
            land_cells: land_cells as u32,
            land_ratio: land_cells as f32 / cell_count_f32,
            sea_level_offset: self.control.sea_level_offset,
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
            global_sediment_export: self.control.global_sediment_export.max(0.0),
            marine_sediment_mass: self.control.marine_sediment_mass.max(0.0),
            solid_earth_mass_proxy: self.control.solid_earth_mass_proxy,
            solid_earth_mass_proxy_drift: self.control.solid_earth_mass_proxy
                - self.control.solid_earth_mass_proxy_baseline,
            ocean_water_inventory: self.control.ocean_water_inventory.max(0.0),
            ocean_water_inventory_drift: self.control.ocean_water_inventory
                - self.control.ocean_water_inventory_baseline,
            ice_inventory: self.control.ice_inventory.max(0.0),
            smoothing_limited_cells_ratio: self.state.geology.smoothing_limited_cells_ratio,
            mean_smoothing_factor: self.state.geology.mean_smoothing_factor,
            zero_mean_adjusted_cells_ratio: self.state.geology.zero_mean_adjusted_cells_ratio,
            zero_mean_mean_abs_correction: self.state.geology.zero_mean_mean_abs_correction,
            zero_mean_std_delta: self.state.geology.zero_mean_std_delta,
            geology_activity: cached_geology.map(|values| values.0).unwrap_or(0.0),
            boundary_activity: cached_geology.map(|values| values.1).unwrap_or(0.0),
            uplift_rate: cached_geology.map(|values| values.2).unwrap_or(0.0),
            subsidence_rate: cached_geology.map(|values| values.3).unwrap_or(0.0),
            mean_compressive: cached_geology.map(|values| values.4).unwrap_or(0.0),
            mean_tensile: cached_geology.map(|values| values.5).unwrap_or(0.0),
            mean_abs_diffusive_raw: cached_geology.map(|values| values.6).unwrap_or(0.0),
            mean_abs_isostatic_raw: cached_geology.map(|values| values.7).unwrap_or(0.0),
            mean_thickness,
            std_thickness,
            mean_density,
            std_density,
            mean_rigidity,
            std_rigidity,
            oceanic_cell_ratio,
            continental_cell_ratio,
            mean_thickness_oceanic,
            mean_thickness_continental,
            mean_rigidity_oceanic,
            mean_rigidity_continental,
        }
    }
}

fn finite_or(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn geology_internal_stats(
    values: &[crate::sim::geology_types::GeologyInternal],
) -> (f32, f32, f32, f32, f32, f32, f32, f32, f32, f32, f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    }

    let len = values.len() as f32;
    let mean_thickness = values
        .iter()
        .map(|value| finite_or(value.thickness))
        .sum::<f32>()
        / len;
    let mean_density = values
        .iter()
        .map(|value| finite_or(value.density))
        .sum::<f32>()
        / len;
    let mean_rigidity = values
        .iter()
        .map(|value| finite_or(value.rigidity))
        .sum::<f32>()
        / len;

    let std_thickness = std_from_mean(
        values.iter().map(|value| finite_or(value.thickness)),
        mean_thickness,
        len,
    );
    let std_density = std_from_mean(
        values.iter().map(|value| finite_or(value.density)),
        mean_density,
        len,
    );
    let std_rigidity = std_from_mean(
        values.iter().map(|value| finite_or(value.rigidity)),
        mean_rigidity,
        len,
    );

    let mut oceanic_count = 0usize;
    let mut oceanic_thickness_sum = 0.0f32;
    let mut oceanic_rigidity_sum = 0.0f32;
    let mut continental_count = 0usize;
    let mut continental_thickness_sum = 0.0f32;
    let mut continental_rigidity_sum = 0.0f32;
    for value in values {
        match value.crust_type {
            CrustType::Oceanic => {
                oceanic_count += 1;
                oceanic_thickness_sum += finite_or(value.thickness);
                oceanic_rigidity_sum += finite_or(value.rigidity);
            }
            CrustType::Continental => {
                continental_count += 1;
                continental_thickness_sum += finite_or(value.thickness);
                continental_rigidity_sum += finite_or(value.rigidity);
            }
        }
    }
    let total = values.len() as f32;
    let oceanic_count_f32 = oceanic_count as f32;
    let continental_count_f32 = continental_count as f32;
    let mean_thickness_oceanic = if oceanic_count > 0 {
        oceanic_thickness_sum / oceanic_count_f32
    } else {
        0.0
    };
    let mean_thickness_continental = if continental_count > 0 {
        continental_thickness_sum / continental_count_f32
    } else {
        0.0
    };
    let mean_rigidity_oceanic = if oceanic_count > 0 {
        oceanic_rigidity_sum / oceanic_count_f32
    } else {
        0.0
    };
    let mean_rigidity_continental = if continental_count > 0 {
        continental_rigidity_sum / continental_count_f32
    } else {
        0.0
    };

    (
        mean_thickness,
        std_thickness,
        mean_density,
        std_density,
        mean_rigidity,
        std_rigidity,
        oceanic_count_f32 / total,
        continental_count_f32 / total,
        mean_thickness_oceanic,
        mean_thickness_continental,
        mean_rigidity_oceanic,
        mean_rigidity_continental,
    )
}

fn std_from_mean<I>(values: I, mean: f32, len: f32) -> f32
where
    I: Iterator<Item = f32>,
{
    let variance = values
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f32>()
        / len;
    variance.max(0.0).sqrt()
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
