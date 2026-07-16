use serde::{Deserialize, Serialize};

pub const SCIENTIFIC_BENCHMARK_SAMPLE_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMode {
    #[default]
    Interactive,
    HeadlessMetrics,
    ScientificBenchmark,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PostStepProfile {
    pub step_sync_erosion_ms: f64,
    pub step_observe_world_change_ms: f64,
    pub step_history_snapshot_ms: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeadlessMetrics {
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

pub trait PostStepRuntime {
    fn verification_mode(&self) -> VerificationMode;
    fn sync_light(&mut self);
    fn observe_after_world_change(&mut self);
    fn save_snapshot_if_needed(&mut self);
    fn refresh_reduced_metrics(&mut self);
    fn push_scientific_benchmark_sample(&mut self);
}

pub trait ProfileClock {
    type Stamp;

    fn now(&self) -> Self::Stamp;
    fn elapsed_ms(&self, start: Self::Stamp) -> f64;
}

pub fn run_post_step(runtime: &mut impl PostStepRuntime) {
    match runtime.verification_mode() {
        VerificationMode::Interactive => {
            runtime.sync_light();
            runtime.observe_after_world_change();
            runtime.save_snapshot_if_needed();
        }
        VerificationMode::HeadlessMetrics => {
            runtime.refresh_reduced_metrics();
        }
        VerificationMode::ScientificBenchmark => {
            runtime.sync_light();
            runtime.observe_after_world_change();
            runtime.save_snapshot_if_needed();
            runtime.push_scientific_benchmark_sample();
        }
    }
}

pub fn run_post_step_profiled<C: ProfileClock>(
    runtime: &mut impl PostStepRuntime,
    clock: &C,
) -> PostStepProfile {
    let mode = runtime.verification_mode();
    match mode {
        VerificationMode::Interactive | VerificationMode::ScientificBenchmark => {
            let phase_start = clock.now();
            runtime.sync_light();
            let step_sync_erosion_ms = clock.elapsed_ms(phase_start);

            let phase_start = clock.now();
            runtime.observe_after_world_change();
            let step_observe_world_change_ms = clock.elapsed_ms(phase_start);

            let phase_start = clock.now();
            runtime.save_snapshot_if_needed();
            let step_history_snapshot_ms = clock.elapsed_ms(phase_start);

            if mode == VerificationMode::ScientificBenchmark {
                runtime.push_scientific_benchmark_sample();
            }

            PostStepProfile {
                step_sync_erosion_ms,
                step_observe_world_change_ms,
                step_history_snapshot_ms,
            }
        }
        VerificationMode::HeadlessMetrics => {
            let phase_start = clock.now();
            runtime.refresh_reduced_metrics();
            let step_reduce_metrics_ms = clock.elapsed_ms(phase_start);
            PostStepProfile {
                step_sync_erosion_ms: 0.0,
                step_observe_world_change_ms: step_reduce_metrics_ms,
                step_history_snapshot_ms: 0.0,
            }
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

pub fn reduce_metrics_for_headless(
    height: &[f32],
    river_flow: &[f32],
    sea_level: f32,
) -> HeadlessMetrics {
    let cell_count = height.len().min(river_flow.len());
    if cell_count == 0 {
        return HeadlessMetrics::default();
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

    for index in 0..cell_count {
        let cell_height = height[index];
        let flux = river_flow[index].max(0.0);
        if cell_height > sea_level {
            land_cells += 1;
        }
        min_height = min_height.min(cell_height);
        max_height = max_height.max(cell_height);
        sum_height += cell_height;
        sum_height_sq += cell_height * cell_height;
        sum_flux += flux;
        max_flux = max_flux.max(flux);
        push_top_flux(&mut top_fluxes, &mut top_fluxes_len, flux);
    }

    let cell_count_f32 = cell_count as f32;
    let mean_height = sum_height / cell_count_f32;
    let variance = (sum_height_sq / cell_count_f32) - (mean_height * mean_height);

    HeadlessMetrics {
        cell_count: cell_count as u32,
        land_cells: land_cells as u32,
        land_ratio: land_cells as f32 / cell_count_f32,
        mean_height,
        height_std_dev: variance.max(0.0).sqrt(),
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
        top10_river_flux_sum: top_fluxes.iter().take(top_fluxes_len).sum::<f32>(),
        river_active_cells: 0,
        river_fragmentation_ratio: 0.0,
        river_ocean_reach_ratio: 0.0,
        river_mainstem_persistence: 0.0,
        river_flux_concentration: 0.0,
        continent_count: 0,
        largest_continent_cells: 0,
    }
}
