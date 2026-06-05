use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::erosion::ErosionAutomatonState;
use frey_wasm::sim::glaciology::types::GlaciologyParams as BenchGlaciologyParams;
use frey_wasm::sim::world::FeedbackQueue;
use frey_wasm::sim::world::World;
use frey_wasm::sim::ExecWorldPhase;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";
const DEFAULT_RUN_TICKS: u64 = 900;
const DEFAULT_RECORD_START_TICK: u64 = 780;
const DEFAULT_RECORD_END_TICK: u64 = 900;
const DEFAULT_LAND_RATIO_MIN: f32 = 0.15;
const DEFAULT_LAND_RATIO_MAX: f32 = 0.85;
const DEFAULT_MAX_LAND_RATIO_JUMP: f32 = 0.03;
const DEFAULT_MAX_SEA_LEVEL_JUMP: f32 = 0.08;
const DEFAULT_MAX_OCEAN_DRIFT_ABS: f32 = 1e-4;
const DEFAULT_TRANSITION_PRE_END_TICK: u64 = 799;
const DEFAULT_TRANSITION_POST_START_TICK: u64 = 800;
const DEFAULT_TRANSITION_POST_END_TICK: u64 = 840;
const DEFAULT_MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT: f32 = 0.04;
const DEFAULT_MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT: f32 = 0.10;
const DEFAULT_MAX_MASS_PROXY_DRIFT_ABS: f32 = 1e-3;
const DEFAULT_MAX_MASS_PROXY_DRIFT_RATIO: f32 = 0.02;
const DEFAULT_MAX_RENDER_LAND_RATIO_DIFF: f32 = 0.02;
const DEFAULT_MAX_SEA_LEVEL_SLOPE: f32 = 0.02;
const DEFAULT_MAX_LARGEST_CONTINENT_RATIO_JUMP: f32 = 0.10;
const DEFAULT_MAX_COASTAL_BAND_RATIO: f32 = 0.12;
const DEFAULT_MAX_LAND_FREEBOARD_P90: f32 = 0.40;

#[derive(Debug, Clone)]
struct BenchConfig {
    seed: String,
    level: u32,
    run_ticks: u64,
    record_start_tick: u64,
    record_end_tick: u64,
    out_path: PathBuf,
    land_ratio_min: f32,
    land_ratio_max: f32,
    max_land_ratio_jump: f32,
    max_sea_level_jump: f32,
    max_ocean_drift_abs: f32,
    transition_pre_end_tick: u64,
    transition_post_start_tick: u64,
    transition_post_end_tick: u64,
    max_transition_land_ratio_median_shift: f32,
    max_transition_sea_level_median_shift: f32,
    max_mass_proxy_drift_abs: f32,
    max_mass_proxy_drift_ratio: f32,
    max_render_land_ratio_diff: f32,
    max_sea_level_slope: f32,
    max_largest_continent_ratio_jump: f32,
    max_coastal_band_ratio: f32,
    max_land_freeboard_p90: f32,
}

#[derive(Debug, Clone, Serialize)]
struct TickRecord {
    tick: u64,
    era: String,
    land_cells: u32,
    land_ratio: f32,
    bedrock_land_ratio: f32,
    sea_level_offset: f32,
    ocean_water_inventory_drift: f32,
    ice_inventory: f32,
    mass_proxy_total: f32,
    water_mass_closure_drift: f32,
    render_land_ratio: f32,
    render_land_ratio_diff: f32,
    zero_level_land_ratio: f32,
    zero_level_land_ratio_diff: f32,
    continent_count: u32,
    largest_continent_cells: u32,
    largest_continent_ratio: f32,
    sea_level_slope: f32,
    land_ratio_slope: f32,
    coastal_band_ratio: f32,
    bedrock_coastal_band_ratio: f32,
    land_freeboard_p10: f32,
    land_freeboard_p50: f32,
    land_freeboard_p90: f32,
    bedrock_freeboard_p10: f32,
    bedrock_freeboard_p50: f32,
    bedrock_freeboard_p90: f32,
    geology_runtime_bedrock_band_ratio: f32,
    geology_runtime_bedrock_p10: f32,
    geology_runtime_bedrock_p50: f32,
    geology_runtime_bedrock_p90: f32,
    geology_runtime_activity_scale: f32,
    geology_runtime_rebuild_applied: f32,
    geology_runtime_mean_abs_surface_write_delta: f32,
    geology_runtime_mean_compressive: f32,
    geology_runtime_mean_tensile: f32,
    geology_runtime_mean_signed_surface_write_delta: f32,
    geology_runtime_min_surface_write_delta: f32,
    geology_runtime_max_surface_write_delta: f32,
    geology_runtime_mean_abs_surface_range_clamp_delta: f32,
    geology_runtime_mean_abs_surface_raw_delta: f32,
    geology_runtime_mean_abs_surface_step_delta: f32,
    geology_runtime_mean_abs_surface_step_clamp_delta: f32,
    geology_runtime_mean_abs_surface_pre_isostatic_delta: f32,
    geology_runtime_mean_abs_surface_output_delta: f32,
    geology_runtime_mean_abs_surface_pre_zero_mean_delta: f32,
    geology_runtime_mean_abs_surface_zero_mean_delta: f32,
    geology_runtime_debug_surface_max_delta_index: f32,
    geology_runtime_debug_surface_max_delta_raw_delta: f32,
    geology_runtime_debug_surface_max_delta_step_delta: f32,
    geology_runtime_debug_surface_max_delta_thermal_subsidence: f32,
    geology_runtime_debug_surface_max_delta_diffusive: f32,
    geology_runtime_debug_surface_max_delta_uplift: f32,
    geology_runtime_debug_surface_max_delta_tectonic_subsidence: f32,
    geology_runtime_debug_surface_max_delta_tensile: f32,
    geology_runtime_debug_surface_max_delta_stress: f32,
    geology_runtime_debug_surface_max_delta_height_before: f32,
    geology_runtime_debug_surface_max_delta_height_after_pre_isostatic: f32,
    geology_runtime_mean_abs_tectonic_uplift: f32,
    geology_runtime_mean_abs_volcanic_uplift: f32,
    geology_runtime_mean_abs_tectonic_subsidence: f32,
    geology_runtime_mean_abs_thermal_subsidence: f32,
    geology_runtime_mean_abs_thickness_equilibrium_gap: f32,
    geology_runtime_mean_abs_isostatic_equilibrium_gap: f32,
    geology_runtime_mean_abs_isostatic_reference_freeboard: f32,
    geology_runtime_mean_abs_isostatic_compensated_anomaly: f32,
    geology_runtime_mean_density_ratio: f32,
    geology_runtime_mean_abs_diffusive_raw: f32,
    geology_runtime_mean_abs_diffusive_applied: f32,
    geology_runtime_mean_abs_diffusive_land_down_raw: f32,
    geology_runtime_mean_abs_diffusive_land_up_raw: f32,
    geology_runtime_mean_abs_diffusive_ocean_down_raw: f32,
    geology_runtime_mean_abs_diffusive_ocean_up_raw: f32,
    geology_runtime_mean_abs_diffusive_ocean_up_applied: f32,
    geology_runtime_mean_abs_isostatic_raw: f32,
    geology_runtime_mean_abs_isostatic_applied: f32,
    geology_runtime_mean_abs_isostatic_reference_freeboard_applied: f32,
    geology_runtime_mean_abs_isostatic_compensated_anomaly_applied: f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_oceanic: f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental: f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_orogenic: f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable: f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_rift: f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_transform:
        f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_margin:
        f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_transform:
        f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_raw_continental_stable_passive_margin:
        f32,
    geology_runtime_mean_signed_isostatic_reference_freeboard_raw_continental_stable_transform: f32,
    geology_runtime_passive_margin_continental_cell_ratio: f32,
    geology_runtime_mean_passive_margin_isostatic_adjustment_rate: f32,
    geology_runtime_mean_passive_margin_smoothing_factor: f32,
    geology_runtime_passive_margin_reference_freeboard_effective_applied_factor: f32,
    geology_runtime_smoothing_limited_cells_ratio: f32,
    geology_runtime_mean_smoothing_factor: f32,
    geology_runtime_zero_mean_adjusted_cells_ratio: f32,
    geology_runtime_zero_mean_mean_abs_correction: f32,
    geology_runtime_zero_mean_std_delta: f32,
    geology_runtime_crust_recentering_shift: f32,
    geology_runtime_crust_recentering_pre_band_ratio: f32,
    geology_runtime_crust_recentering_post_band_ratio: f32,
    geology_stage_mean_abs_height_delta: f32,
    glaciology_stage_mean_abs_height_delta: f32,
    hydrology_stage_mean_abs_height_delta: f32,
    hypsometry_bins: [u32; 8],
}

#[derive(Debug, Clone, Copy, Default)]
struct TickStageDiagnostics {
    geology_stage_mean_abs_height_delta: f32,
    glaciology_stage_mean_abs_height_delta: f32,
    hydrology_stage_mean_abs_height_delta: f32,
}

#[derive(Debug, Clone, Serialize)]
struct ViolationRecord {
    tick: u64,
    kind: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct BenchRecord {
    benchmark: String,
    run_id: String,
    seed: String,
    level: u32,
    run_ticks: u64,
    record_start_tick: u64,
    record_end_tick: u64,
    land_ratio_min: f32,
    land_ratio_max: f32,
    max_land_ratio_jump: f32,
    max_sea_level_jump: f32,
    max_ocean_drift_abs: f32,
    transition_pre_end_tick: u64,
    transition_post_start_tick: u64,
    transition_post_end_tick: u64,
    max_transition_land_ratio_median_shift: f32,
    max_transition_sea_level_median_shift: f32,
    max_mass_proxy_drift_abs: f32,
    max_mass_proxy_drift_ratio: f32,
    max_render_land_ratio_diff: f32,
    max_sea_level_slope: f32,
    max_largest_continent_ratio_jump: f32,
    max_coastal_band_ratio: f32,
    max_land_freeboard_p90: f32,
    resume_from_snapshot_stage: Option<String>,
    resume_from_snapshot_tick: Option<u64>,
    samples: Vec<TickRecord>,
    violations: Vec<ViolationRecord>,
    warnings: Vec<ViolationRecord>,
}

fn main() {
    let config = load_config();
    let run_id = default_run_id();
    let record = run_benchmark(&config, run_id.clone());
    if let Err(err) = append_jsonl(&config.out_path, &record) {
        panic!("failed to write benchmark artifact: {err}");
    }
    if !record.violations.is_empty() {
        panic!(
            "alpha_transition_guard failed: {} violations (run_id={})",
            record.violations.len(),
            run_id
        );
    }
    println!(
        "alpha_transition_guard: PASS (samples={}, run_id={})",
        record.samples.len(),
        run_id
    );
}

fn load_config() -> BenchConfig {
    let seed = env::var("ALPHA_TRANSITION_SEED")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env_u32("ALPHA_TRANSITION_LEVEL").unwrap_or(DEFAULT_LEVEL);
    let run_ticks = env_u64("ALPHA_TRANSITION_TICKS").unwrap_or(DEFAULT_RUN_TICKS);
    let record_start_tick =
        env_u64("ALPHA_TRANSITION_RECORD_START").unwrap_or(DEFAULT_RECORD_START_TICK);
    let record_end_tick = env_u64("ALPHA_TRANSITION_RECORD_END").unwrap_or(DEFAULT_RECORD_END_TICK);
    let out_path = env::var("ALPHA_TRANSITION_BENCH_OUT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("benches/results/alpha_transition_guard/alpha_transition_guard.jsonl")
        });
    let land_ratio_min =
        env_f32("ALPHA_TRANSITION_LAND_RATIO_MIN").unwrap_or(DEFAULT_LAND_RATIO_MIN);
    let land_ratio_max =
        env_f32("ALPHA_TRANSITION_LAND_RATIO_MAX").unwrap_or(DEFAULT_LAND_RATIO_MAX);
    let max_land_ratio_jump =
        env_f32("ALPHA_TRANSITION_MAX_LAND_RATIO_JUMP").unwrap_or(DEFAULT_MAX_LAND_RATIO_JUMP);
    let max_sea_level_jump =
        env_f32("ALPHA_TRANSITION_MAX_SEA_LEVEL_JUMP").unwrap_or(DEFAULT_MAX_SEA_LEVEL_JUMP);
    let max_ocean_drift_abs =
        env_f32("ALPHA_TRANSITION_MAX_OCEAN_DRIFT_ABS").unwrap_or(DEFAULT_MAX_OCEAN_DRIFT_ABS);
    let transition_pre_end_tick =
        env_u64("ALPHA_TRANSITION_PRE_END_TICK").unwrap_or(DEFAULT_TRANSITION_PRE_END_TICK);
    let transition_post_start_tick =
        env_u64("ALPHA_TRANSITION_POST_START_TICK").unwrap_or(DEFAULT_TRANSITION_POST_START_TICK);
    let transition_post_end_tick =
        env_u64("ALPHA_TRANSITION_POST_END_TICK").unwrap_or(DEFAULT_TRANSITION_POST_END_TICK);
    let max_transition_land_ratio_median_shift =
        env_f32("ALPHA_TRANSITION_MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT")
            .unwrap_or(DEFAULT_MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT);
    let max_transition_sea_level_median_shift =
        env_f32("ALPHA_TRANSITION_MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT")
            .unwrap_or(DEFAULT_MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT);
    let max_mass_proxy_drift_abs = env_f32("ALPHA_TRANSITION_MAX_MASS_PROXY_DRIFT_ABS")
        .unwrap_or(DEFAULT_MAX_MASS_PROXY_DRIFT_ABS);
    let max_mass_proxy_drift_ratio = env_f32("ALPHA_TRANSITION_MAX_MASS_PROXY_DRIFT_RATIO")
        .unwrap_or(DEFAULT_MAX_MASS_PROXY_DRIFT_RATIO);
    let max_render_land_ratio_diff = env_f32("ALPHA_TRANSITION_MAX_RENDER_LAND_RATIO_DIFF")
        .unwrap_or(DEFAULT_MAX_RENDER_LAND_RATIO_DIFF);
    let max_sea_level_slope =
        env_f32("ALPHA_TRANSITION_MAX_SEA_LEVEL_SLOPE").unwrap_or(DEFAULT_MAX_SEA_LEVEL_SLOPE);
    let max_largest_continent_ratio_jump =
        env_f32("ALPHA_TRANSITION_MAX_LARGEST_CONTINENT_RATIO_JUMP")
            .unwrap_or(DEFAULT_MAX_LARGEST_CONTINENT_RATIO_JUMP);
    let max_coastal_band_ratio = env_f32("ALPHA_TRANSITION_MAX_COASTAL_BAND_RATIO")
        .unwrap_or(DEFAULT_MAX_COASTAL_BAND_RATIO);
    let max_land_freeboard_p90 = env_f32("ALPHA_TRANSITION_MAX_LAND_FREEBOARD_P90")
        .unwrap_or(DEFAULT_MAX_LAND_FREEBOARD_P90);
    BenchConfig {
        seed,
        level,
        run_ticks,
        record_start_tick,
        record_end_tick,
        out_path,
        land_ratio_min,
        land_ratio_max,
        max_land_ratio_jump,
        max_sea_level_jump,
        max_ocean_drift_abs,
        transition_pre_end_tick,
        transition_post_start_tick,
        transition_post_end_tick,
        max_transition_land_ratio_median_shift,
        max_transition_sea_level_median_shift,
        max_mass_proxy_drift_abs,
        max_mass_proxy_drift_ratio,
        max_render_land_ratio_diff,
        max_sea_level_slope,
        max_largest_continent_ratio_jump,
        max_coastal_band_ratio,
        max_land_freeboard_p90,
    }
}

fn run_benchmark(config: &BenchConfig, run_id: String) -> BenchRecord {
    let geology_params = GeologyParams {
        level: config.level,
        ..GeologyParams::default()
    };
    let mut warnings = Vec::new();
    let (mut world, erosion_state) = sim::headless::init_world_for_headless_runner(
        &config.seed,
        config.level,
        geology_params.clone(),
    )
    .unwrap_or_else(|err| panic!("failed to init world: {err}"));
    let mut hydrology_state = Some(erosion_state);
    let mut feedback = FeedbackQueue::new(world.cell_count());

    let mut samples = Vec::new();
    let mut violations = Vec::new();
    let mut prev_sample: Option<TickRecord> = None;
    let mut mass_proxy_baseline: Option<f32> = None;
    let mut stage_diagnostics = TickStageDiagnostics::default();

    maybe_record_sample(
        &world,
        config,
        &mut samples,
        &mut violations,
        &mut prev_sample,
        &mut mass_proxy_baseline,
        stage_diagnostics,
    );

    let steps_to_run = config.run_ticks.saturating_sub(world.clock.tick);
    for _ in 0..steps_to_run {
        stage_diagnostics =
            exec_tick_with_stage_diagnostics(&mut world, &mut feedback, &mut hydrology_state);
        post_step_sync_light(&mut world, hydrology_state.as_mut(), &geology_params);
        maybe_record_sample(
            &world,
            config,
            &mut samples,
            &mut violations,
            &mut prev_sample,
            &mut mass_proxy_baseline,
            stage_diagnostics,
        );
    }
    validate_transition_continuity(config, &samples, &mut warnings);

    BenchRecord {
        benchmark: "alpha_transition_guard".to_string(),
        run_id,
        seed: config.seed.clone(),
        level: config.level,
        run_ticks: config.run_ticks,
        record_start_tick: config.record_start_tick,
        record_end_tick: config.record_end_tick,
        land_ratio_min: config.land_ratio_min,
        land_ratio_max: config.land_ratio_max,
        max_land_ratio_jump: config.max_land_ratio_jump,
        max_sea_level_jump: config.max_sea_level_jump,
        max_ocean_drift_abs: config.max_ocean_drift_abs,
        transition_pre_end_tick: config.transition_pre_end_tick,
        transition_post_start_tick: config.transition_post_start_tick,
        transition_post_end_tick: config.transition_post_end_tick,
        max_transition_land_ratio_median_shift: config.max_transition_land_ratio_median_shift,
        max_transition_sea_level_median_shift: config.max_transition_sea_level_median_shift,
        max_mass_proxy_drift_abs: config.max_mass_proxy_drift_abs,
        max_mass_proxy_drift_ratio: config.max_mass_proxy_drift_ratio,
        max_render_land_ratio_diff: config.max_render_land_ratio_diff,
        max_sea_level_slope: config.max_sea_level_slope,
        max_largest_continent_ratio_jump: config.max_largest_continent_ratio_jump,
        max_coastal_band_ratio: config.max_coastal_band_ratio,
        max_land_freeboard_p90: config.max_land_freeboard_p90,
        resume_from_snapshot_stage: None,
        resume_from_snapshot_tick: None,
        samples,
        violations,
        warnings,
    }
}

fn maybe_record_sample(
    world: &World,
    config: &BenchConfig,
    samples: &mut Vec<TickRecord>,
    violations: &mut Vec<ViolationRecord>,
    prev_sample: &mut Option<TickRecord>,
    mass_proxy_baseline: &mut Option<f32>,
    stage_diagnostics: TickStageDiagnostics,
) {
    let tick = world.clock.tick;
    if tick < config.record_start_tick || tick > config.record_end_tick {
        return;
    }
    let sample = build_tick_record(
        world,
        prev_sample.as_ref(),
        mass_proxy_baseline,
        config,
        stage_diagnostics,
    );
    evaluate_sample(
        config,
        &sample,
        prev_sample.as_ref(),
        mass_proxy_baseline,
        violations,
    );
    *prev_sample = Some(sample.clone());
    samples.push(sample);
}

fn build_tick_record(
    world: &World,
    prev_sample: Option<&TickRecord>,
    mass_proxy_baseline: &mut Option<f32>,
    config: &BenchConfig,
    stage_diagnostics: TickStageDiagnostics,
) -> TickRecord {
    let metrics = world.metrics();
    let render_land_ratio = render_land_ratio(world);
    let zero_level_land_ratio = zero_level_land_ratio(world);
    let mass_proxy_total = metrics.ocean_water_inventory
        + BenchGlaciologyParams::default().sea_level_coupling.max(0.0) * metrics.ice_inventory;
    if mass_proxy_baseline.is_none() && world.clock.tick >= config.transition_post_start_tick {
        *mass_proxy_baseline = Some(mass_proxy_total);
    }
    let mass_proxy_drift = mass_proxy_baseline
        .map(|baseline| mass_proxy_total - baseline)
        .unwrap_or(0.0);
    let largest_continent_ratio = if metrics.cell_count > 0 {
        metrics.largest_continent_cells as f32 / metrics.cell_count as f32
    } else {
        0.0
    };
    let (sea_level_slope, land_ratio_slope) = if let Some(prev) = prev_sample {
        (
            sample_delta(metrics.sea_level_offset, prev.sea_level_offset),
            sample_delta(metrics.land_ratio, prev.land_ratio),
        )
    } else {
        (0.0, 0.0)
    };

    let bedrock_land_ratio = bedrock_land_ratio(world);
    let bedrock_band_ratio =
        bedrock_coastal_band_ratio(world, world.control.sea_level_offset, 0.02);
    let (land_freeboard_p10, land_freeboard_p50, land_freeboard_p90) =
        land_freeboard_percentiles(world);
    let (bedrock_freeboard_p10, bedrock_freeboard_p50, bedrock_freeboard_p90) =
        bedrock_freeboard_percentiles(world);
    let runtime_metrics = world
        .exec_scratch
        .geology_dynamics
        .as_ref()
        .map(|state| state.cached_metrics)
        .unwrap_or_default();

    TickRecord {
        tick: world.clock.tick,
        era: world.clock.epoch.as_key().to_string(),
        land_cells: metrics.land_cells,
        land_ratio: metrics.land_ratio,
        bedrock_land_ratio,
        sea_level_offset: metrics.sea_level_offset,
        ocean_water_inventory_drift: metrics.ocean_water_inventory_drift,
        ice_inventory: metrics.ice_inventory,
        mass_proxy_total,
        water_mass_closure_drift: mass_proxy_drift,
        render_land_ratio,
        render_land_ratio_diff: (metrics.land_ratio - render_land_ratio).abs(),
        zero_level_land_ratio,
        zero_level_land_ratio_diff: (metrics.land_ratio - zero_level_land_ratio).abs(),
        continent_count: metrics.continent_count,
        largest_continent_cells: metrics.largest_continent_cells,
        largest_continent_ratio,
        sea_level_slope,
        land_ratio_slope,
        coastal_band_ratio: coastal_band_ratio(world, world.control.sea_level_offset, 0.02),
        bedrock_coastal_band_ratio: bedrock_band_ratio,
        land_freeboard_p10,
        land_freeboard_p50,
        land_freeboard_p90,
        bedrock_freeboard_p10,
        bedrock_freeboard_p50,
        bedrock_freeboard_p90,
        geology_runtime_bedrock_band_ratio: runtime_metrics.bedrock_zero_level_coastal_band_ratio,
        geology_runtime_bedrock_p10: runtime_metrics.bedrock_freeboard_p10,
        geology_runtime_bedrock_p50: runtime_metrics.bedrock_freeboard_p50,
        geology_runtime_bedrock_p90: runtime_metrics.bedrock_freeboard_p90,
        geology_runtime_activity_scale: runtime_metrics.activity_scale,
        geology_runtime_rebuild_applied: runtime_metrics.runtime_rebuild_applied,
        geology_runtime_mean_abs_surface_write_delta: runtime_metrics.mean_abs_surface_write_delta,
        geology_runtime_mean_compressive: runtime_metrics.mean_compressive,
        geology_runtime_mean_tensile: runtime_metrics.mean_tensile,
        geology_runtime_mean_signed_surface_write_delta: runtime_metrics
            .mean_signed_surface_write_delta,
        geology_runtime_min_surface_write_delta: runtime_metrics.min_surface_write_delta,
        geology_runtime_max_surface_write_delta: runtime_metrics.max_surface_write_delta,
        geology_runtime_mean_abs_surface_range_clamp_delta: runtime_metrics
            .mean_abs_surface_range_clamp_delta,
        geology_runtime_mean_abs_surface_raw_delta: runtime_metrics.mean_abs_surface_raw_delta,
        geology_runtime_mean_abs_surface_step_delta: runtime_metrics
            .mean_abs_surface_step_delta,
        geology_runtime_mean_abs_surface_step_clamp_delta: runtime_metrics
            .mean_abs_surface_step_clamp_delta,
        geology_runtime_mean_abs_surface_pre_isostatic_delta: runtime_metrics
            .mean_abs_surface_pre_isostatic_delta,
        geology_runtime_mean_abs_surface_output_delta: runtime_metrics
            .mean_abs_surface_output_delta,
        geology_runtime_mean_abs_surface_pre_zero_mean_delta: runtime_metrics
            .mean_abs_surface_pre_zero_mean_delta,
        geology_runtime_mean_abs_surface_zero_mean_delta: runtime_metrics
            .mean_abs_surface_zero_mean_delta,
        geology_runtime_debug_surface_max_delta_index: runtime_metrics
            .debug_surface_max_delta_index,
        geology_runtime_debug_surface_max_delta_raw_delta: runtime_metrics
            .debug_surface_max_delta_raw_delta,
        geology_runtime_debug_surface_max_delta_step_delta: runtime_metrics
            .debug_surface_max_delta_step_delta,
        geology_runtime_debug_surface_max_delta_thermal_subsidence: runtime_metrics
            .debug_surface_max_delta_thermal_subsidence,
        geology_runtime_debug_surface_max_delta_diffusive: runtime_metrics
            .debug_surface_max_delta_diffusive,
        geology_runtime_debug_surface_max_delta_uplift: runtime_metrics
            .debug_surface_max_delta_uplift,
        geology_runtime_debug_surface_max_delta_tectonic_subsidence: runtime_metrics
            .debug_surface_max_delta_tectonic_subsidence,
        geology_runtime_debug_surface_max_delta_tensile: runtime_metrics
            .debug_surface_max_delta_tensile,
        geology_runtime_debug_surface_max_delta_stress: runtime_metrics
            .debug_surface_max_delta_stress,
        geology_runtime_debug_surface_max_delta_height_before: runtime_metrics
            .debug_surface_max_delta_height_before,
        geology_runtime_debug_surface_max_delta_height_after_pre_isostatic: runtime_metrics
            .debug_surface_max_delta_height_after_pre_isostatic,
        geology_runtime_mean_abs_tectonic_uplift: runtime_metrics.mean_abs_tectonic_uplift,
        geology_runtime_mean_abs_volcanic_uplift: runtime_metrics.mean_abs_volcanic_uplift,
        geology_runtime_mean_abs_tectonic_subsidence: runtime_metrics.mean_abs_tectonic_subsidence,
        geology_runtime_mean_abs_thermal_subsidence: runtime_metrics.mean_abs_thermal_subsidence,
        geology_runtime_mean_abs_thickness_equilibrium_gap: runtime_metrics
            .mean_abs_thickness_equilibrium_gap,
        geology_runtime_mean_abs_isostatic_equilibrium_gap: runtime_metrics
            .mean_abs_isostatic_equilibrium_gap,
        geology_runtime_mean_abs_isostatic_reference_freeboard: runtime_metrics
            .mean_abs_isostatic_reference_freeboard,
        geology_runtime_mean_abs_isostatic_compensated_anomaly: runtime_metrics
            .mean_abs_isostatic_compensated_anomaly,
        geology_runtime_mean_density_ratio: runtime_metrics.mean_density_ratio,
        geology_runtime_mean_abs_diffusive_raw: runtime_metrics.mean_abs_diffusive_raw,
        geology_runtime_mean_abs_diffusive_applied: runtime_metrics.mean_abs_diffusive_applied,
        geology_runtime_mean_abs_diffusive_land_down_raw: runtime_metrics
            .mean_abs_diffusive_land_down_raw,
        geology_runtime_mean_abs_diffusive_land_up_raw: runtime_metrics
            .mean_abs_diffusive_land_up_raw,
        geology_runtime_mean_abs_diffusive_ocean_down_raw: runtime_metrics
            .mean_abs_diffusive_ocean_down_raw,
        geology_runtime_mean_abs_diffusive_ocean_up_raw: runtime_metrics
            .mean_abs_diffusive_ocean_up_raw,
        geology_runtime_mean_abs_diffusive_ocean_up_applied: runtime_metrics
            .mean_abs_diffusive_ocean_up_applied,
        geology_runtime_mean_abs_isostatic_raw: runtime_metrics.mean_abs_isostatic_raw,
        geology_runtime_mean_abs_isostatic_applied: runtime_metrics.mean_abs_isostatic_applied,
        geology_runtime_mean_abs_isostatic_reference_freeboard_applied: runtime_metrics
            .mean_abs_isostatic_reference_freeboard_applied,
        geology_runtime_mean_abs_isostatic_compensated_anomaly_applied: runtime_metrics
            .mean_abs_isostatic_compensated_anomaly_applied,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_oceanic: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_oceanic,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_continental,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_orogenic: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_continental_orogenic,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_continental_stable,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_rift: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_continental_stable_rift,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_transform: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_transform,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_margin: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_continental_stable_passive_margin,
        geology_runtime_mean_signed_isostatic_reference_freeboard_applied_continental_stable_transform: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_applied_continental_stable_transform,
        geology_runtime_mean_signed_isostatic_reference_freeboard_raw_continental_stable_passive_margin: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_raw_continental_stable_passive_margin,
        geology_runtime_mean_signed_isostatic_reference_freeboard_raw_continental_stable_transform: runtime_metrics
            .mean_signed_isostatic_reference_freeboard_raw_continental_stable_transform,
        geology_runtime_passive_margin_continental_cell_ratio: runtime_metrics
            .passive_margin_continental_cell_ratio,
        geology_runtime_mean_passive_margin_isostatic_adjustment_rate: runtime_metrics
            .mean_passive_margin_isostatic_adjustment_rate,
        geology_runtime_mean_passive_margin_smoothing_factor: runtime_metrics
            .mean_passive_margin_smoothing_factor,
        geology_runtime_passive_margin_reference_freeboard_effective_applied_factor: runtime_metrics
            .passive_margin_reference_freeboard_effective_applied_factor,
        geology_runtime_smoothing_limited_cells_ratio: runtime_metrics.smoothing_limited_cells_ratio,
        geology_runtime_mean_smoothing_factor: runtime_metrics.mean_smoothing_factor,
        geology_runtime_zero_mean_adjusted_cells_ratio: runtime_metrics
            .zero_mean_adjusted_cells_ratio,
        geology_runtime_zero_mean_mean_abs_correction: runtime_metrics
            .zero_mean_mean_abs_correction,
        geology_runtime_zero_mean_std_delta: runtime_metrics.zero_mean_std_delta,
        geology_runtime_crust_recentering_shift: runtime_metrics.crust_recentering_shift,
        geology_runtime_crust_recentering_pre_band_ratio: runtime_metrics
            .crust_recentering_pre_band_ratio,
        geology_runtime_crust_recentering_post_band_ratio: runtime_metrics
            .crust_recentering_post_band_ratio,
        geology_stage_mean_abs_height_delta: stage_diagnostics.geology_stage_mean_abs_height_delta,
        glaciology_stage_mean_abs_height_delta: stage_diagnostics
            .glaciology_stage_mean_abs_height_delta,
        hydrology_stage_mean_abs_height_delta: stage_diagnostics
            .hydrology_stage_mean_abs_height_delta,
        hypsometry_bins: hypsometry_bins(world),
    }
}

fn exec_tick_with_stage_diagnostics(
    world: &mut World,
    feedback: &mut FeedbackQueue,
    hydrology_state: &mut Option<ErosionAutomatonState>,
) -> TickStageDiagnostics {
    let mut diagnostics = TickStageDiagnostics::default();
    let mut phase = sim::first_phase();
    let starting_tick = world.clock.tick;

    while world.clock.tick == starting_tick {
        let before_height = world.state.geology.height.clone();
        let current_phase = phase;
        let result =
            sim::exec_world_slice_with_hydrology(world, feedback, hydrology_state, phase, 1);
        let delta = mean_abs_height_delta(&before_height, &world.state.geology.height);
        match current_phase {
            ExecWorldPhase::Geology => {
                diagnostics.geology_stage_mean_abs_height_delta = delta;
            }
            ExecWorldPhase::Glaciology => {
                diagnostics.glaciology_stage_mean_abs_height_delta = delta;
            }
            ExecWorldPhase::Hydrology => {
                diagnostics.hydrology_stage_mean_abs_height_delta = delta;
            }
            _ => {}
        }
        phase = result.next_phase;
    }

    diagnostics
}

fn mean_abs_height_delta(before: &[f32], after: &[f32]) -> f32 {
    let count = before.len().min(after.len());
    if count == 0 {
        return 0.0;
    }
    before
        .iter()
        .zip(after.iter())
        .take(count)
        .map(|(before, after)| (after - before).abs())
        .sum::<f32>()
        / count as f32
}

fn evaluate_sample(
    config: &BenchConfig,
    sample: &TickRecord,
    prev_sample: Option<&TickRecord>,
    mass_proxy_baseline: &Option<f32>,
    violations: &mut Vec<ViolationRecord>,
) {
    let baseline_for_ratio = mass_proxy_baseline
        .unwrap_or(sample.mass_proxy_total)
        .abs()
        .max(1.0);
    let mass_proxy_drift_ratio = sample.water_mass_closure_drift.abs() / baseline_for_ratio;
    if mass_proxy_baseline.is_some()
        && sample.water_mass_closure_drift.abs() > config.max_mass_proxy_drift_abs
        && mass_proxy_drift_ratio > config.max_mass_proxy_drift_ratio
    {
        violations.push(ViolationRecord {
            tick: sample.tick,
            kind: "mass_proxy_drift".to_string(),
            detail: format!(
                "drift={} abs_threshold={} ratio={} ratio_threshold={} (baseline={}, current={})",
                sample.water_mass_closure_drift,
                config.max_mass_proxy_drift_abs,
                mass_proxy_drift_ratio,
                config.max_mass_proxy_drift_ratio,
                baseline_for_ratio,
                sample.mass_proxy_total
            ),
        });
    }
    if sample.land_ratio < config.land_ratio_min || sample.land_ratio > config.land_ratio_max {
        violations.push(ViolationRecord {
            tick: sample.tick,
            kind: "land_ratio_out_of_range".to_string(),
            detail: format!(
                "land_ratio={} expected in [{}, {}]",
                sample.land_ratio, config.land_ratio_min, config.land_ratio_max
            ),
        });
    }
    if sample.render_land_ratio_diff > config.max_render_land_ratio_diff {
        violations.push(ViolationRecord {
            tick: sample.tick,
            kind: "render_land_ratio_diff".to_string(),
            detail: format!(
                "diff={} threshold={} (land_ratio={}, render_land_ratio={})",
                sample.render_land_ratio_diff,
                config.max_render_land_ratio_diff,
                sample.land_ratio,
                sample.render_land_ratio
            ),
        });
    }
    if sample.coastal_band_ratio > config.max_coastal_band_ratio {
        violations.push(ViolationRecord {
            tick: sample.tick,
            kind: "coastal_band_ratio".to_string(),
            detail: format!(
                "ratio={} threshold={}",
                sample.coastal_band_ratio, config.max_coastal_band_ratio
            ),
        });
    }
    if sample.land_freeboard_p90 > config.max_land_freeboard_p90 {
        violations.push(ViolationRecord {
            tick: sample.tick,
            kind: "land_freeboard_p90".to_string(),
            detail: format!(
                "p90={} threshold={}",
                sample.land_freeboard_p90, config.max_land_freeboard_p90
            ),
        });
    }
    if let Some(prev) = prev_sample {
        let land_jump = (sample.land_ratio - prev.land_ratio).abs();
        if land_jump > config.max_land_ratio_jump {
            violations.push(ViolationRecord {
                tick: sample.tick,
                kind: "land_ratio_jump".to_string(),
                detail: format!(
                    "delta={} threshold={} (prev={}, current={})",
                    land_jump, config.max_land_ratio_jump, prev.land_ratio, sample.land_ratio
                ),
            });
        }
        let sea_jump = (sample.sea_level_offset - prev.sea_level_offset).abs();
        if sea_jump > config.max_sea_level_jump {
            violations.push(ViolationRecord {
                tick: sample.tick,
                kind: "sea_level_jump".to_string(),
                detail: format!(
                    "delta={} threshold={} (prev={}, current={})",
                    sea_jump,
                    config.max_sea_level_jump,
                    prev.sea_level_offset,
                    sample.sea_level_offset
                ),
            });
        }
        if sample.sea_level_slope.abs() > config.max_sea_level_slope {
            violations.push(ViolationRecord {
                tick: sample.tick,
                kind: "sea_level_slope".to_string(),
                detail: format!(
                    "slope={} threshold={} (prev={}, current={})",
                    sample.sea_level_slope,
                    config.max_sea_level_slope,
                    prev.sea_level_offset,
                    sample.sea_level_offset
                ),
            });
        }
        let largest_continent_jump =
            (sample.largest_continent_ratio - prev.largest_continent_ratio).abs();
        if largest_continent_jump > config.max_largest_continent_ratio_jump {
            violations.push(ViolationRecord {
                tick: sample.tick,
                kind: "largest_continent_ratio_jump".to_string(),
                detail: format!(
                    "delta={} threshold={} (prev={}, current={})",
                    largest_continent_jump,
                    config.max_largest_continent_ratio_jump,
                    prev.largest_continent_ratio,
                    sample.largest_continent_ratio
                ),
            });
        }
    }
}

fn validate_transition_continuity(
    config: &BenchConfig,
    samples: &[TickRecord],
    violations: &mut Vec<ViolationRecord>,
) {
    let pre: Vec<&TickRecord> = samples
        .iter()
        .filter(|s| s.tick <= config.transition_pre_end_tick)
        .collect();
    let post: Vec<&TickRecord> = samples
        .iter()
        .filter(|s| {
            s.tick >= config.transition_post_start_tick && s.tick <= config.transition_post_end_tick
        })
        .collect();
    if pre.is_empty() || post.is_empty() {
        return;
    }
    let pre_land = median(pre.iter().map(|s| s.land_ratio).collect());
    let post_land = median(post.iter().map(|s| s.land_ratio).collect());
    let land_shift = (post_land - pre_land).abs();
    if land_shift > config.max_transition_land_ratio_median_shift {
        violations.push(ViolationRecord {
            tick: config.transition_post_start_tick,
            kind: "transition_land_ratio_median_shift".to_string(),
            detail: format!(
                "median_shift={} threshold={} (pre={}, post={}, window_pre<= {}, window_post=[{}, {}])",
                land_shift,
                config.max_transition_land_ratio_median_shift,
                pre_land,
                post_land,
                config.transition_pre_end_tick,
                config.transition_post_start_tick,
                config.transition_post_end_tick
            ),
        });
    }
    let pre_sea = median(pre.iter().map(|s| s.sea_level_offset).collect());
    let post_sea = median(post.iter().map(|s| s.sea_level_offset).collect());
    let sea_shift = (post_sea - pre_sea).abs();
    if sea_shift > config.max_transition_sea_level_median_shift {
        violations.push(ViolationRecord {
            tick: config.transition_post_start_tick,
            kind: "transition_sea_level_median_shift".to_string(),
            detail: format!(
                "median_shift={} threshold={} (pre={}, post={}, window_pre<= {}, window_post=[{}, {}])",
                sea_shift,
                config.max_transition_sea_level_median_shift,
                pre_sea,
                post_sea,
                config.transition_pre_end_tick,
                config.transition_post_start_tick,
                config.transition_post_end_tick
            ),
        });
    }
}

fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) * 0.5
    } else {
        values[mid]
    }
}

fn render_land_ratio(world: &World) -> f32 {
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 {
        return 0.0;
    }
    let land = (0..cell_count)
        .filter(|index| world.is_land_cell(*index))
        .count();
    land as f32 / cell_count as f32
}

fn zero_level_land_ratio(world: &World) -> f32 {
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 {
        return 0.0;
    }
    let land = (0..cell_count)
        .filter(|index| {
            world
                .surface_elevation(*index)
                .map(|surface_elevation| surface_elevation > 0.0)
                .unwrap_or(false)
        })
        .count();
    land as f32 / cell_count as f32
}

fn bedrock_land_ratio(world: &World) -> f32 {
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 {
        return 0.0;
    }
    let sea_level = world.control.sea_level_offset;
    let land = world
        .state
        .geology
        .height
        .iter()
        .filter(|&&height| height > sea_level)
        .count();
    land as f32 / cell_count as f32
}

fn coastal_band_ratio(world: &World, sea_level: f32, band: f32) -> f32 {
    let _ = sea_level;
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 {
        return 0.0;
    }
    let in_band = (0..cell_count)
        .filter(|index| {
            world
                .surface_elevation(*index)
                .map(|surface_elevation| surface_elevation.abs() <= band)
                .unwrap_or(false)
        })
        .count();
    in_band as f32 / cell_count as f32
}

fn bedrock_coastal_band_ratio(world: &World, sea_level: f32, band: f32) -> f32 {
    let cell_count = world.state.geology.height.len();
    if cell_count == 0 {
        return 0.0;
    }
    let in_band = world
        .state
        .geology
        .height
        .iter()
        .filter(|&&height| (height - sea_level).abs() <= band)
        .count();
    in_band as f32 / cell_count as f32
}

fn land_freeboard_percentiles(world: &World) -> (f32, f32, f32) {
    let mut freeboard = Vec::new();
    for index in 0..world.state.geology.height.len() {
        if let Some(surface_elevation) = world.surface_elevation(index) {
            if surface_elevation > 0.0 {
                freeboard.push(surface_elevation);
            }
        }
    }
    percentile_triplet(&mut freeboard)
}

fn bedrock_freeboard_percentiles(world: &World) -> (f32, f32, f32) {
    let sea_level = world.control.sea_level_offset;
    let mut freeboard = world
        .state
        .geology
        .height
        .iter()
        .filter_map(|&height| {
            let freeboard = height - sea_level;
            if freeboard > 0.0 {
                Some(freeboard)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    percentile_triplet(&mut freeboard)
}

fn sample_delta(current: f32, previous: f32) -> f32 {
    current - previous
}

fn hypsometry_bins(world: &World) -> [u32; 8] {
    let mut bins = [0_u32; 8];
    let cell_count = world.state.geology.height.len();
    for index in 0..cell_count {
        let surface = world.surface_elevation(index).unwrap_or(0.0);
        let bucket = if surface <= -0.20 {
            0
        } else if surface <= -0.10 {
            1
        } else if surface <= -0.02 {
            2
        } else if surface <= 0.02 {
            3
        } else if surface <= 0.10 {
            4
        } else if surface <= 0.20 {
            5
        } else if surface <= 0.40 {
            6
        } else {
            7
        };
        bins[bucket] = bins[bucket].saturating_add(1);
    }
    bins
}

fn percentile_triplet(values: &mut Vec<f32>) -> (f32, f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (
        percentile_sorted(values, 0.10),
        percentile_sorted(values, 0.50),
        percentile_sorted(values, 0.90),
    )
}

fn percentile_sorted(values: &[f32], quantile: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0];
    }
    let q = quantile.clamp(0.0, 1.0);
    let position = q * (values.len() - 1) as f32;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return values[lower];
    }
    let weight = position - lower as f32;
    values[lower] * (1.0 - weight) + values[upper] * weight
}

fn post_step_sync_light(
    world: &mut World,
    hydrology_state: Option<&mut ErosionAutomatonState>,
    params: &GeologyParams,
) {
    let Some(state) = hydrology_state else {
        return;
    };
    frey_wasm::sim::hydrology::sync_hydrology_state_for_headless_runner(world, state, params);
}

fn append_jsonl(path: &PathBuf, record: &BenchRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create directory {}: {err}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let line = serde_json::to_string(record)
        .map_err(|err| format!("failed to serialize benchmark record: {err}"))?;
    file.write_all(line.as_bytes())
        .map_err(|err| format!("failed to write record: {err}"))?;
    file.write_all(b"\n")
        .map_err(|err| format!("failed to write newline: {err}"))?;
    Ok(())
}

fn env_u32(key: &str) -> Option<u32> {
    env::var(key).ok()?.parse::<u32>().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    env::var(key).ok()?.parse::<u64>().ok()
}

fn env_f32(key: &str) -> Option<f32> {
    env::var(key).ok()?.parse::<f32>().ok()
}

fn default_run_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("alpha-transition-{now}")
}
